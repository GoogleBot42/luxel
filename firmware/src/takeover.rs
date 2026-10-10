//! Foreign-firmware → Luxel self-install ("takeover"), behind a per-board
//! takeover feature (docs/boards.md; ~25 KB, Gitea #501). On a device
//! already running a Luxel layout it costs one 256-byte flash read per boot
//! and does nothing.
//!
//! Two flavours share this module, one per board:
//!   * `wled-takeover` — the outgoing firmware is WLED. Settings come out of
//!     WLED's littlefs (src/wledfs.rs); WiFi creds live there too.
//!   * `pixelblaze-takeover` — the outgoing firmware is Pixelblaze. LED
//!     settings come out of Pixelblaze's SPIFFS `config.json` (src/pbfs.rs)
//!     and the WiFi creds out of its ESP-IDF NVS partition (src/pbnvs.rs),
//!     because a stock PB keeps no creds on its filesystem.
//! Everything else — noticing the foreign table, the verified self-copy,
//! the config wipe, writing the new table — is identical, so it lives in
//! one skeleton here. The only per-flavour code is [inherit_wifi] and
//! [inherit_device], which have exactly one definition each (the other
//! flavour's is `#[cfg]`-ed out), plus the reader each builds over the
//! outgoing firmware's partitions.
//!
//! The table-writing half of the skeleton — reading the live table, finding
//! our own image in a foreign slot, the verified self-copy, installing a
//! table — lives in [crate::parttab] and is built on EVERY board, because
//! [crate::migrate] needs exactly the same primitives to carry a device
//! from one Luxel layout to the next (Gitea #501).
//!
//! A Luxel app image delivered into one of the outgoing firmware's two app
//! slots boots under that firmware's partition table — ESP32 app images are
//! position-independent across slots (the bootloader MMU-maps whichever slot
//! otadata points at). On boot this module notices the foreign table, copies
//! the running image into what will become ota_0 (0x10000 — app0 has the
//! same offset in WLED's, Pixelblaze's and Luxel's 4 MB layouts), wipes the
//! config/otadata sectors, rewrites the partition table to Luxel's, and
//! reboots. The second-stage bootloader (the outgoing firmware's own — never
//! touched) reads the new table, finds otadata erased, and falls back to
//! ota_0: Luxel. The next boot sees a matching table and this module does
//! nothing.
//!
//! Crash-safety: every step before the final table rewrite leaves the old
//! table and otadata intact, so a power cut mid-copy just re-runs the
//! takeover on the next boot. The only serial-recovery-only window is the
//! single 4 KiB erase+write of the table sector itself (milliseconds).
//! If a takeover build crash-loops before getting this far, ota::preboot_guard
//! (which runs first, before the heap allocators) flips otadata back to the
//! old slot on the third failed boot — the device recovers to the stock
//! firmware on its own, even for a panic in the pre-heap window.
//!
//! Flake-safety (issue #35): an abort on anything that could be a
//! per-boot flash flake (the self-copy verify, the config wipe, a
//! descriptor read) reboots and retries, at most [TAKEOVER_TRIES] boots
//! total, before settling into the provisioning AP — the 2026-08-16 bench
//! conversion hit exactly one such flake and recovered on the next power
//! cycle, which this automates. Retry reboots are marked deliberate in
//! the guard record so they don't count toward preboot_guard's rollback.
//!
//! Settings inheritance: before touching anything, the takeover reads the
//! outgoing firmware's WiFi credentials and LED wiring and, after the config
//! wipe, writes them into Luxel's own records, so the device reappears on
//! the same network without ever being provisioned. A factory-fresh source
//! (or any parse failure) inherits nothing and Luxel boots its provisioning
//! AP instead — the fallback path, not an error.

use esp_println::println;

use crate::parttab::{self, Part, SECTOR, TABLE_OFFSET, TYPE_DATA};

/// nvs + boot-guard + otadata (+ phy_init) sectors under every layout we
/// convert from: wiped so Luxel starts from config defaults and the
/// bootloader's empty-otadata fallback picks ota_0. A WLED or Pixelblaze
/// device has nothing of ours in them; the Luxel→Luxel migrator deliberately
/// wipes NOTHING (it keeps the config it finds), which is why this range is
/// not in parttab. NB the inherited creds are read BEFORE this runs — on a
/// Pixelblaze the creds live inside this very range (NVS at 0x9000).
const CONFIG_WIPE: core::ops::Range<u32> = 0x9000..0x10000;

/// data/spiffs — what both WLED (littlefs) and Pixelblaze (SPIFFS) call
/// their filesystem partition.
const SUBTYPE_SPIFFS: u8 = 0x82;

/// The outgoing firmware's filesystem partition, if the foreign table has
/// one.
fn foreign_fs_part(live_table: &[u8]) -> Option<Part> {
    parttab::entries(live_table)
        .into_iter()
        .find(|p| p.ptype == TYPE_DATA && p.subtype == SUBTYPE_SPIFFS)
}

/// A reader over the outgoing firmware's filesystem partition, relative to
/// its start.
fn fs_reader(live_table: &[u8]) -> Option<(Part, impl FnMut(u32, &mut [u8]) -> bool)> {
    let fs_part = foreign_fs_part(live_table)?;
    let base = fs_part.offset;
    Some((fs_part, move |off: u32, buf: &mut [u8]| {
        off.checked_add(base)
            .is_some_and(|abs| crate::assets::read_chunk(abs, buf))
    }))
}

// ------------------------------------------------------------------------
// WLED flavour: creds + wiring out of WLED's littlefs (src/wledfs.rs).
// ------------------------------------------------------------------------

/// Best-effort WiFi inheritance from WLED's littlefs. Read-only; any failure
/// just returns None and the provisioning AP covers it.
#[cfg(feature = "wled-takeover")]
fn inherit_wifi(live_table: &[u8]) -> Option<(alloc::string::String, alloc::string::String)> {
    let (fs_part, mut read) = fs_reader(live_table)?;
    crate::wledfs::extract_wifi(&mut read, fs_part.len)
}

/// Best-effort LED wiring + defaults inheritance from the same cfg.json
/// (pixel count, strip type, color order, boot brightness, power cap,
/// gamma, data pin), mapped into a Luxel device-config record. Read-only,
/// None on any failure, and every field is individually optional — the
/// board defaults cover whatever is missing or unmappable.
#[cfg(feature = "wled-takeover")]
fn inherit_device(live_table: &[u8]) -> Option<crate::config::DeviceConfig> {
    let (fs_part, mut read) = fs_reader(live_table)?;
    let w = crate::wledfs::extract_wiring(&mut read, fs_part.len)?;

    let protocol = w.strip_type.and_then(crate::wledfs::map_strip_type);
    if let (Some(t), None) = (w.strip_type, protocol) {
        println!(
            "takeover: WLED strip type {} has no Luxel equivalent — keeping {}",
            t,
            crate::board::DEFAULT_PROTOCOL.name()
        );
    }
    // WLED's data pin is imported when this board can drive it (Gitea #154)
    // — the takeover reboots anyway, so the first Luxel boot binds the SPI
    // to it.
    let data_pin = match w.pin {
        Some(pin) if pin == crate::board::DEFAULT_DATA_PIN as i32 => None,
        Some(pin) if (0..64).contains(&pin) && crate::board::data_pin_ok(pin as u8) => {
            println!("takeover: WLED drove the strip on GPIO{} — importing as Luxel's data pin", pin);
            Some(pin as u8)
        }
        Some(pin) => {
            println!(
                "takeover: WLED drove the strip on GPIO{}, which this board reserves — keeping GPIO{}; rewire or pick another pin in Settings",
                pin,
                crate::board::DEFAULT_DATA_PIN
            );
            None
        }
        None => None,
    };
    Some(crate::config::DeviceConfig {
        // WLED boot brightness is 0-255; Luxel's is 0-31 (>31 voids the
        // record). Round, and floor at 1 so an imported config can never
        // look like a dead strip.
        brightness: w
            .bri
            .map(|b| (((b as u32) * 31 + 127) / 255).max(1) as u8)
            .unwrap_or(crate::APA_BRIGHTNESS),
        protocol: protocol.unwrap_or(crate::board::DEFAULT_PROTOCOL.as_u8()),
        sync_mode: 0,
        pixel_count: w
            .pixels
            .map(|p| p.min(crate::shared::MAX_PIXELS))
            .unwrap_or(crate::board::DEFAULT_PIXEL_COUNT),
        tz_minutes: 0,
        // Only meaningful relative to a protocol we actually mapped — a
        // guessed protocol would make the remap a color bug.
        color_order: protocol
            .and_then(|p| w.order.and_then(|o| crate::wledfs::map_color_order(o, p)))
            .unwrap_or(0),
        gamma_tenths: w.gamma_tenths.unwrap_or(0),
        cap_ma: w.cap_ma.unwrap_or(0),
        // WLED has no equivalent of the post-process chain — stay off.
        bright_curve_tenths: 0,
        blur_pct: 0,
        glow_pct: 0,
        data_pin,
    })
}

// ------------------------------------------------------------------------
// Pixelblaze flavour: WiFi out of ESP-IDF NVS (src/pbnvs.rs), LED settings
// out of SPIFFS config.json (src/pbfs.rs).
// ------------------------------------------------------------------------

/// nvs — ESP-IDF's key/value partition, where Pixelblaze (like every
/// ESP-IDF device) keeps the WiFi station credentials.
#[cfg(feature = "pixelblaze-takeover")]
const SUBTYPE_NVS: u8 = 0x02;

/// A reader over the outgoing firmware's NVS partition, relative to its
/// start. preboot_guard has already erased one 4 KiB page inside it (the
/// LXBG guard record at 0xC000) by the time this runs — pbnvs tolerates a
/// missing/garbage page and reads the creds from the survivors.
#[cfg(feature = "pixelblaze-takeover")]
fn nvs_reader(live_table: &[u8]) -> Option<(Part, impl FnMut(u32, &mut [u8]) -> bool)> {
    let part = parttab::entries(live_table)
        .into_iter()
        .find(|p| p.ptype == TYPE_DATA && p.subtype == SUBTYPE_NVS)?;
    let base = part.offset;
    Some((part, move |off: u32, buf: &mut [u8]| {
        off.checked_add(base)
            .is_some_and(|abs| crate::assets::read_chunk(abs, buf))
    }))
}

/// Best-effort WiFi inheritance from Pixelblaze's NVS. Read-only; None on
/// any failure and the provisioning AP covers it.
#[cfg(feature = "pixelblaze-takeover")]
fn inherit_wifi(live_table: &[u8]) -> Option<(alloc::string::String, alloc::string::String)> {
    let (nvs_part, mut read) = nvs_reader(live_table)?;
    crate::pbnvs::extract_wifi(&mut read, nvs_part.len)
}

/// Best-effort LED settings inheritance from Pixelblaze's `config.json`
/// (pixel count, LED type → protocol, color order, brightness), mapped into
/// a Luxel device-config record. None on any failure; board defaults cover
/// whatever is missing or unmappable.
///
/// Fields Luxel has and Pixelblaze's config.json does not: the data pin (PB
/// v3 hardwires DATA=GPIO23/CLK=GPIO18, which IS this board's default, so
/// nothing to import), a mA power cap, and the post-process chain — all left
/// at their defaults.
#[cfg(feature = "pixelblaze-takeover")]
fn inherit_device(live_table: &[u8]) -> Option<crate::config::DeviceConfig> {
    let (fs_part, mut read) = fs_reader(live_table)?;
    let w = crate::pbfs::extract_wiring(&mut read, fs_part.len)?;

    if w.protocol.is_none() {
        println!(
            "takeover: Pixelblaze LED type has no Luxel equivalent — keeping {}",
            crate::board::DEFAULT_PROTOCOL.name()
        );
    }
    // Pixelblaze shows what the strip was ACTUALLY running at: the main
    // slider (`brightness`) scaled by Settings → "Limit brightness"
    // (`maxBrightness`). Luxel has one brightness knob and no fractional
    // cap, so fold the two together — the converted strip comes up looking
    // the way it did, not twice as bright.
    let bri_255 = match (w.bri_255, w.max_bri_255) {
        (Some(b), Some(m)) if m != 255 => {
            let eff = ((b as u32) * (m as u32) + 127) / 255;
            println!(
                "takeover: Pixelblaze ran at brightness {}/255 × limit {}/255 → effective {}/255",
                b, m, eff
            );
            Some(eff as u8)
        }
        (Some(b), _) => Some(b),
        (None, _) => None,
    };
    Some(crate::config::DeviceConfig {
        // PB brightness is a 0-255 value here; Luxel's is 0-31 (>31 voids
        // the record). Round, and floor at 1 so an imported config can
        // never look like a dead strip.
        brightness: bri_255
            .map(|b| (((b as u32) * 31 + 127) / 255).max(1) as u8)
            .unwrap_or(crate::APA_BRIGHTNESS),
        protocol: w.protocol.unwrap_or(crate::board::DEFAULT_PROTOCOL.as_u8()),
        sync_mode: 0,
        pixel_count: w
            .pixels
            .map(|p| p.min(crate::shared::MAX_PIXELS))
            .unwrap_or(crate::board::DEFAULT_PIXEL_COUNT),
        tz_minutes: 0,
        // pbfs already resolved the color order against the mapped
        // protocol's native wire order (None whenever the protocol is).
        color_order: w.order.unwrap_or(0),
        // PB applies its own gamma inside the engine and exposes no gamma in
        // config.json; leave Luxel's output gamma off (the default).
        gamma_tenths: 0,
        cap_ma: 0,
        bright_curve_tenths: 0,
        blur_pct: 0,
        glow_pct: 0,
        // PB v3 hardwires the LED pins to this board's default.
        data_pin: None,
    })
}

/// Total boots that attempt the takeover before giving up and settling
/// into the provisioning AP (1 initial + 2 reboot-to-retries).
const TAKEOVER_TRIES: u8 = 3;

/// A takeover attempt aborted on something that might be a per-boot flash
/// flake (the 2026-08-16 bench conversion failed its first boot's
/// self-copy and ran clean on the next — issue #35): reboot to retry, at
/// most [TAKEOVER_TRIES] boots total, then settle into the provisioning
/// AP so a confused device never reboot-loops. The outgoing firmware's
/// table is intact in either case; a later power cycle gets a fresh retry
/// budget.
fn retry_or_settle(what: &str) {
    let done = crate::ota::takeover_retries().saturating_add(1); // incl. this boot
    if done < TAKEOVER_TRIES {
        crate::ota::bump_takeover_retries();
        println!(
            "takeover: {} — rebooting to retry ({}/{} attempts used)",
            what, done, TAKEOVER_TRIES
        );
        esp_hal::system::software_reset()
    }
    crate::ota::clear_takeover_retries();
    println!(
        "takeover: {} — giving up after {} attempts; provisioning AP will cover (stock table intact)",
        what, TAKEOVER_TRIES
    );
}

/// Called once at boot, after ota::init (and after ota::preboot_guard, which
/// runs before the heap allocators) and before any module that touches data
/// partitions. No-op (one 256-byte flash read) when the partition table is
/// already Luxel's.
pub fn maybe_takeover() {
    let Some(live) = parttab::live_table() else { return };
    if live == parttab::EMBEDDED {
        return;
    }
    // An OLDER LUXEL table is not a takeover: migrate::maybe_migrate moves
    // it (and carries the pattern store across) — the takeover's config
    // wipe would throw away the very credentials the device needs to come
    // back on the network (Gitea #501).
    if parttab::is_luxel(&live) {
        return;
    }
    println!("takeover: foreign partition table at {:#x} — installing Luxel layout", TABLE_OFFSET);

    // Flash-size preflight: never write a table that references flash the
    // chip doesn't have (a 2 MB part would brick on the first boot after).
    if !parttab::flash_fits(parttab::EMBEDDED) {
        return;
    }

    let Some(dest) = parttab::app_slot(parttab::EMBEDDED, parttab::SUBTYPE_OTA0) else {
        println!("takeover: no ota_0 in embedded table?! aborting");
        return;
    };

    // Inherit the outgoing firmware's WiFi credentials and LED settings
    // before anything is modified; both are persisted after the config
    // wipe below. The SOURCE and the field mapping are flavour-specific
    // (inherit_wifi/inherit_device have one definition per feature); the
    // rest of this function is identical for every flavour.
    let inherited = inherit_wifi(&live);
    match &inherited {
        Some((ssid, _)) => println!("takeover: inherited WiFi credentials for \"{}\"", ssid),
        None => println!("takeover: no WiFi credentials to inherit (provisioning AP will cover)"),
    }
    let device = inherit_device(&live);
    if device.is_none() {
        println!("takeover: no LED wiring to inherit (board defaults)");
    }

    // Candidate slots under the live (foreign) table. Destination offset
    // first: if a previous, interrupted takeover already copied the image
    // there — or the upload happened to land in app0 — skip the copy
    // entirely and never risk touching the region we run from.
    let Some(src) = parttab::find_own_slot(&live, dest.offset) else {
        // a Luxel image IS running under this foreign table, so "not
        // found" can only be a descriptor-read flake — retryable
        retry_or_settle("own image not found in any app slot");
        return;
    };

    if src.offset == dest.offset {
        println!("takeover: image already in place at {:#x}", dest.offset);
    } else {
        let Some(len) = parttab::image_len(src.offset) else {
            retry_or_settle("cannot size own image");
            return;
        };
        if len > dest.len {
            println!("takeover: image ({} B) exceeds ota_0 ({} B) — aborting", len, dest.len);
            return;
        }
        // Overlap guard: erasing the destination must never touch the
        // region we are executing from. Can't happen with today's layouts
        // (WLED, Pixelblaze and Luxel all put app0 at 0x10000, our slots
        // above it) — this protects future table changes that resize/move
        // app slots.
        if src.offset < dest.offset + dest.len && dest.offset < src.offset + len {
            println!(
                "takeover: running image {:#x}+{} overlaps destination {:#x}+{} — refusing",
                src.offset, len, dest.offset, dest.len
            );
            return;
        }
        println!("takeover: copying {} B {:#x} → {:#x}", len, src.offset, dest.offset);
        if !parttab::copy_region(src.offset, dest.offset, len, "takeover") {
            println!("takeover: copy failed — aborting before table rewrite (stock table intact)");
            retry_or_settle("self-copy failed");
            return;
        }
    }

    println!("takeover: wiping config/otadata sectors {:#x}..{:#x}", CONFIG_WIPE.start, CONFIG_WIPE.end);
    let mut at = CONFIG_WIPE.start;
    while at < CONFIG_WIPE.end {
        if !parttab::erase_sector(at) {
            println!("takeover: config wipe failed at {:#x} — aborting", at);
            // safe to re-run whole: the image is already in place, so the
            // retry boot short-circuits the copy and re-wipes from scratch
            retry_or_settle("config wipe failed");
            return;
        }
        at += SECTOR;
    }

    // Persist inherited credentials into Luxel's own record (after the
    // wipe, so it survives). Failure is non-fatal: the AP path remains.
    if let Some((ssid, pass)) = inherited {
        match crate::config::write_wifi(&ssid, &pass) {
            Ok(()) => println!("takeover: WiFi credentials carried over"),
            Err(e) => println!("takeover: creds carry-over failed ({}) — AP fallback", e),
        }
    }

    // Persist the inherited LED settings the same way: board defaults for
    // the rest. Failure is non-fatal: defaults cover, never a retry.
    if let Some(dev) = device {
        match crate::config::write_device(&dev) {
            Ok(()) => println!(
                "takeover: settings carried over ({} px, {}, order {}, brightness {}/31, cap {} mA, gamma {}.{})",
                dev.pixel_count,
                crate::leds::Protocol::from_u8(dev.protocol).name(),
                luxel_core::outpipe::ColorOrder(dev.color_order).name(),
                dev.brightness,
                dev.cap_ma,
                dev.gamma_tenths / 10,
                dev.gamma_tenths % 10
            ),
            Err(e) => println!("takeover: settings carry-over failed ({}) — board defaults", e),
        }
    }

    // The point of no return: one sector erase + write. Everything above
    // is re-runnable under the old table; after this line the new table is
    // authoritative and the bootloader boots ota_0.
    if !parttab::install(parttab::EMBEDDED, "takeover") {
        return;
    }
    println!("takeover: partition table installed — rebooting into Luxel");
    esp_hal::system::software_reset()
}

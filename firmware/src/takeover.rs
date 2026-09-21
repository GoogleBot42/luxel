//! WLED → Luxel self-install ("takeover"), behind the per-board
//! `wled-takeover` feature (docs/boards.md; ~25 KB, Gitea #501). On a
//! device already running a Luxel layout it costs one 256-byte flash read
//! per boot and does nothing.
//!
//! The table-writing half of this — reading the live table, finding our own
//! image in a foreign slot, the verified self-copy, installing a table —
//! lives in [crate::parttab] and is built on EVERY board, because
//! [crate::migrate] needs exactly the same primitives to carry a device
//! from one Luxel layout to the next (Gitea #501). What stays here is the
//! WLED-specific half: littlefs, credential and wiring inheritance, and the
//! config wipe.
//!
//! A Luxel app image uploaded through WLED's own OTA updater lands in one
//! of WLED's two 1.5 MB app slots and boots under WLED's partition table —
//! ESP32 app images are position-independent across slots (the bootloader
//! MMU-maps whichever slot otadata points at), and WLED's updater accepts
//! any image with the 0xE9 magic. On boot this module notices the foreign
//! table, copies the running image into what will become ota_0 (0x10000 —
//! app0 has the same offset in both layouts), wipes the config/otadata
//! sectors, rewrites the partition table to Luxel's, and reboots. The
//! second-stage bootloader (WLED's own — never touched) reads the new
//! table, finds otadata erased, and falls back to ota_0: Luxel. The next
//! boot sees a matching table and this module does nothing.
//!
//! Crash-safety: every step before the final table rewrite leaves WLED's
//! table and otadata intact, so a power cut mid-copy just re-runs the
//! takeover on the next boot. The only serial-recovery-only window is the
//! single 4 KiB erase+write of the table sector itself (milliseconds).
//! If a takeover build crash-loops before getting this far, ota::preboot_guard
//! (which runs first, before the heap allocators) flips otadata back to the
//! WLED slot on the third failed boot — the device recovers to stock WLED on
//! its own, even for a panic in the pre-heap window.
//!
//! Flake-safety (issue #35): an abort on anything that could be a
//! per-boot flash flake (the self-copy verify, the config wipe, a
//! descriptor read) reboots and retries, at most [TAKEOVER_TRIES] boots
//! total, before settling into the provisioning AP — the 2026-08-16 bench
//! conversion hit exactly one such flake and recovered on the next power
//! cycle, which this automates. Retry reboots are marked deliberate in
//! the guard record so they don't count toward preboot_guard's rollback.
//!
//! Settings inheritance: before touching anything, the takeover mounts
//! WLED's littlefs (read-only, src/wledfs.rs) and lifts the WiFi
//! credentials out of cfg.json/wsec.json; after the config wipe they are
//! written into Luxel's own creds record, so the device reappears on the
//! same network without ever being provisioned. A factory-fresh WLED (or
//! any parse failure) inherits nothing and Luxel boots its provisioning
//! AP instead — the fallback path, not an error.

use esp_println::println;

use crate::parttab::{self, Part, SECTOR, TABLE_OFFSET, TYPE_DATA};

/// nvs + boot-guard + otadata (+ phy_init) sectors under BOTH layouts:
/// wiped so Luxel starts from config defaults and the bootloader's
/// empty-otadata fallback picks ota_0. A WLED device has nothing of ours
/// in them; the Luxel→Luxel migrator deliberately wipes NOTHING (it keeps
/// the config it finds), which is why this range is not in parttab.
const CONFIG_WIPE: core::ops::Range<u32> = 0x9000..0x10000;

/// data/spiffs — what WLED's table calls its littlefs partition
const SUBTYPE_SPIFFS: u8 = 0x82;

/// The outgoing firmware's filesystem partition (WLED: littlefs in its
/// `spiffs` data partition), if the foreign table has one.
fn wled_fs_part(live_table: &[u8]) -> Option<Part> {
    parttab::entries(live_table)
        .into_iter()
        .find(|p| p.ptype == TYPE_DATA && p.subtype == SUBTYPE_SPIFFS)
}

/// A reader over the WLED filesystem partition, relative to its start.
fn fs_reader(live_table: &[u8]) -> Option<(Part, impl FnMut(u32, &mut [u8]) -> bool)> {
    let fs_part = wled_fs_part(live_table)?;
    let base = fs_part.offset;
    Some((fs_part, move |off: u32, buf: &mut [u8]| {
        off.checked_add(base)
            .is_some_and(|abs| crate::assets::read_chunk(abs, buf))
    }))
}

/// Best-effort WiFi inheritance from the outgoing firmware's filesystem.
/// Read-only; any failure just returns None and the provisioning AP covers it.
fn inherit_wifi(live_table: &[u8]) -> Option<(alloc::string::String, alloc::string::String)> {
    let (fs_part, mut read) = fs_reader(live_table)?;
    crate::wledfs::extract_wifi(&mut read, fs_part.len)
}

/// Best-effort LED wiring + defaults inheritance from the same cfg.json
/// (pixel count, strip type, color order, boot brightness, power cap,
/// gamma). Same posture as [inherit_wifi]: read-only, None on any failure,
/// and every field is individually optional — the board defaults cover
/// whatever is missing or unmappable.
fn inherit_wiring(live_table: &[u8]) -> Option<crate::wledfs::WledWiring> {
    let (fs_part, mut read) = fs_reader(live_table)?;
    crate::wledfs::extract_wiring(&mut read, fs_part.len)
}

/// Total boots that attempt the takeover before giving up and settling
/// into the provisioning AP (1 initial + 2 reboot-to-retries).
const TAKEOVER_TRIES: u8 = 3;

/// A takeover attempt aborted on something that might be a per-boot flash
/// flake (the 2026-08-16 bench conversion failed its first boot's
/// self-copy and ran clean on the next — issue #35): reboot to retry, at
/// most [TAKEOVER_TRIES] boots total, then settle into the provisioning
/// AP so a confused device never reboot-loops. WLED's table is intact in
/// either case; a later power cycle gets a fresh retry budget.
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
        "takeover: {} — giving up after {} attempts; provisioning AP will cover (WLED table intact)",
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

    // Inherit the outgoing firmware's WiFi credentials and LED wiring
    // before anything is modified; both are persisted after the config
    // wipe below.
    let inherited = inherit_wifi(&live);
    match &inherited {
        Some((ssid, _)) => println!("takeover: inherited WiFi credentials for \"{}\"", ssid),
        None => println!("takeover: no WiFi credentials to inherit (provisioning AP will cover)"),
    }
    let wiring = inherit_wiring(&live);
    if wiring.is_none() {
        println!("takeover: no LED wiring to inherit (board defaults)");
    }

    // Candidate slots under the live (foreign) table. Destination offset
    // first: if a previous, interrupted takeover already copied the image
    // there — or WLED happened to write the upload into app0 — skip the
    // copy entirely and never risk touching the region we run from.
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
        // (both put app0 at 0x10000, our slots above it) — this protects
        // future table changes that resize/move app slots.
        if src.offset < dest.offset + dest.len && dest.offset < src.offset + len {
            println!(
                "takeover: running image {:#x}+{} overlaps destination {:#x}+{} — refusing",
                src.offset, len, dest.offset, dest.len
            );
            return;
        }
        println!("takeover: copying {} B {:#x} → {:#x}", len, src.offset, dest.offset);
        if !parttab::copy_region(src.offset, dest.offset, len, "takeover") {
            println!("takeover: copy failed — aborting before table rewrite (WLED table intact)");
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

    // Persist the inherited LED wiring the same way: build a device-config
    // record from whatever mapped, board defaults for the rest. WLED's data
    // pin is imported when this board can drive it (Gitea #154) — the
    // takeover reboots anyway, so the first Luxel boot binds the SPI to it.
    // Failure is non-fatal: defaults cover, never a retry.
    if let Some(w) = wiring {
        let protocol = w.strip_type.and_then(crate::wledfs::map_strip_type);
        if let (Some(t), None) = (w.strip_type, protocol) {
            println!(
                "takeover: WLED strip type {} has no Luxel equivalent — keeping {}",
                t,
                crate::board::DEFAULT_PROTOCOL.name()
            );
        }
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
        let dev = crate::config::DeviceConfig {
            // WLED boot brightness is 0-255; Luxel's is 0-31 (>31 voids the
            // record). Round, and floor at 1 so an imported config can
            // never look like a dead strip.
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
            // Only meaningful relative to a protocol we actually mapped —
            // a guessed protocol would make the remap a color bug.
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
        };
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

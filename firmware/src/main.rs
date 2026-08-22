//! Luxel firmware, milestone 2: engine + LED output + WiFi live-coding.
//!
//! ESP32-C3 + SK9822/APA102 strip over SPI (data GPIO7, clock GPIO6 by
//! default — see board configuration below). Runs the Luxel engine and
//! pushes frames as fast as they render; when WiFi credentials are baked in
//! it joins the network, serves the playground web app on port 80, and swaps
//! the running pattern on upload without dropping frames.
//!
//! Flash + monitor (devkit over USB):
//!   cd firmware && LUXEL_SSID=net LUXEL_PASS=secret cargo run --release
//! Without LUXEL_SSID the firmware runs offline (render-only).

#![no_std]
#![no_main]
// the picoserve router's nested type (one layer per route) exceeds the
// default query depth
#![recursion_limit = "256"]
// Stack-frame guardrails (see clippy.toml). The main-task stack is tight
// and shared with WiFi NMI frames; a multi-KB buffer on it is a crash, not
// a slowdown. `large_stack_arrays` is a hard error — it would have caught
// the `[0u8; 4096]` staging buffer that briefly slipped into the pattern
// store. `large_stack_frames` (nursery) is a warning: it flags fat frames
// but can false-positive on async state machines.
// Run via `cargo clippy` on the default esp32c3 build (board-c3-devkit):
// the code is board-independent, and clippy can't run on the Xtensa build
// (its forked core + -Zbuild-std trips clippy-driver's intrinsic checks).
// For library/deep frames the Xtensa lint can't see, use tools/stack-check.
#![deny(clippy::large_stack_arrays)]
#![warn(clippy::large_stack_frames)]

extern crate alloc;

use core::sync::atomic::Ordering;

use embassy_executor::Spawner;
use embassy_net::{Runner, StackResources};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Rng;
use esp_hal::spi::master::{Config as SpiConfig, Spi, SpiDma};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::Blocking;
use esp_println::println;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

mod assets;
mod board;
mod config;
mod devicemap;
mod leds;
mod mqtt;
mod netin;
mod ota;
mod patterns;
mod playlist;
mod provision;
mod resume;
mod sensors;
mod server;
mod sntp;
mod shared;
mod takeover;
mod wledfs;

use leds::Protocol;
use luxel_core::jsonview;
use shared::{
    publish, set_pixels, set_vmerr, Msg, BRIGHTNESS, CONTROLS_JSON, FPS, MAX_PIXELS, MSG_QUEUE,
    PIXEL_COUNT, PROTOCOL, READOUTS_JSON, VARS_JSON,
};

esp_bootloader_esp_idf::esp_app_desc!();

/// After esp-backtrace prints a panic, reboot instead of halting forever —
/// an unattended device must never require a hands-on power cycle (learned
/// the hard way: a heap-exhaustion panic bricked the PB overnight until
/// morning). A crash loop still surfaces on serial and in DHCP activity.
#[unsafe(no_mangle)]
extern "Rust" fn custom_halt() -> ! {
    println!("panic: rebooting in 3s");
    let d = esp_hal::delay::Delay::new();
    d.delay_millis(3000);
    esp_hal::system::software_reset()
}

/// Signalled by the OTA handler once the success response is on the wire.
pub static REBOOT: embassy_sync::signal::Signal<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    (),
> = embassy_sync::signal::Signal::new();

// ---- board configuration (see docs/firmware.md for the full tables) ----
// board-c3-devkit (default): bare C3 devkit — CLOCK → GPIO6, DATA → GPIO7.
// board-athom-music: Athom WLED ESP32 music-reactive — channel 1 DATA1 =
//   GPIO18 / CLK1 = GPIO5; 16A strip-VCC relay on GPIO2 (must be high or
//   the strip stays dark); channel 2 + mic + IR unused for now.
// board-pixelblaze-v3: Pixelblaze v3 Standard (official schematic in
//   github.com/simap/pixelblaze) — DATA = GPIO23 (MOSI), CLOCK = GPIO18
//   (SCK), both through the onboard 5V level shifter; status LED GPIO12
//   (lit at boot = Luxel alive), button GPIO32 (unused).
// board-s3-devkit: ESP32-S3-DevKitC-1 — CLOCK → GPIO12, DATA → GPIO11
//   (SPI2/FSPI IO_MUX pins, so DMA gets the direct route; both free on
//   WROOM-1 modules including the octal-PSRAM variants, which claim
//   GPIO33–37). UNTESTED ON METAL — no S3 on the bench, wiring reviewed
//   against the devkit pinout only.
// board-c6-devkit: ESP32-C6-DevKitC-1 — CLOCK → GPIO6, DATA → GPIO7
//   (SPI2/FSPI IO_MUX pins; same numbers as the C3 devkit by coincidence
//   of the IO_MUX tables, and clear of the onboard RGB LED on GPIO8).
//   UNTESTED ON METAL — same caveat as the S3.
/// Board defaults (name, protocol, pixel count) come from board.rs; the
/// live values live in shared:: atomics (seeded at boot, runtime-settable
/// via /api/protocol and /api/config).
use board::{DEFAULT_PIXEL_COUNT, DEFAULT_PROTOCOL};
/// Global brightness 0–31 (APA102 5-bit current limiter; ignored for
/// WS2812). Keep modest on USB power.
const APA_BRIGHTNESS: u8 = 4;

/// Baked-in WiFi credentials (station mode) until NVS + provisioning land
/// in M3. Set at build time; absent → offline render-only mode.
const SSID: Option<&str> = option_env!("LUXEL_SSID");
const PASSWORD: Option<&str> = option_env!("LUXEL_PASS");

/// Built-in default pattern: source for `GET /api/pattern`, bytecode (built
/// by build.rs — the firmware links no compiler) for execution.
const PATTERN: &str = include_str!("../../library/rainbow.js");
const PATTERN_BC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/default.lxbc"));

macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);
    // Boot-loop guard, armed BEFORE the heap allocators below. A panic in the
    // pre-guard window — notably esp-alloc's "Exceeded the maximum of 3 heap
    // memory regions", which a flash-read flake corrupting the HEAP static's
    // .data slot array can trigger on a WLED-bootloader takeover boot
    // (reproduced under QEMU; tools/qemu/heap-regions-test.py) — reboots via
    // custom_halt but never reaches the old post-init guard, so a
    // deterministic version would loop forever without ever rolling back to
    // the working slot (WLED, on a takeover device). preboot_guard is
    // heap-free by necessity (no allocator yet); it fully replaces the old
    // ota::boot_guard() call. The flash driver is borrowed here and handed to
    // ota::init once the heap is up.
    let ota_flash = if option_env!("LUXEL_NO_OTA").is_none() {
        let mut flash = esp_storage::FlashStorage::new(p.FLASH);
        ota::preboot_guard(&mut flash);
        Some(flash)
    } else {
        None
    };
    // The WiFi blob mallocs through this allocator and does NOT null-check;
    // running the heap dry shows up as StoreProhibited crashes inside the
    // blob (seen on the PB v3 in pm_on_beacon_rx), not as clean OOM panics.
    // Keep headroom generous and watch /api/status heap_free. The reclaimed
    // (dram2) region is 98768 bytes on esp32, ~66 KB on the C3.
    #[cfg(feature = "esp32")]
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 96 * 1024);
    #[cfg(not(feature = "esp32"))]
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    // Classic ESP32 has less contiguous DRAM than the C3 once the WiFi
    // blob's statics are linked in — 160 KB here overflows the region by
    // ~17 KB (linker: "cannot move location counter backwards").
    //
    // CAUTION: whatever RWDATA this static does NOT claim becomes the main
    // task's stack (esp-hal's .stack section is "the rest of the region"),
    // and that one stack runs the embassy executor — every task's poll,
    // picoserve's response path, esp-storage's flash ops, AND the WiFi
    // level-6 NMI frames that land on whatever stack is current. At
    // 120 KB heap the leftover stack measured 15.6 KB and overflowed
    // reproducibly during flash reads (all 24 logged stack-guard panics).
    //
    // The SAME budget applies to every static — embassy task futures
    // included. v0.1.19's first cut put ~12 KB of MQTT/netin buffers in
    // task futures and bricked the boot (stack ≈ 10.7 KB); big task
    // buffers must be heap Vecs. History: 88 KB left ~31 KB of stack —
    // sized for the on-device compiler's recursion, which v0.1.24 removed
    // (devices execute bytecode; the decoder is iterative).
    //
    // MEASURE, don't estimate: `.stack` in `readelf -S` is the ground
    // truth. v0.1.31's 92 KB was tuned by estimating the 3-slot web pool
    // (server::WEB_TASK_POOL_SIZE) at ~9 KB total and claiming ~27 KB of
    // stack; the pool's static is really ~8.6 KB PER SLOT (26 KB — each
    // slot holds picoserve's whole response-path future), and the shipped
    // stack was 17.9 KB. That's ~2 KB above the measured 15.6 KB overflow
    // point, and one WiFi NMI frame atop a request-context flash read
    // (read_wifi, asset streaming) ate it: deterministic stack-guard
    // panics with SP at the stack floor and PC in
    // esp_rom_spiflash_read_status (Athom, v0.1.32, 2026-07-27). 80 KB
    // puts .stack at ~30 KB (31 KB ran clean for weeks) with the 3-slot
    // web pool. The pairing matters: `small-chip` drops the pool to 2
    // slots (server::WEB_TASK_POOL_SIZE), freeing ~8.6 KB of task arena,
    // and 88 KB here banks that as heap while keeping .stack in the same
    // measured ~30 KB zone (stack-check verified at 30,564 B). Never mix
    // 88 KB with the 3-slot pool — that lands ~22 KB of stack, under the
    // 24 KB floor. The esp-rtos stack guard + boot-loop guard catch it
    // non-destructively if this ever proves too tight.
    #[cfg(all(feature = "esp32", feature = "small-chip"))]
    esp_alloc::heap_allocator!(size: 88 * 1024);
    #[cfg(all(feature = "esp32", not(feature = "small-chip")))]
    esp_alloc::heap_allocator!(size: 80 * 1024);
    // Non-esp32 (C3/S3/C6): tuned on the C3's 313 KB DRAM, which is the
    // tightest of the three — the S3 (dram_seg ~334 KB) and C6 (~441 KB)
    // inherit it and simply keep a larger leftover .stack. UNTESTED ON
    // METAL for S3/C6; revisit per-chip when hardware exists.
    #[cfg(not(feature = "esp32"))]
    esp_alloc::heap_allocator!(size: 160 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!(
        "luxel-fw: boot ({} px default, {} @ {} Hz SPI)",
        DEFAULT_PIXEL_COUNT,
        DEFAULT_PROTOCOL.name(),
        DEFAULT_PROTOCOL.spi_hz()
    );

    // ---- BOARD WIRING (the only pin-specific code; see docs/boards.md) ----
    println!("board: {}", board::NAME);
    // Strip power relay: on before anything renders (see board notes above).
    #[cfg(feature = "board-athom-music")]
    let _relay = esp_hal::gpio::Output::new(
        p.GPIO2,
        esp_hal::gpio::Level::High,
        esp_hal::gpio::OutputConfig::default(),
    );
    // Status LED: lit = Luxel booted.
    #[cfg(feature = "board-pixelblaze-v3")]
    let _status_led = esp_hal::gpio::Output::new(
        p.GPIO12,
        esp_hal::gpio::Level::High,
        esp_hal::gpio::OutputConfig::default(),
    );

    let spi = Spi::new(
        p.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(DEFAULT_PROTOCOL.spi_hz()))
            .with_mode(Mode::_0),
    )
    .expect("spi init");
    #[cfg(feature = "board-c3-devkit")]
    let spi = spi.with_sck(p.GPIO6).with_mosi(p.GPIO7);
    #[cfg(feature = "board-athom-music")]
    let spi = spi.with_sck(p.GPIO5).with_mosi(p.GPIO18);
    #[cfg(feature = "board-pixelblaze-v3")]
    let spi = spi.with_sck(p.GPIO18).with_mosi(p.GPIO23);
    // generic classic-ESP32: VSPI defaults — most WROOM boards break these out
    #[cfg(feature = "board-esp32-generic")]
    let spi = spi.with_sck(p.GPIO18).with_mosi(p.GPIO23);
    // S3/C6 devkits: the chip's SPI2 (FSPI) IO_MUX pins. UNTESTED ON METAL.
    #[cfg(feature = "board-s3-devkit")]
    let spi = spi.with_sck(p.GPIO12).with_mosi(p.GPIO11);
    #[cfg(feature = "board-c6-devkit")]
    let spi = spi.with_sck(p.GPIO6).with_mosi(p.GPIO7);
    // ---- end board wiring ----

    // DMA, so each frame is ONE continuous transfer. The FIFO path splits
    // writes into 64-byte transactions with a CPU busy-wait between them;
    // 64 B = 512 SPI bits, not divisible by WS2812's 3-bits-per-bit, so
    // every chunk boundary corrupts a bit mid-symbol — and a WiFi interrupt
    // in the gap stretches it past the strip's latch time (partial-frame
    // latch). SK9822 is clocked and never noticed.
    // GDMA chips (C3/S3/C6) take a numbered channel; the classic ESP32's
    // older DMA is bound to the peripheral instead.
    #[cfg(not(feature = "esp32"))]
    let spi = spi.with_dma(p.DMA_CH0);
    #[cfg(feature = "esp32")]
    let spi = spi.with_dma(p.DMA_SPI2);

    // Bisect knob: LUXEL_NO_OTA=1 at build time skips OTA init entirely —
    // no esp-storage FlashStorage construction, no boot-time partition
    // table read — to test whether flash-driver setup interacts with the
    // esp32 radio crashes (serving worked before the OTA commit).
    if let Some(flash) = ota_flash {
        // The boot-loop guard already ran (preboot_guard, before the heap
        // allocators); reuse the flash driver it borrowed. Consuming
        // ota_flash here is what gates the rest on LUXEL_NO_OTA.
        ota::init(flash);
    // WLED → Luxel self-install (no-op when the partition table is already
    // ours). The boot guard (preboot_guard) already ran and armed rollback,
    // so a crash-looping takeover build still rolls back to the WLED slot
    // (WLED's table has valid ota_0/ota_1 + otadata). BEFORE assets/patterns
    // init: under a foreign table those regions belong to other partitions
    // (they fail safe, but takeover reboots).
    // Load-bearing for the WLED migration path: this exact call shipped
    // commented out ("//SIZETEST") from v0.1.31 through v0.1.38-dev and
    // nobody noticed until a real via-WLED install (2026-08-16). Don't
    // disable it outside a local experiment, and never commit that.
    takeover::maybe_takeover();
    assets::init();
    patterns::init();
    playlist::init(); // after patterns::init (shares the storage partition)
    devicemap::init();
    } else {
        println!("LUXEL_NO_OTA: ota disabled");
    }

    // Seed runtime settings from flash (else compile-time defaults) BEFORE the
    // render task spawns — it reads these once when it builds the engine and
    // configures SPI.
    let stored = config::read_device();
    let brightness = stored.map(|c| c.brightness).unwrap_or(APA_BRIGHTNESS);
    let pixels = stored
        .map(|c| c.pixel_count)
        .filter(|&n| n >= 1 && n <= MAX_PIXELS)
        .unwrap_or(DEFAULT_PIXEL_COUNT);
    let protocol = stored.map(|c| Protocol::from_u8(c.protocol)).unwrap_or(DEFAULT_PROTOCOL);
    BRIGHTNESS.store(brightness, Ordering::Relaxed);
    PIXEL_COUNT.store(pixels, Ordering::Relaxed);
    PROTOCOL.store(protocol.as_u8(), Ordering::Relaxed);
    // requested == applied at boot (see shared::device_config_snapshot)
    shared::WANT_PIXEL_COUNT.store(pixels, Ordering::Relaxed);
    shared::WANT_PROTOCOL.store(protocol.as_u8(), Ordering::Relaxed);
    shared::SYNC_MODE.store(stored.map(|c| c.sync_mode).unwrap_or(0), Ordering::Relaxed);
    shared::TZ_MINUTES.store(
        stored.map(|c| c.tz_minutes as i32).unwrap_or(0),
        Ordering::Relaxed,
    );
    shared::COLOR_ORDER.store(stored.map(|c| c.color_order).unwrap_or(0), Ordering::Relaxed);
    shared::GAMMA_TENTHS.store(stored.map(|c| c.gamma_tenths).unwrap_or(0), Ordering::Relaxed);
    shared::CAP_MA.store(stored.map(|c| c.cap_ma as u32).unwrap_or(0), Ordering::Relaxed);
    println!(
        "settings: {} px, {}, brightness {}/31 ({})",
        pixels,
        protocol.name(),
        brightness,
        if stored.is_some() { "flash" } else { "default" }
    );

    spawner.spawn(reboot_task().unwrap());
    // PB sensor expansion board input: the classic-ESP32 boards expose
    // UART0's RX (GPIO3) on the expansion header, where the board's TX
    // lands. Same 115200-8N1 the console runs, and TX stays untouched, so
    // logging is unaffected. (C3 devkit: no header wired — skipped.)
    #[cfg(feature = "esp32")]
    {
        let uart_cfg =
            esp_hal::uart::Config::default().with_baudrate(115_200);
        match esp_hal::uart::UartRx::new(p.UART0, uart_cfg) {
            Ok(rx) => {
                let rx = rx.with_rx(p.GPIO3).into_async();
                spawner.spawn(sensors::uart_task(rx).unwrap());
            }
            Err(e) => println!("sensor uart init failed: {:?}", e),
        }
    }
    // Bisect knob: LUXEL_QUIET=1 at build time skips the render task
    // entirely (no SPI, no engine, no snapshot publishing) to isolate
    // whether it interacts with the esp32 radio crashes.
    if option_env!("LUXEL_QUIET").is_none() {
        spawner.spawn(render_task(spi).unwrap());
        spawner.spawn(playlist::playlist_task().unwrap());
    } else {
        println!("LUXEL_QUIET: render task disabled");
    }

    // Credentials: the flash record wins (survives images built without
    // env creds — the lockout class that stranded the device twice), then
    // compile-time env, else offline. Treat empty env strings like unset —
    // `LUXEL_SSID='' …` shouldn't try to join a network named "".
    let flash_creds = if option_env!("LUXEL_NO_OTA").is_none() {
        config::read_wifi()
    } else {
        None
    };
    let baked = match (SSID, PASSWORD) {
        (Some(s), Some(p)) if !s.is_empty() => Some((s, p)),
        _ => None,
    };
    // "luxel-xxxxxx": the DHCP hostname as a station, the SSID as an AP.
    let mac_addr = esp_hal::efuse::base_mac_address();
    let mac = mac_addr.as_bytes();
    let mut hostname = heapless::String::<32>::new();
    let _ = core::fmt::Write::write_fmt(
        &mut hostname,
        format_args!("luxel-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5]),
    );
    println!("hostname: {}", hostname);

    // Provisioning AP when there's no way onto a network (or on request via
    // POST /api/apmode — a one-shot flag, so a crash here can't strand the
    // device off-net: the next boot is a normal station boot again).
    let force_ap = option_env!("LUXEL_NO_OTA").is_none() && ota::take_force_ap();
    let creds = match (&flash_creds, baked) {
        (Some((s, p)), _) => {
            println!("wifi: creds from flash (\"{}\")", s);
            Some((s.as_str(), p.as_str()))
        }
        (None, Some((s, p))) => {
            println!("wifi: compile-time creds (\"{}\")", s);
            Some((s, p))
        }
        (None, None) => None,
    };
    let ap_mode = force_ap || creds.is_none();

    let (config, wifi_interface, net_config) = if ap_mode {
        println!(
            "provisioning mode{}: open AP \"{}\"",
            if force_ap { " (requested)" } else { " (no wifi credentials)" },
            hostname
        );
        (
            WifiConfig::AccessPoint(
                esp_radio::wifi::ap::AccessPointConfig::default().with_ssid(hostname.as_str()),
            ),
            Interface::access_point(),
            embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
                address: embassy_net::Ipv4Cidr::new(provision::AP_IP, 24),
                gateway: Some(provision::AP_IP),
                dns_servers: heapless::Vec::new(),
            }),
        )
    } else {
        let (ssid, password) = creds.unwrap();
        println!("wifi: joining \"{}\"", ssid);
        let mut dhcp = embassy_net::DhcpConfig::default();
        dhcp.hostname = Some(hostname.clone());
        (
            WifiConfig::Station(
                StationConfig::default()
                    .with_ssid(ssid)
                    .with_password(password.into()),
            ),
            Interface::station(),
            embassy_net::Config::dhcpv4(dhcp),
        )
    };

    let controller = WifiController::new(
        p.WIFI,
        ControllerConfig::default().with_initial_config(config),
    )
    .expect("wifi controller");

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        mk_static!(
            // +2 spare, +2 DDP/E1.31 UDP, +1 MQTT TCP, +1 its DNS queries,
            // +1 sync beacons, +1 the follower's pattern-pull TCP
            // (AP mode reuses the pool for DHCP + DNS)
            StackResources<{ server::WEB_TASK_POOL_SIZE + 8 }>,
            StackResources::new()
        ),
        seed,
    );

    if ap_mode {
        spawner.spawn(ap_task(controller).unwrap());
    } else {
        spawner.spawn(connection_task(controller).unwrap());
    }
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        println!("ip: http://{}/", cfg.address.address());
    }
    println!("heap free: {}", esp_alloc::HEAP.free());

    // Single-pattern resume waits for the network on purpose: WiFi bring-up
    // mallocs don't null-check, and resume's pattern load is a multi-KB burst.
    // Spawned earlier it raced WiFi init and OOM-panicked the boot on heavy
    // configs (2048 px) — three strikes and the boot-loop guard flipped slots.
    // Post-IP the heap is at steady state and resume's own pre-flight is
    // measuring reality.
    spawner.spawn(resume::resume_task().unwrap());

    for task_id in 0..server::WEB_TASK_POOL_SIZE {
        spawner.spawn(server::web_task(task_id, stack).unwrap());
    }
    if ap_mode {
        provision::log_started(hostname.as_str());
        spawner.spawn(provision::dhcp_task(stack).unwrap());
        spawner.spawn(provision::dns_task(stack).unwrap());
    } else {
        spawner.spawn(netin::ddp_task(stack).unwrap());
        spawner.spawn(netin::e131_task(stack).unwrap());
        spawner.spawn(mqtt::mqtt_task(stack).unwrap());
        // boot id: random per boot, so followers notice a leader restart
        spawner.spawn(netin::sync_task(stack, rng.random()).unwrap());
        spawner.spawn(sntp::sntp_task(stack).unwrap());
    }

    let mut first_beat = true;
    loop {
        Timer::after(Duration::from_secs(60)).await;
        if first_beat {
            first_beat = false;
            ota::boot_ok(); // survived a minute of serving — not a boot loop
        }
        println!(
            "fps: {}  heap free: {}",
            FPS.load(Ordering::Relaxed),
            esp_alloc::HEAP.free()
        );
    }
}

/// Renders frames and drives the strip; picks up uploaded patterns between
/// Blend two RGB pixels by `t` in 0..=65536 (0 = a, 65536 = b).
#[inline]
fn blend_px(a: [u8; 3], b: [u8; 3], t: i32) -> [u8; 3] {
    let mix = |x: u8, y: u8| (((x as i32) * (65536 - t) + (y as i32) * t) >> 16) as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

/// Output pipeline (Settings): color-order remap + gamma LUT + power cap,
/// applied to a scratch copy just before protocol encoding. Returns the
/// original frame untouched when every knob is off. The LUT is cached and
/// rebuilt only when the gamma setting changes.
fn apply_outpipe<'a>(
    frame: &'a [[u8; 3]],
    pipe_buf: &'a mut alloc::vec::Vec<[u8; 3]>,
    gamma_cache: &mut (u8, Option<alloc::boxed::Box<[u8; 256]>>),
    brightness5: u8,
) -> &'a [[u8; 3]] {
    use luxel_core::outpipe::{self, ColorOrder};
    let order = shared::COLOR_ORDER.load(Ordering::Relaxed);
    let gamma = shared::GAMMA_TENTHS.load(Ordering::Relaxed);
    let cap = shared::CAP_MA.load(Ordering::Relaxed);
    let gamma_on = gamma > 0 && gamma != 10;
    if order == 0 && !gamma_on && cap == 0 {
        return frame;
    }
    if gamma_on && gamma_cache.0 != gamma {
        *gamma_cache = (gamma, Some(alloc::boxed::Box::new(outpipe::gamma_lut(gamma))));
    }
    pipe_buf.clear();
    pipe_buf.extend_from_slice(frame);
    outpipe::apply(
        pipe_buf,
        ColorOrder(order),
        if gamma_on { gamma_cache.1.as_deref() } else { None },
        cap,
        brightness5,
    );
    pipe_buf
}

/// SPI config for a protocol (only the clock rate differs; mode 0 for both).
fn spi_cfg(p: Protocol) -> SpiConfig {
    SpiConfig::default()
        .with_frequency(Rate::from_hz(p.spi_hz()))
        .with_mode(Mode::_0)
}

/// SPI encode buffer, u32-backed: the DMA driver only streams a slice
/// zero-copy when its base is 4-byte-aligned and its length a multiple
/// of 4 (classic-ESP32 DMA rule) — a Vec<u8> guarantees neither, and the
/// fallback re-chunks the frame through a bounce buffer (wire gaps: the
/// exact WS2812 corruption DMA is here to prevent). The ≤3 pad bytes
/// stay zero — harmless on both protocols (SK9822 end clocks / WS2812
/// latch tail).
struct EncodeBuf(alloc::vec::Vec<u32>);

impl EncodeBuf {
    const fn new() -> Self {
        Self(alloc::vec::Vec::new())
    }
    fn len(&self) -> usize {
        self.0.len() * 4
    }
    fn bytes(&self) -> &[u8] {
        // u32 → u8 reinterpret: alignment only loosens, length is exact
        unsafe { core::slice::from_raw_parts(self.0.as_ptr().cast(), self.0.len() * 4) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast(), self.0.len() * 4) }
    }
}

/// Resize the SPI encode buffer, releasing the old allocation BEFORE
/// reserving the new one — a protocol switch can more than double it
/// (WS2812 is 9 B/px vs SK9822's ~4 B/px; at 2048 px that's an 18 KB
/// allocation which must not coexist with the old buffer on a tight heap).
/// Fallible: on false the buffer is left empty and the encode paths (which
/// check the length) skip SPI output rather than indexing out of bounds.
fn realloc_buf(buf: &mut EncodeBuf, len: usize) -> bool {
    buf.0 = alloc::vec::Vec::new(); // free the old allocation first
    let words = len.div_ceil(4);
    if buf.0.try_reserve_exact(words).is_err() {
        return false;
    }
    buf.0.resize(words, 0);
    true
}

/// Build an engine from a decoded program with an array budget derived
/// from LIVE free heap (half of it, in 8-byte `Value`s, capped at PB's
/// 10240 elements). Patterns that out-allocate the device then record an
/// "array budget" vmerr instead of exhausting the allocator — an alloc
/// failure is a panic, i.e. a reboot (the soak-v5 OOM).
/// Heap the rest of the firmware needs while a pattern runs: jsonview
/// snapshots (~8.5 KB peak for var-heavy patterns — a 8 KB floor lost to
/// exactly that once), MQTT publishes, SPI buffer resizes, WiFi-blob
/// mallocs (which do NOT null-check), plus two HTTP connection buffers
/// (4 KB each — bodies STREAM, so connections never need body-sized
/// buffers). A pattern may not eat into this — its array budget stops
/// short of it, and a pattern whose engine leaves less free is rejected
/// outright after loading ("pattern too large" vmerr; soak-proven to
/// never panic).
const RUNTIME_FLOOR: usize = 20 * 1024;

fn budgeted_engine(prog: luxel_core::vm::Program, count: u32) -> Engine {
    // Arrays may consume free heap down to (but not past) the runtime
    // floor — byte-accurate (elements × 8 + per-array overhead), so one
    // big array isn't taxed for overhead only swarms of tiny ones pay.
    // The extra 4 KB keeps a maxed-out array arena from sitting EXACTLY on
    // the floor and losing the post-load check to a few bytes of churn.
    // The 16 KB minimum keeps ordinary strip patterns (a few arrays of
    // pixelCount) working even when free heap reads low mid-churn — if
    // that minimum genuinely doesn't fit, the post-load floor check
    // rejects the pattern instead (soak-proven: a rejection, never a
    // panic).
    let budget = (esp_alloc::HEAP.free() as usize)
        .saturating_sub(RUNTIME_FLOOR + 4 * 1024)
        .max(16 * 1024);
    Engine::from_program_budgeted(prog, count, 1, budget)
}

/// [`budgeted_engine`] + post-build floor check: a pattern that fits its
/// array budget but still leaves the heap under the floor (huge program,
/// long strip) is rejected — soak v5 showed a routine 8.5 KB jsonview
/// alloc panicking (= reboot) right after such a pattern loaded.
/// `Err(free_bytes_at_rejection)` — measured BEFORE the engine is dropped,
/// so error messages report the pressure that caused the rejection, not
/// the comfortable number after freeing.
fn try_budgeted_engine(prog: luxel_core::vm::Program, count: u32) -> Result<Engine, usize> {
    let e = budgeted_engine(prog, count);
    let free = esp_alloc::HEAP.free() as usize;
    if free < RUNTIME_FLOOR {
        println!(
            "pattern rejected: {} B heap left after load (< {} floor)",
            free, RUNTIME_FLOOR
        );
        return Err(free); // drops the engine, freeing its heap
    }
    Ok(e)
}

/// Stamp the just-swapped pattern's identity + read-back location. Runs on
/// the render task at the swap, so id / hash / location can never disagree
/// with the running content (senders used to stamp the id after queueing —
/// racy).
///
/// LIBRARY swaps (`id` non-empty: playlist advance, activate, MQTT select,
/// boot resume) write NOTHING: their source + blob already live in the
/// pattern store, and read-back serves from there (shared::*Loc::Library).
/// This is the flash-WEAR fix — the raw slot's fixed sectors used to be
/// erased on EVERY playlist advance (~17k cycles/day at 5 s items).
///
/// AD-HOC swaps (`id` empty: /api/code, sync adoption) still persist to the
/// slot (replacing the old standing RAM copies): a brief one-time frame
/// hitch on a rare, human-driven event. On a failed write (flash leased out
/// by a concurrent OTA/save, or too large) read-back is marked unavailable
/// and logged — /api/status then reports src=false, bc=false, and
/// GET /api/pattern + the sync envelope serve nothing until the next swap
/// (never a panic).
async fn persist_current_pattern(src: &str, bc: &[u8], id: &str) {
    shared::set_pattern_hash(src);
    shared::set_current_pattern_id(id);
    if !id.is_empty() {
        shared::set_current_library(src.len(), bc.len());
        return;
    }
    if patterns::store_current(src, bc).await {
        shared::set_current_flash(src.len(), bc.len());
    } else {
        println!(
            "current-pattern: flash write failed (src {} B + bc {} B, {} B free) — read-back (/api/pattern, sync) degraded until the next swap",
            src.len(),
            bc.len(),
            esp_alloc::HEAP.free()
        );
        shared::set_current_gone();
    }
}

/// frames. Yields to the network tasks after every frame.
#[embassy_executor::task]
async fn render_task(mut spi: SpiDma<'static, Blocking>) -> ! {
    let cur_protocol = || Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed));
    // master power (HA light switch): off = encode at brightness 0 (black on
    // both protocols) while the engine keeps ticking, so ON resumes mid-motion
    let out_brightness = || {
        if shared::POWER.load(Ordering::Relaxed) {
            BRIGHTNESS.load(Ordering::Relaxed)
        } else {
            0
        }
    };
    // Heap discipline: exactly ONE decoded Program lives at a time — inside
    // the engine. Rebuilds (pixel-count change, map clear) re-decode from
    // the running pattern's blob rather than keeping a second Program
    // resident; a resident copy + per-rebuild clones is what OOM'd soak v5
    // (Programs with debug info are several times their blob size). That blob
    // no longer sits in RAM either — it lives in the flash read-back slot
    // (shared::current_bc), read into a TRANSIENT Vec only for the rebuild.
    // deserialize_lean: no debug info on-device — halves a Program's RAM
    let mut engine = match luxel_core::bytecode::deserialize_lean(PATTERN_BC) {
        Ok(p) => Some(budgeted_engine(p, PIXEL_COUNT.load(Ordering::Relaxed))),
        Err(e) => {
            println!("embedded pattern bytecode error (build bug?): {}", e);
            None
        }
    };
    // The boot default is `&'static` rodata: read-back serves it directly,
    // no heap and no flash write. Every later swap repoints this at flash.
    shared::set_current_default(PATTERN, PATTERN_BC);
    // Rebuild the engine from the running blob at the current pixel count.
    // The blob comes from wherever read-back currently points: the rodata
    // default (borrowed, no alloc), the flash slot, or the pattern store for
    // a library pattern — the latter two are transient fallible Vecs dropped
    // as soon as the Program is built. A flash-busy read (or a library
    // pattern deleted mid-session) yields None and the engine stays paused
    // until the next swap — never a panic.
    let rebuild = || {
        let count = PIXEL_COUNT.load(Ordering::Relaxed);
        match shared::current_bc() {
            shared::BcLoc::Default(b) => luxel_core::bytecode::deserialize_lean(b)
                .ok()
                .and_then(|p| try_budgeted_engine(p, count).ok()),
            shared::BcLoc::Flash(len) => {
                let bc = crate::patterns::read_current_bc(len)?;
                luxel_core::bytecode::deserialize_lean(&bc)
                    .ok()
                    .and_then(|p| try_budgeted_engine(p, count).ok())
            }
            // library pattern: fetch the store's CURRENT blob (not the
            // snapshot length — a re-save may have changed it, and the
            // store's copy is the truth)
            shared::BcLoc::Library(_) => {
                let bc = crate::patterns::bytecode_of(&shared::get_current_pattern_id())?;
                luxel_core::bytecode::deserialize_lean(&bc)
                    .ok()
                    .and_then(|p| try_budgeted_engine(p, count).ok())
            }
            shared::BcLoc::Gone => None,
        }
    };
    if let Some(eng) = engine.as_ref() {
        publish(&CONTROLS_JSON, jsonview::controls_json(eng));
    }

    // Apply the seeded protocol — flash may specify one different from the
    // boot-time default the SPI was constructed with.
    if let Err(e) = spi.apply_config(&spi_cfg(cur_protocol())) {
        println!("spi config error: {:?}", e);
    }
    let mut buf = EncodeBuf::new();
    if !realloc_buf(
        &mut buf,
        cur_protocol().buf_len(PIXEL_COUNT.load(Ordering::Relaxed) as usize),
    ) {
        // the encode paths retry lazily once heap frees up
        println!("encode buffer alloc failed at boot — output paused");
    }
    // crossfade: the outgoing engine + blend timing + a reusable blend buffer
    let mut prev: Option<Engine> = None;
    let mut blend_start = Instant::now();
    let mut blend_ms: u32 = 0;
    let mut blend_buf: alloc::vec::Vec<[u8; 3]> = alloc::vec::Vec::new();
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();
    let mut sensor_seen: u32 = 0;
    // last reported vmerr site (fn, pc) — dedupes the per-frame report
    let mut vmerr_seen: Option<(u16, u32)> = None;
    // output-pipeline scratch (heap: the main-stack rule for task futures)
    let mut pipe_buf: alloc::vec::Vec<[u8; 3]> = alloc::vec::Vec::new();
    let mut gamma_cache: (u8, Option<alloc::boxed::Box<[u8; 256]>>) = (0, None);

    loop {
        while let Ok(msg) = MSG_QUEUE.try_receive() {
            match msg {
                Msg::Code { env, id } => {
                    // Envelope-validated by the sender. Drop the outgoing
                    // engine BEFORE decoding the new program — peak heap
                    // lands here, where the most is free.
                    engine = None;
                    prev = None;
                    // The Program owns its bytes, so once it's decoded and the
                    // envelope is persisted to the flash read-back slot, the
                    // ~envelope-sized buffer can be DROPPED before the engine
                    // builds — its 10s-of-KB then count toward the array budget
                    // and the post-load floor check instead of against them.
                    // (Observed on-device: Music Sequencer @300 px missed the
                    // floor by 456 B purely because the envelope was still
                    // held here.)
                    let decoded = match luxel_core::bytecode::decode_envelope(&env) {
                        Ok(le) => match luxel_core::bytecode::deserialize_lean(le.bytecode) {
                            Ok(p) => {
                                persist_current_pattern(le.source, le.bytecode, &id).await;
                                Ok(p)
                            }
                            // decode can legitimately fail on a starved heap
                            // (try_reserve) — surface it, don't just log
                            Err(e) => Err(Some(e)),
                        },
                        Err(e) => {
                            println!("envelope decode failed (bug?): {}", e);
                            Err(None)
                        }
                    };
                    drop(env);
                    match decoded {
                        Ok(p) => {
                            match try_budgeted_engine(p, PIXEL_COUNT.load(Ordering::Relaxed)) {
                                Ok(e) => {
                                    publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                                    engine = Some(e);
                                    set_vmerr(None);
                                    vmerr_seen = None;
                                    last = Instant::now();
                                    devicemap::mark_dirty(); // re-apply the installed map
                                }
                                Err(left) => set_vmerr(Some(alloc::format!(
                                    "pattern too large for this device — it left only {} KB of heap free (the firmware needs {} KB to keep running)",
                                    left / 1024,
                                    RUNTIME_FLOOR / 1024
                                ))),
                            }
                        }
                        Err(Some(e)) => {
                            println!("bytecode decode failed: {}", e);
                            set_vmerr(Some(alloc::format!("{}", e)));
                        }
                        Err(None) => {}
                    }
                }
                Msg::Freeze => {
                    // free the engine's heap for whoever asked (OTA flash
                    // phase, or a pattern upload that couldn't allocate);
                    // the next Code/Crossfade revives rendering
                    engine = None;
                    prev = None;
                    println!("engine frozen (heap released)");
                }
                Msg::Control(name, values) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_control(&name, &values);
                    }
                }
                Msg::Var(name, value) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_var(&name, value);
                    }
                }
                // Live pixel-count change (no reboot): resize the SPI buffer
                // and rebuild the engine at the new count from the current
                // source. This task is the sole writer of PIXEL_COUNT.
                Msg::Config(count) => {
                    let count = count.clamp(1, MAX_PIXELS);
                    PIXEL_COUNT.store(count, Ordering::Relaxed);
                    engine = None; // free before re-decoding (peak heap)
                    prev = None;
                    // resize AFTER freeing the engines — at 2048 px the new
                    // buffer is a multi-KB alloc that wants the peak heap too
                    if !realloc_buf(&mut buf, cur_protocol().buf_len(count as usize)) {
                        println!("encode buffer alloc failed ({} px) — output paused", count);
                    }
                    if let Some(e) = rebuild() {
                        publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                        engine = Some(e);
                        set_vmerr(None);
                        vmerr_seen = None;
                        last = Instant::now();
                        devicemap::mark_dirty(); // re-apply the installed map
                    }
                    // invariants are config-relative — re-check the playlist
                    playlist::preflight_mark_dirty();
                    println!("pixel count → {}", count);
                }
                // Live LED-protocol change (no reboot): reconfigure the SPI
                // clock and resize the buffer to the new encoding. Sole writer
                // of PROTOCOL. Ordering matters twice over:
                // - the SPI clock is reconfigured FIRST, and the protocol only
                //   commits (atomic + buffer) if that succeeded — otherwise
                //   the encode format and the wire clock would disagree and
                //   the strip would show garbage until the next switch;
                // - messages drain between frames and spi.write is blocking,
                //   so a switch can never land mid-frame — each frame goes
                //   out entirely in one protocol at one clock.
                Msg::Protocol(code) => {
                    let p = Protocol::from_u8(code);
                    if let Err(e) = spi.apply_config(&spi_cfg(p)) {
                        println!(
                            "spi config error: {:?} — staying on {}",
                            e,
                            cur_protocol().name()
                        );
                    } else {
                        PROTOCOL.store(p.as_u8(), Ordering::Relaxed);
                        let need = p.buf_len(PIXEL_COUNT.load(Ordering::Relaxed) as usize);
                        if !realloc_buf(&mut buf, need) {
                            // heap too tight for the bigger encoding: free the
                            // engines (Freeze semantics — the strip holds its
                            // last frame) and retry; the next Code/Crossfade
                            // revives rendering
                            engine = None;
                            prev = None;
                            if !realloc_buf(&mut buf, need) {
                                println!(
                                    "encode buffer alloc failed ({} B) — output paused",
                                    need
                                );
                            }
                        }
                        last = Instant::now();
                        println!("protocol → {}", p.name());
                    }
                }
                // Crossfade to a new pattern (playlist transition): keep the
                // outgoing engine and blend over `ms`.
                Msg::Crossfade { env, ms, id } => {
                    // the outgoing engine stays alive on purpose (it's the
                    // blend source) — this is the one path where two
                    // programs coexist, bounded by the crossfade duration
                    prev = None; // but never THREE (a fade still in flight)
                    // Same envelope-drop-before-engine-build discipline as
                    // Msg::Code above — doubly important here, where the
                    // outgoing engine also stays alive as the blend source.
                    let decoded = match luxel_core::bytecode::decode_envelope(&env) {
                        Ok(le) => match luxel_core::bytecode::deserialize_lean(le.bytecode) {
                            Ok(p) => {
                                persist_current_pattern(le.source, le.bytecode, &id).await;
                                Ok(p)
                            }
                            Err(e) => Err(Some(e)),
                        },
                        Err(e) => {
                            println!("envelope decode failed (bug?): {}", e);
                            Err(None)
                        }
                    };
                    drop(env);
                    match decoded {
                        Ok(p) => {
                            match try_budgeted_engine(p, PIXEL_COUNT.load(Ordering::Relaxed)) {
                                Ok(e) => {
                                    publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                                    if ms > 0 && engine.is_some() {
                                        prev = engine.take();
                                        blend_start = Instant::now();
                                        blend_ms = ms;
                                    }
                                    engine = Some(e);
                                    set_vmerr(None);
                                    vmerr_seen = None;
                                    last = Instant::now();
                                    devicemap::mark_dirty();
                                }
                                Err(left) => set_vmerr(Some(alloc::format!(
                                    "pattern too large for this device — it left only {} KB of heap free (the firmware needs {} KB to keep running)",
                                    left / 1024,
                                    RUNTIME_FLOOR / 1024
                                ))),
                            }
                        }
                        Err(Some(e)) => {
                            println!("crossfade bytecode decode failed: {}", e);
                            set_vmerr(Some(alloc::format!("{}", e)));
                        }
                        Err(None) => {}
                    }
                }
            }
        }

        // apply (or clear) the installed pixel map when it changed
        if devicemap::take_dirty() {
            if devicemap::has_map() {
                if let Some(eng) = engine.as_mut() {
                    devicemap::apply(eng);
                }
            } else {
                // cleared → rebuild without a map (do not re-mark dirty)
                drop(engine.take()); // free before re-decoding (peak heap)
                engine = rebuild();
            }
        }

        // sensor data (sensor board / POST /api/sensors) lands between frames
        if let Some(sf) = shared::take_sensor_frame(&mut sensor_seen) {
            if let Some(eng) = engine.as_mut() {
                eng.set_sensors(&sf);
            }
        }

        // injected events (POST /api/events) land between frames too
        let evs = shared::take_events();
        if !evs.is_empty() {
            if let Some(eng) = engine.as_mut() {
                for ev in evs {
                    eng.push_event(ev);
                }
            }
        }

        // network input (DDP/E1.31) overrides the engine while packets flow;
        // LIVE_TIMEOUT_MS after the stream stops, the pattern takes back over
        if shared::live_proto(Instant::now().as_millis() as u32).is_some() {
            shared::LIVE_PIXELS.lock(|c| {
                let live = c.borrow();
                blend_buf.clear();
                for i in 0..PIXEL_COUNT.load(Ordering::Relaxed) as usize {
                    let p = i * 3;
                    blend_buf.push(match live.get(p..p + 3) {
                        Some(px) => [px[0], px[1], px[2]],
                        None => [0, 0, 0],
                    });
                }
            });
            set_pixels(&blend_buf);
            let b5 = out_brightness();
            let wire = apply_outpipe(&blend_buf, &mut pipe_buf, &mut gamma_cache, b5);
            let proto = cur_protocol();
            // the buffer is empty after a failed realloc — retry it lazily
            // (heap may have freed up); never index out of bounds
            let need = proto.buf_len(wire.len());
            if buf.len() >= need || realloc_buf(&mut buf, need) {
                proto.encode(wire, b5, buf.bytes_mut());
                if let Err(e) = spi.write(buf.bytes()) {
                    println!("spi write error: {:?}", e);
                }
            }
            last = Instant::now(); // keep the pattern clock fresh for resume
        } else if engine.is_some() {
            let now = Instant::now();
            let delta_us = (now - last).as_micros();
            last = now;
            // µs → 16.16 ms
            let mut delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

            // sync follower: converge on the leader clock — big offsets
            // jump, small ones slew by stretching this delta ≤ ±25%
            if shared::SYNC_MODE.load(Ordering::Relaxed) == 2 {
                if let Some((_, lt, at)) = shared::sync_leader() {
                    let eng = engine.as_mut().unwrap();
                    let target = lt + at.elapsed().as_millis();
                    let err = target as i64 - eng.time_ms() as i64;
                    if err.unsigned_abs() > 1000 {
                        eng.set_time_ms(target);
                    } else {
                        let cap = (delta.raw() as i64 / 4).max(1);
                        let adj = (err << 16).clamp(-cap, cap); // ms → raw
                        delta =
                            Fx::from_raw((delta.raw() as i64 + adj).clamp(0, i32::MAX as i64) as i32);
                    }
                }
            }

            // crossfade progress (0..=65536); 65536 = the fade is complete
            let t = if blend_ms > 0 {
                ((blend_start.elapsed().as_millis() as i64 * 65536 / blend_ms as i64).min(65536))
                    as i32
            } else {
                65536
            };
            let out: &[[u8; 3]] = if prev.is_some() && t < 65536 {
                // copy the incoming frame, then blend the outgoing on top
                blend_buf.clear();
                blend_buf.extend_from_slice(engine.as_mut().unwrap().frame(delta));
                let px_old = prev.as_mut().unwrap().frame(delta);
                for i in 0..blend_buf.len().min(px_old.len()) {
                    blend_buf[i] = blend_px(px_old[i], blend_buf[i], t);
                }
                &blend_buf
            } else {
                prev = None; // fade finished
                engine.as_mut().unwrap().frame(delta)
            };
            set_pixels(out);
            let b5 = out_brightness();
            let wire = apply_outpipe(out, &mut pipe_buf, &mut gamma_cache, b5);
            let proto = cur_protocol();
            // the buffer is empty after a failed realloc — retry it lazily
            // (heap may have freed up); never index out of bounds
            let need = proto.buf_len(wire.len());
            if buf.len() >= need || realloc_buf(&mut buf, need) {
                proto.encode(wire, b5, buf.bytes_mut());
                if let Err(e) = spi.write(buf.bytes()) {
                    println!("spi write error: {:?}", e);
                }
            }
            if let Some(e) = engine.as_mut().unwrap().take_error() {
                // report each distinct error site once, not per frame — an
                // erroring pattern at 120 fps floods serial and churns the
                // (possibly already tight) heap with format! strings
                if vmerr_seen != Some((e.fn_idx, e.pc)) {
                    vmerr_seen = Some((e.fn_idx, e.pc));
                    // (0,0) = no source location (lean decode) — don't
                    // prefix noise
                    let msg = if e.line == 0 && e.col == 0 {
                        e.message
                    } else {
                        alloc::format!("line {}:{}: {}", e.line, e.col, e.message)
                    };
                    println!("vmerr: {}", msg);
                    set_vmerr(Some(msg));
                }
            }
            // publish the engine clock (leader beacons + /api/sync)
            shared::set_engine_time_ms(engine.as_ref().unwrap().time_ms());
        }

        frames += 1;
        if (Instant::now() - fps_mark).as_millis() >= 1000 {
            FPS.store(frames, Ordering::Relaxed);
            frames = 0;
            fps_mark = Instant::now();
        }
        if (Instant::now() - vars_mark).as_millis() >= 250 {
            vars_mark = Instant::now();
            if let Some(eng) = engine.as_mut() {
                publish(&VARS_JSON, jsonview::vars_json(eng));
                publish(&READOUTS_JSON, jsonview::readouts_json(eng));
                // NTP-synced local time for the clock builtins
                if let Some(local) = shared::wall_now_local() {
                    eng.set_wall_clock(local);
                }
            }
        }

        // playlist pre-flight: one queued item per frame — run its
        // assert() invariants against the CURRENT config in a throwaway
        // VM (free for assert-less patterns; the message table gates it).
        // Same headroom math as budgeted_engine so a check can't starve
        // the live engine; a stale-format blob reports its decode error
        // (the fix — recompile — is the same user action either way).
        if let Some(id) = playlist::preflight_next() {
            let violation = match patterns::bytecode_of(&id) {
                Some(bc) => match luxel_core::bytecode::deserialize_lean(&bc) {
                    Ok(p) => {
                        let budget = (esp_alloc::HEAP.free() as usize)
                            .saturating_sub(RUNTIME_FLOOR + 4 * 1024)
                            .max(16 * 1024);
                        luxel_core::engine::check_asserts(
                            &p,
                            PIXEL_COUNT.load(Ordering::Relaxed),
                            budget,
                        )
                    }
                    Err(e) => Some(alloc::format!("{}", e)),
                },
                None => None, // deleted pattern; the scheduler logs it
            };
            if let Some(m) = &violation {
                println!("playlist preflight: {} → {}", id, m);
            }
            playlist::preflight_record(&id, violation);
        }

        // Pace to ~120 fps: an uncapped render loop starves the network
        // tasks (choppy preview, timed-out polls) for frame rate nobody can
        // see. Slow patterns just yield. No engine (rejected pattern) =
        // nothing to render — idle properly instead of busy-spinning.
        if engine.is_none() {
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }
        let spent = Instant::now() - last;
        if spent.as_micros() < 8_000 {
            Timer::after(Duration::from_micros(8_000 - spent.as_micros())).await;
        } else {
            embassy_futures::yield_now().await;
        }
    }
}

/// Waits for the OTA handler's signal, gives the TCP stack a moment to
/// flush the response, then resets into the freshly activated slot.
#[embassy_executor::task]
async fn reboot_task() -> ! {
    REBOOT.wait().await;
    println!("rebooting into new firmware…");
    Timer::after(Duration::from_millis(400)).await;
    esp_hal::system::software_reset()
}

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(info) => {
                println!("wifi connected: {:?}", info);
                let info = controller.wait_for_disconnect_async().await.ok();
                println!("wifi disconnected: {:?}", info);
            }
            Err(e) => {
                println!("wifi connect failed: {:?}", e);
            }
        }
        Timer::after(Duration::from_millis(5000)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}

/// AP (provisioning) mode: the controller's initial config already started
/// the access point — this task just owns it for the rest of the boot.
#[embassy_executor::task]
async fn ap_task(controller: WifiController<'static>) -> ! {
    let _keep = controller;
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

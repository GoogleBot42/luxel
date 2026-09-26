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
// The JIT writes instruction memory and has to fence the prefetch against
// it (`isync`, firmware/src/jit.rs). `asm!` on Xtensa is still behind a
// feature gate; the gate is opened ONLY for the builds that need it — the
// `jit` feature is Xtensa-only and those builds use Espressif's
// nightly-based rustc fork, while every RISC-V board compiles this file
// with the attribute absent.
#![cfg_attr(feature = "jit", feature(asm_experimental_arch))]
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
#[cfg(not(feature = "hub75"))]
use esp_hal::spi::master::{Config as SpiConfig, Spi};
#[cfg(not(feature = "hub75"))]
use esp_hal::spi::Mode;
#[cfg(not(feature = "hub75"))]
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::projection::ProjectionMode;

mod appimg;
#[cfg(multi_core)]
mod appwdt;
mod assets;
mod board;
mod config;
mod core1;
mod devicemap;
mod devname;
mod patlog;
mod flashmap;
mod parttab;
mod migrate;
mod gpio;
/// On-device JIT (Gitea #658). Xtensa only — the emitter has one backend.
#[cfg(feature = "jit")]
mod jit;
#[cfg(all(feature = "jit", not(target_arch = "xtensa")))]
compile_error!(
    "the `jit` feature is Xtensa-only: luxel-jit emits LX6/LX7 code and \
     firmware/src/jit.rs's exec buffer assumes instruction-bus `.rwtext`"
);
#[cfg(feature = "hub75")]
mod hub75;
#[cfg(all(feature = "hub75-spare-plane", not(feature = "hub75")))]
compile_error!("`hub75-spare-plane` is a HUB75 driver mode and needs the `hub75` feature");
mod layout;
mod leds;
mod mqtt;
mod netin;
mod ota;
mod outpal;
mod output;
mod patterns;
mod pipeline;
mod playlist;
mod provision;
#[cfg(feature = "psram-arena")]
mod psram;
mod resume;
mod scenes;
mod scenestore;
mod sensors;
mod server;
mod sprites;
mod sntp;
mod shared;
mod textslots;
#[cfg(feature = "wled-takeover")]
mod takeover;
#[cfg(feature = "wled-takeover")]
mod wledfs;

use leds::Protocol;
use luxel_core::jsonview;
use shared::{
    publish, set_vmerr, Msg, BRIGHTNESS, CONTROLS_JSON, FPS, MAX_PIXELS, MSG_QUEUE,
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

/// Optional built-in default pattern: source for `GET /api/pattern`,
/// bytecode (built by build.rs — the firmware links no compiler) for
/// execution.
///
/// **A shipped image has none.** A device that has never been given a
/// pattern plays nothing and the strip is dark (Gitea #744); what runs at
/// boot comes from flash — `playlist::init` or `resume::resume_task` — or
/// from the first `POST /api/code` / activate. These constants exist only
/// for a build that passed `LUXEL_DEFAULT_PATTERN`, which is how the QEMU
/// JIT gate selects a pattern in an environment where nothing else can
/// (build.rs `bake_default_pattern`, Gitea #658).
///
/// Both halves come out of `OUT_DIR`, so the source served and the bytecode
/// executed are provably the same file.
#[cfg(default_pattern)]
const PATTERN: &str = include_str!(concat!(env!("OUT_DIR"), "/default.js"));

/// `include_bytes!` gives alignment 1, and `deserialize_lean_static` only
/// BORROWS a blob whose word region is 4-aligned in memory (it silently
/// copies otherwise). The zero-sized `[u32; 0]` raises the struct's
/// alignment to 4 without adding a byte, so the boot default executes from
/// rodata like every other mapped pattern (Gitea #260).
#[cfg(default_pattern)]
#[repr(C)]
struct Aligned4<T: ?Sized> {
    _align: [u32; 0],
    bytes: T,
}
#[cfg(default_pattern)]
static PATTERN_BC_ALIGNED: &Aligned4<[u8]> = &Aligned4 {
    _align: [],
    bytes: *include_bytes!(concat!(env!("OUT_DIR"), "/default.lxbc")),
};
#[cfg(default_pattern)]
const PATTERN_BC: &[u8] = &PATTERN_BC_ALIGNED.bytes;

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
    let mut ota_flash = if option_env!("LUXEL_NO_OTA").is_none() {
        let flash = esp_storage::FlashStorage::new(p.FLASH);
        // Dual-core: esp-storage's default strategy fails every flash write
        // while the second core runs. The flash fence (core1.rs) parks the
        // other core around each op instead — that guarantee is what makes
        // `multicore_ignore` sound here.
        #[cfg(multi_core)]
        let flash = unsafe { flash.multicore_ignore() };
        let mut flash = flash;
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
    // A second output driver (Gitea #474) is task statics — another SpiDma
    // and another encode-buffer Vec inside the render task's future — and on
    // the classic ESP32 `.stack` is what is LEFT after them. The board that
    // has one gives the kilobyte back from the heap rather than from the
    // stack floor, so `tools/stack-check.sh` stays green on it; every other
    // board's RAM layout is untouched (docs/boards.md "Two outputs").
    #[cfg(multi_output)]
    const SECOND_OUTPUT_RAM: usize = 1024;
    #[cfg(not(multi_output))]
    const SECOND_OUTPUT_RAM: usize = 0;
    // Statics that have accumulated since the 80 KB figure was set, bought
    // back from the heap for the same reason `SECOND_OUTPUT_RAM` is: on the
    // classic ESP32 `.stack` is the DRAM LEFT OVER, so a growing static
    // eats the stack floor rather than the heap. Measured 2026-09-19 with
    // `tools/stack-check.sh`: `board-pixelblaze-v3` was already **116 B
    // UNDER** the 24,576 B floor on master (24,460 B) before #538's device
    // name and SNTP-poke statics took another 120 B; 512 B here puts it at
    // 24,852 B and `board-athom-music` at 25,676 B. `tools/ci.sh` does not
    // run stack-check, which is how master drifted under it — Gitea #515.
    //
    // 2026-09-24, 512 → 4096: Phase C's text-slot table (#484/#485) and
    // Phase B's scene routes (#478) between them took another 3.3 KB of
    // DRAM statics, most of it the web task's FUTURE — picoserve's whole
    // response path, replicated `server::WEB_TASK_POOL_SIZE` times, which
    // four new route arms grow by ~400 B per slot. Measured with
    // `tools/stack-check.sh` on THIS tree: `board-pixelblaze-v3` 21,252 B
    // and `board-athom-music` 22,100 B before the bump (master itself was
    // already under the floor at 23,484 / 24,340 — #484's statics), 24,836
    // and 25,684 after it. The classic ESP32 gives up 3.5 KB of heap for
    // it, the same trade `SECOND_OUTPUT_RAM` makes and for the same reason:
    // here `.stack` is the DRAM left over, so a static eats the floor.
    const STATICS_RESERVE: usize = 4096;
    #[cfg(all(feature = "esp32", feature = "small-chip"))]
    esp_alloc::heap_allocator!(size: 88 * 1024 - SECOND_OUTPUT_RAM - STATICS_RESERVE);
    #[cfg(all(feature = "esp32", not(feature = "small-chip")))]
    esp_alloc::heap_allocator!(size: 80 * 1024 - SECOND_OUTPUT_RAM - STATICS_RESERVE);
    // Non-esp32 (C3/S3/C6): tuned on the C3's 313 KB DRAM, which is the
    // tightest of the three — the S3 (dram_seg ~334 KB) and C6 (~441 KB)
    // inherit it and simply keep a larger leftover .stack. UNTESTED ON
    // METAL for S3/C6; revisit per-chip when hardware exists.
    #[cfg(all(not(feature = "esp32"), not(feature = "psram-arena")))]
    esp_alloc::heap_allocator!(size: 160 * 1024);
    // psram-arena boards pay 6 KB of it back to `.stack`. esp-hal's PSRAM
    // bring-up is `#[ram]` throughout (it runs with the data cache
    // suspended), and on the S3 `.rwtext` and `.stack` are the same SRAM:
    // linking it cost a MEASURED 5,584 B of stack (28,780 → 23,196 B,
    // through the 24 KB floor). 154 KB puts it back at 29,340 B, above
    // where it was. The heap gives up 6 KB and gets an 8 MB array arena —
    // and a big pattern's arrays no longer come out of this region at all.
    // 2026-09-24, 154 → 152 KB: the same ~2 KB of Phase B/C statics that
    // pushed the classic ESP32 under its floor (see `STATICS_RESERVE`
    // above) took this board from 26,228 B to 24,004 B, through it.
    // Measured back at 26,052 B; on a board with an 8 MB array arena, 2 KB
    // of DRAM heap is the cheapest place to find it.
    #[cfg(all(not(feature = "esp32"), feature = "psram-arena"))]
    esp_alloc::heap_allocator!(size: 152 * 1024);

    // External PSRAM as the pattern-array arena (Gitea #253, psram.rs). A
    // SEPARATE esp-alloc heap, so nothing above this line changes meaning.
    // It has to be here: before esp_rtos::start / core1 (map_psram suspends
    // the data cache), before any flashmap::map (PSRAM and flash mappings
    // share the S3's DBUS MMU table and esp-hal maps PSRAM after the LAST
    // valid entry), and long before WiFi. Reads the flash clock out of the
    // image header via the driver the boot guard already borrowed.
    #[cfg(feature = "psram-arena")]
    psram::init(p.PSRAM, ota_flash.as_mut());

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    // Dual-core: the ProCpu's half of the flash fence (core1.rs) — armed
    // before the AppCpu exists so the first fence from either side works.
    #[cfg(multi_core)]
    core1::install_pro_parker(sw_int.software_interrupt2);
    // Dual-core: black box from the previous run + the RTC watchdog that
    // turns a wedged ProCpu into a reset with a diagnosis (core1.rs).
    #[cfg(multi_core)]
    {
        core1::boot_blackbox();
        core1::arm_watchdog(p.RTC_TIMER);
        spawner.spawn(core1::watchdog_task().unwrap());
    }

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
    // (The strip output itself is wired further down, after the flash
    // driver is up: its DATA pin is a stored setting — Gitea #154.)

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
    // nobody noticed until a real via-WLED install (2026-08-16). Which is
    // why it is a per-board cargo FEATURE and not an edit (#501): the boards
    // that ship it are `wled-takeover` in firmware/Cargo.toml and
    // `board_takeover` in firmware/board-target.sh, and tools/image-check.sh
    // fails the build in both directions if those two ever disagree with
    // what actually got linked. Never comment this call out instead.
    #[cfg(feature = "wled-takeover")]
    takeover::maybe_takeover();
    // Map the assets partition through the cache MMU (flashmap.rs) before
    // the TOC parse so init() and every asset response read it as memory.
    // After takeover (the region is ours only under our table) and after
    // ota::init (the self-check's read_nor half needs the driver).
    assets::map_region();
    assets::init();
    patterns::init();
    // Partition-layout migration (Gitea #501): the device may still carry
    // the pre-#501 1 MiB-slot table. AFTER patterns::init, which resolves
    // and scans the OLD store — the migrator carries exactly the records
    // the device was already serving — and before anything writes to it.
    // Reboots into the new layout when it runs; a no-op (one table read)
    // on every boot after that.
    migrate::maybe_migrate();
    playlist::init(); // after patterns::init (shares the storage partition)
    devicemap::init();
    layout::init();
    outpal::init(); // device output palette (also a reserved-key blob)
    scenes::init(); // scene records (one reserved-key blob, like the playlist)
    // Host-set text slots (Gitea #745). HERE, before any task spawns: it is
    // the only point where writing `luxel_core::text`'s lock-free
    // single-writer table is unconditionally sound, and it puts the text in
    // place before the first frame a resuming playlist of scenes renders.
    // ~1.3 KB worst case, in the same class as the palette/Layout blobs
    // above — not the multi-KB burst `resume::resume_task` defers past DHCP.
    textslots::init();
    } else {
        println!("LUXEL_NO_OTA: ota disabled");
    }

    // The persisted settings record, read once here — AFTER ota::init, which
    // installs the flash driver `assets::read_chunk` reads through (before
    // it the read silently answers "no record" and every setting boots at
    // its default; that cost one debug cycle on 2026-09-02). The strip DATA
    // pin is the one setting the wiring below needs (Gitea #154); the rest
    // seed the runtime atomics further down.
    let stored = config::read_device();

    #[cfg(not(feature = "hub75"))]
    let out = {
        let spi = Spi::new(
            p.SPI2,
            SpiConfig::default()
                .with_frequency(Rate::from_hz(DEFAULT_PROTOCOL.spi_hz()))
                .with_mode(Mode::_0),
        )
        .expect("spi init");
        // CLK is a typed board constant (SK9822 only; WS2812 boards leave it
        // unconnected). DATA is runtime data: a stored override that passes
        // the board's pin tables (board.rs) wins, else the board default.
        #[cfg(feature = "board-c3-devkit")]
        let spi = spi.with_sck(p.GPIO6);
        #[cfg(feature = "board-athom-music")]
        let spi = spi.with_sck(p.GPIO5);
        #[cfg(feature = "board-pixelblaze-v3")]
        let spi = spi.with_sck(p.GPIO18);
        // generic classic-ESP32: VSPI defaults — most WROOM boards break these out
        #[cfg(feature = "board-esp32-generic")]
        let spi = spi.with_sck(p.GPIO18);
        // S3/C6 devkits: the chip's SPI2 (FSPI) IO_MUX pins. UNTESTED ON METAL.
        #[cfg(feature = "board-s3-devkit")]
        let spi = spi.with_sck(p.GPIO12);
        #[cfg(feature = "board-c6-devkit")]
        let spi = spi.with_sck(p.GPIO6);
        let want = stored.and_then(|c| c.data_pin);
        let data_pin = match want {
            Some(pin) if board::data_pin_ok(pin) => pin,
            Some(pin) => {
                println!(
                    "settings: stored data pin GPIO{} is not usable on this board — using GPIO{}",
                    pin,
                    board::DEFAULT_DATA_PIN
                );
                board::DEFAULT_DATA_PIN
            }
            None => board::DEFAULT_DATA_PIN,
        };
        shared::DATA_PIN.store(data_pin, Ordering::Relaxed);
        shared::set_want_data_pin(want);
        println!(
            "strip data pin: GPIO{} ({})",
            data_pin,
            if want.is_some() { "configured" } else { "board default" }
        );
        // SAFETY: `data_pin_ok` excludes every pin the firmware names by type
        // (this section, board::RESERVED_PINS) and pattern GPIO (gpio.rs)
        // excludes DATA_PIN in turn, so this is the pad's only owner. The
        // typed default (e.g. `p.GPIO18`) is simply never used.
        let spi = spi.with_mosi(unsafe { esp_hal::gpio::AnyPin::steal(data_pin) });

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
        output::SpiStripOutput::new(spi)
    };
    // The board's SECOND strip output (Gitea #474): one driver instance per
    // configured output, each carrying a consecutive run of the ONE pixel
    // space. Built only when the stored Layout has an `out 1` line — with
    // none, SPI3/DMA_SPI3 and the CLK2 pad are never touched and the board
    // behaves exactly as it did before. The Layout is already loaded here
    // (`layout::init()` above, with the rest of the store).
    #[cfg(multi_output)]
    let out = {
        let mut out = out;
        match layout::configured_output(1) {
            Some(o) if board::data_pin_ok(o.pin) => {
                let proto = Protocol::from_u8(o.proto);
                match Spi::new(
                    p.SPI3,
                    SpiConfig::default()
                        .with_frequency(Rate::from_hz(proto.spi_hz()))
                        .with_mode(Mode::_0),
                ) {
                    Ok(spi) => {
                        // CLK2 — the typed half of board::SECOND_CLK_PIN,
                        // which board.rs asserts is a reserved pad.
                        #[cfg(feature = "board-athom-music")]
                        let spi = spi.with_sck(p.GPIO16);
                        // SAFETY: as output 0's MOSI above — `data_pin_ok`
                        // excludes every pad the firmware names by type, and
                        // `gpio::pin_is_free` excludes DATA_PIN2 in turn, so
                        // this is the pad's only owner.
                        let spi = spi
                            .with_mosi(unsafe { esp_hal::gpio::AnyPin::steal(o.pin) })
                            .with_dma(p.DMA_SPI3);
                        shared::DATA_PIN2.store(o.pin, Ordering::Relaxed);
                        println!(
                            "output 1: GPIO{} {} {} px{}",
                            o.pin,
                            proto.name(),
                            o.count,
                            if o.rev { " reversed" } else { "" }
                        );
                        out.attach_second(spi, proto, o.order);
                    }
                    Err(_) => println!("output 1: spi init failed — not driven"),
                }
            }
            Some(o) => println!("output 1: GPIO{} is not usable on this board — not driven", o.pin),
            None => {}
        }
        out
    };
    // HUB75 panel over LCD_CAM (S3 only): the strip SPI is not wired at
    // all — DMA_CH0 feeds the panel's circular rescan instead. The pin map
    // is per-board and lives in board.rs with the rest of the board
    // identity. UNTESTED ON METAL on either panel board (#75).
    #[cfg(feature = "hub75")]
    let out = hub75::Hub75Output::new(p.LCD_CAM, board::hub75_pins!(p), p.DMA_CH0);
    // ---- end board wiring ----

    // Seed runtime settings from flash (else compile-time defaults) BEFORE the
    // render task spawns — it reads these once when it builds the engine and
    // configures SPI. (`stored` was read above, with the strip wiring.)
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
    shared::BRIGHT_CURVE.store(
        stored.map(|c| c.bright_curve_tenths).unwrap_or(0),
        Ordering::Relaxed,
    );
    shared::POST_BLUR.store(stored.map(|c| c.blur_pct).unwrap_or(0), Ordering::Relaxed);
    shared::POST_GLOW.store(stored.map(|c| c.glow_pct).unwrap_or(0), Ordering::Relaxed);
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
            Err(_) => println!("sensor uart init failed"),
        }
    }
    // Bisect knob: LUXEL_QUIET=1 at build time skips the render task
    // entirely (no SPI, no engine, no snapshot publishing) to isolate
    // whether it interacts with the esp32 radio crashes.
    if option_env!("LUXEL_QUIET").is_none() {
        // The frame sink: on a pipelined board (hub75 + dual core) the
        // driver moves to an output task on THIS core and the render task
        // keeps only the hand-off; everywhere else it owns the driver and
        // runs every post-VM stage inline. See pipeline.rs.
        #[cfg(pipelined)]
        let sink = {
            spawner.spawn(pipeline::output_task(out).unwrap());
            println!("output task: ProCpu (frame pipeline)");
            pipeline::RenderSide::new()
        };
        #[cfg(not(pipelined))]
        let sink = pipeline::DirectSink::new(out);
        // Dual-core boards run the render task on the AppCpu, on its own
        // executor (core1.rs): a frame no longer holds the CPU that WiFi,
        // the network stack and the web pool live on (Gitea #259, #260).
        // Single-core boards spawn it on the main executor exactly as
        // before. Everything else — the playlist task included — stays here.
        #[cfg(multi_core)]
        match core1::start(
            p.CPU_CTRL,
            sw_int.software_interrupt1,
            sw_int.software_interrupt3,
            move |s: Spawner| s.spawn(render_task(sink).unwrap()),
        ) {
            Ok(()) => println!("render task: AppCpu"),
            Err(init) => {
                println!("core1: stack alloc failed — render task stays on ProCpu");
                init(spawner);
            }
        }
        #[cfg(not(multi_core))]
        spawner.spawn(render_task(sink).unwrap());
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
    // "luxel-xxxxxx": the board default. It is the SSID as an AP, and the
    // DHCP hostname as a station unless the user has named the device
    // (devname.rs, Gitea #538) — the name is read here, once, because the
    // network stack takes the string at construction and never re-reads it.
    // #536 will make the AP's SSID (and its password) follow the same name.
    let mac_addr = esp_hal::efuse::base_mac_address();
    let mac = mac_addr.as_bytes();
    let mut ap_ssid = heapless::String::<32>::new();
    let _ = core::fmt::Write::write_fmt(
        &mut ap_ssid,
        format_args!("luxel-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5]),
    );
    devname::init(ap_ssid.as_str());
    let mut hostname = heapless::String::<32>::new();
    shared::with_device_name(|n| {
        let _ = hostname.push_str(n);
    });
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
            ap_ssid
        );
        (
            WifiConfig::AccessPoint(
                esp_radio::wifi::ap::AccessPointConfig::default().with_ssid(ap_ssid.as_str()),
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

    // WiFi buffer pools. esp-radio's defaults are throughput-tuned and
    // generous for a device whose traffic is a handful of small HTTP
    // requests plus DDP/E1.31 frames: 10 static RX buffers (~1.6 KB each,
    // allocated inside esp_wifi_init and NEVER freed), a 32-deep dynamic
    // RX pool, and AMPDU RX on with a 6-frame block-ack window. Measured
    // residual blob draw at idle was ~50 KB — the single biggest heap
    // consumer left on classic ESP32.
    //
    // `small-chip` (the RAM-constrained profile — docs/boards.md tiers)
    // trims it hard: static RX 10 -> 4 reclaims ~9.6 KB outright, AMPDU RX
    // off drops the block-ack reassembly buffers, and the dynamic RX cap
    // 32 -> 16 bounds the on-demand pool's worst case. TX counts are left
    // at the defaults on purpose: dynamic TX buffers are allocated on
    // demand, so lowering the cap reclaims nothing at idle and only buys
    // TX starvation under load.
    //
    // The DEFAULT build takes a milder version of the same trim: static RX
    // 10 -> 6 (+6.4 KB idle, measured on the Athom), and *only* that knob —
    // AMPDU RX stays on and the dynamic pool stays at 32, so RX throughput
    // on a busy network is untouched and only the never-freed idle
    // reservation shrinks. static_rx_buf_num is in any case the only knob
    // that reclaims anything at idle; the dynamic pool and the block-ack
    // buffers are on-demand allocations.
    //
    // Deliberately conservative, because the blob's allocations do NOT
    // null-check — an undersized pool under load is a StoreProhibited
    // panic, not a clean error. rx_ba_win stays at its default 6, which
    // still satisfies ControllerConfig::validate() against both trims
    // (6 < 32 and 6 < 2 x 6 for the default; 6 < 16 dynamic and 6 < 2 x 4
    // static for small-chip, so the pairing stays legal there too if AMPDU
    // RX is ever switched back on).
    //
    // Both settings are soak-backed on the Athom rig, not estimates — see
    // the 2026-08-22 UPDATES.md entries (small-chip) and Gitea #60 (default).
    let wifi_cfg = ControllerConfig::default().with_initial_config(config);
    #[cfg(not(feature = "small-chip"))]
    let wifi_cfg = wifi_cfg.with_static_rx_buf_num(6);
    #[cfg(feature = "small-chip")]
    let wifi_cfg = wifi_cfg
        .with_static_rx_buf_num(4)
        .with_dynamic_rx_buf_num(16)
        .with_ampdu_rx_enable(false);

    let controller = WifiController::new(p.WIFI, wifi_cfg).expect("wifi controller");

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
        provision::log_started(ap_ssid.as_str());
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
        #[cfg(not(pipelined))]
        println!(
            "fps: {}  heap free: {}",
            FPS.load(Ordering::Relaxed),
            esp_alloc::HEAP.free()
        );
        #[cfg(pipelined)]
        println!(
            "fps: {} rendered / {} output ({} frames dropped)  heap free: {}",
            FPS.load(Ordering::Relaxed),
            shared::OUT_FPS.load(Ordering::Relaxed),
            pipeline::DROPPED.load(Ordering::Relaxed),
            esp_alloc::HEAP.free()
        );
    }
}

/// Renders frames and drives the strip; picks up uploaded patterns between
/// The protocol the wire is currently encoding for.
pub(crate) fn cur_protocol() -> Protocol {
    Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed))
}

/// The 0-31 level the frame is actually encoded at.
///
/// Master power (the HA light switch) off = encode at brightness 0 (black on
/// every protocol) while the engine keeps ticking, so ON resumes mid-motion.
/// The brightness curve reshapes the dimmer here, at the single place the
/// wire value is decided, so the power-cap estimate and the encoded frame
/// agree on how bright the output is actually being driven.
pub(crate) fn out_brightness() -> u8 {
    if shared::POWER.load(Ordering::Relaxed) {
        luxel_core::outpipe::curve_brightness(
            BRIGHTNESS.load(Ordering::Relaxed),
            shared::BRIGHT_CURVE.load(Ordering::Relaxed),
        )
    } else {
        0
    }
}

/// Publish the running engine's EFFECTIVE geometry for `/api/status`'s
/// `geom` block (Gitea #464).
///
/// Called from the render task wherever the engine or the installed map can
/// have changed — not per frame: `Engine::pattern_dims` walks the bytecode
/// looking for coordinate-using bulk ops, which is a per-load cost, not a
/// per-frame one. With no engine resident the device map's own shape is
/// reported instead, so a panel board still reads as 64x64 while it is
/// frozen for an OTA.
fn publish_geom(engine: Option<&luxel_core::engine::Engine>) {
    let pixels = PIXEL_COUNT.load(Ordering::Relaxed);
    let (dims, grid, pattern_dims) = match engine {
        Some(e) => (e.installed_map().map_or(0, |m| m.dims), e.grid(), e.pattern_dims()),
        None => {
            let (d, g) = devicemap::shape();
            (d, g, 0)
        }
    };
    shared::set_geom(luxel_core::caps::Geom::derive(
        devicemap::source(),
        dims,
        grid,
        pixels,
        pattern_dims,
    ));
}

/// Blend the INCOMING pixel `src` over the OUTGOING pixel `dst` by `t` in
/// 0..=65536 (0 = all outgoing, 65536 = all incoming).
///
/// This was a local `blend_px(a, b, t) = (a*(65536-t) + b*t) >> 16`; the
/// kernel now lives in `luxel_core::compose` so the device, the mirror and
/// the playground share one blend. `Blend::Normal` is
/// `dst + ((src-dst)*t >> 16)` — the same expression rearranged, and the
/// shift is arithmetic in both, so they floor identically and a
/// single-pattern crossfade is bit-for-bit what shipped before scenes
/// (pinned by
/// `compose::tests::a_two_layer_stack_reproduces_the_crossfade_exactly`).
#[inline]
fn blend_over(dst: &mut [u8; 3], src: [u8; 3], t: i32) {
    luxel_core::compose::blend_px_mode(dst, src, luxel_core::scene::Blend::Normal, t);
}

/// The DEVICE output chain's per-board power model — the power cap models
/// the output stage, and strips conduct every pixel at once where a HUB75
/// panel time-multiplexes rows (see `outpipe::PowerModel`).
///
/// A function rather than a const since #401: the panel's scan depth is a
/// stored setting applied at boot, so it comes from what the DMA actually
/// booted with (`hub75::live_scan`, a relaxed atomic load — this is on the
/// per-frame path).
#[cfg(not(feature = "hub75"))]
pub(crate) fn power_model() -> luxel_core::outpipe::PowerModel {
    luxel_core::outpipe::PowerModel::Strip
}
#[cfg(feature = "hub75")]
pub(crate) fn power_model() -> luxel_core::outpipe::PowerModel {
    luxel_core::outpipe::PowerModel::Hub75 { scan: crate::hub75::live_scan() }
}

/// This frame's device output-chain settings, read out of the `/api/output`
/// globals (Gitea #466).
///
/// The chain itself is `luxel_core::outpipe::DeviceChain` — it used to be
/// `apply_outpipe` here, ~90 lines over functions that already lived in
/// `luxel-core`. Lifting it means the wasm playground runs the SAME chain as
/// the device (`lx_outpipe`), so a console preview no longer diverges from
/// the wire by the whole Settings page, and the scratch/LUT lifecycle
/// (Gitea #446/#476) became host-testable —
/// `crates/luxel-core/tests/outpipe_chain.rs` holds the old body verbatim and
/// asserts the two are byte-identical.
pub(crate) fn outpipe_settings() -> luxel_core::outpipe::ChainSettings {
    luxel_core::outpipe::ChainSettings {
        order: luxel_core::outpipe::ColorOrder(shared::COLOR_ORDER.load(Ordering::Relaxed)),
        gamma_tenths: shared::GAMMA_TENTHS.load(Ordering::Relaxed),
        cap_ma: shared::CAP_MA.load(Ordering::Relaxed),
        blur_pct: shared::POST_BLUR.load(Ordering::Relaxed),
        glow_pct: shared::POST_GLOW.load(Ordering::Relaxed),
        palette_pct: shared::POST_PALETTE_AMOUNT.load(Ordering::Relaxed),
        palette_epoch: shared::POST_PALETTE_EPOCH.load(Ordering::Relaxed),
    }
}

// `RUNTIME_FLOOR` and the array-budget arithmetic live in
// `luxel_core::budget` — that module carries the full rationale for both
// numbers. They are shared rather than local because the web editor imports
// the SAME constants through the wasm build to predict, before a push,
// whether a pattern will fit the device it is connected to (Gitea #15).
// One definition means the prediction can never drift from the device that
// enforces it.
use luxel_core::budget::RUNTIME_FLOOR;

/// The array-arena BYTE budget for a load starting now.
///
/// On a board with an external arena (Gitea #253) that is the arena's own
/// free space, not free DRAM: pattern arrays no longer come out of the main
/// heap, so charging them against it would leave 8 MB unusable. Everywhere
/// else this is exactly `budget::array_budget(HEAP.free())` as before, and
/// the post-load `RUNTIME_FLOOR` check still measures the main heap on
/// every board — what a pattern costs in DRAM is unchanged.
fn array_budget_now() -> usize {
    let free = esp_alloc::HEAP.free() as usize;
    #[cfg(feature = "psram-arena")]
    if let Some((arena_free, _)) = psram::stats() {
        return luxel_core::budget::external_array_budget(free, arena_free);
    }
    luxel_core::budget::array_budget(free)
}

/// PB's element ledger, except on a board whose arena is external — there
/// the byte budget is the real constraint and the element count is raised
/// out of the way (`budget::external_element_budget`; the arena's slot
/// vector is bounded separately by `vm::MAX_ARENA_SLOTS`).
fn element_budget(_byte_budget: usize) -> usize {
    #[cfg(feature = "psram-arena")]
    if psram::stats().is_some() {
        return luxel_core::budget::external_element_budget(_byte_budget);
    }
    luxel_core::vm::DEFAULT_ARRAY_BUDGET
}

fn budgeted_engine(prog: luxel_core::vm::Program, count: u32) -> Engine {
    // Arrays may consume free heap down to (but not past) the runtime
    // floor — byte-accurate (elements × 8 + per-array overhead), so one
    // big array isn't taxed for overhead only swarms of tiny ones pay.
    // See luxel_core::budget::array_budget for the slack + minimum rules.
    let budget = array_budget_now();
    // Wall clock at CONSTRUCTION so top-level clockHour()-family reads see
    // real time (SNTP may not have synced yet on early boot -> None -> 0,
    // same as a PB with no time source). The render loop keeps it fresh
    // per frame afterwards.
    let mut e = Engine::from_program_budgeted_at_ext(
        prog,
        count,
        1,
        budget,
        element_budget(budget),
        shared::wall_now_local(),
    );
    // Install the Layout's projection defaults BEFORE the first frame
    // (Gitea #465/#473): `pixelCount` under an along-axis projection is the
    // strip's length, and the pattern's top-level init has already run by
    // the time anything else could set it.
    e.set_projection(layout::projection());
    e
}

/// [`budgeted_engine`] + post-build floor check: a pattern that fits its
/// array budget but still leaves the heap under the floor (huge program,
/// long strip) is rejected — soak v5 showed a routine 8.5 KB jsonview
/// alloc panicking (= reboot) right after such a pattern loaded.
/// `Err(free_bytes_at_rejection)` — measured BEFORE the engine is dropped,
/// so error messages report the pressure that caused the rejection, not
/// the comfortable number after freeing.
pub(crate) fn try_budgeted_engine(
    prog: luxel_core::vm::Program,
    count: u32,
) -> Result<Engine, usize> {
    // A bare pattern replaces the WHOLE stack with one engine, and that
    // engine is the program `/api/status`'s scalar `jit` block describes
    // (Gitea #718). Resetting here rather than at the call sites is what
    // makes the per-slot table self-maintaining: every non-scene
    // activation — boot default, /api/code, store activate, library swap,
    // crossfade, pixel-count rebuild — funnels through this function, so a
    // new one cannot forget. A SCENE layer goes through
    // [`try_budgeted_layer`], which is armed for its own slot inside a
    // stack `scenes::build_runtime` has already reset.
    #[cfg(feature = "jit")]
    jit::single();
    try_budgeted_layer(prog, count)
}

/// [`try_budgeted_engine`] for ONE layer of the scene
/// `scenes::build_runtime` is assembling: the same floor check and the same
/// single compile hook, but the per-slot JIT recorder is already armed for
/// this layer (`jit::arm`) and the stack must NOT be reset out from under
/// the layers below it.
pub(crate) fn try_budgeted_layer(
    prog: luxel_core::vm::Program,
    count: u32,
) -> Result<Engine, usize> {
    #[allow(unused_mut)]
    let mut e = budgeted_engine(prog, count);
    let free = esp_alloc::HEAP.free() as usize;
    if free < RUNTIME_FLOOR {
        println!(
            "pattern rejected: {} B heap left after load (< {} floor)",
            free, RUNTIME_FLOOR
        );
        #[cfg(feature = "jit")]
        jit::note_interpreted("init-error");
        return Err(free); // drops the engine, freeing its heap
    }
    // Compile HERE and nowhere else: every activation — boot default,
    // /api/code, store activate, library swap, crossfade — funnels through
    // this function, and it runs AFTER init, which is what makes the kind
    // annotations trustworthy (docs/jit-design.md §2.3, §5 "Lifecycle").
    // Whole-program or nothing, and a refusal leaves a working interpreted
    // pattern rather than a failed one.
    #[cfg(feature = "jit")]
    jit::try_compile(&mut e);
    Ok(e)
}

/// Record what the engine just built costs the heap, for `/api/status`
/// `engine_heap` (Gitea #287). `free_before` is free heap sampled with NO
/// engine resident — every load path drops the outgoing engine before it
/// decodes — so the difference is the whole resident cost of the new
/// pattern: its `Program` tables, any owned code words, the VM globals, the
/// pixel buffer and whatever the array arena settled at.
///
/// The playground adds this back to `heap_free` to predict the NEXT swap:
/// the heap the incoming pattern is measured against is the heap AFTER this
/// one is dropped (`luxel_core::budget::load_base`). Crossfades don't record
/// (the outgoing engine is still alive by design), so the value there is
/// simply the last clean measurement.
fn note_engine_heap(free_before: usize) {
    let now = esp_alloc::HEAP.free() as usize;
    shared::ENGINE_HEAP.store(free_before.saturating_sub(now) as u32, Ordering::Relaxed);
    // `free_before` was sampled with NO engine resident, which is exactly
    // `budget::load_base` — measured, not reconstructed from two numbers
    // that overlap during a swap. That overlap is why `caps.layers` must not
    // recompute it in the HTTP handler: a `/api/status` landing between the
    // teardown and the build sees the freed heap AND the outgoing engine's
    // cost, and reads 15 KB too high (seen on the panel, 2026-09-24).
    shared::note_heap_base(free_before as u32);
}

/// Drop the crossfade's outgoing engine AND release the arena pin that kept
/// its extent from being moved or freed while it was still executing from it
/// (patterns.rs' pin set, Gitea #260). Order matters — the engine goes
/// first, the pin second; never `prev = None` on its own.
fn drop_prev(prev: &mut Option<Engine>, prev_scene: &mut Option<scenes::Runtime>) {
    *prev = None;
    *prev_scene = None;
    patterns::unpin_prev();
}

/// Pattern layers the CURRENT stack holds resident: a scene's, or 1 for a
/// plain pattern. The left-hand side of the transition rule.
fn cur_pattern_layers(scene: &Option<scenes::Runtime>, engine: &Option<Engine>) -> usize {
    match scene {
        Some(rt) => rt.pattern_layers(),
        None => usize::from(engine.is_some()),
    }
}

/// Contract §3: a transition whose OUTGOING and INCOMING stacks together
/// need more pattern layers than `caps.layers` is a HARD CUT. There is
/// neither the heap for both nor — on a JIT board — a second exec half
/// (`jit.rs HALVES = 2`), and a fade that cannot build its incoming engines
/// is worse than no fade.
fn transition_ms(ms: u32, out_layers: usize, in_layers: usize) -> u32 {
    if out_layers + in_layers > crate::server::scene_layer_cap() as usize {
        0
    } else {
        ms
    }
}

/// Republish the scene-layer arena pins for everything resident — the live
/// scene AND the outgoing one a crossfade is still rendering from.
fn republish_layer_pins(scene: &Option<scenes::Runtime>, prev: &Option<scenes::Runtime>) {
    let mut ids: alloc::vec::Vec<alloc::string::String> = alloc::vec::Vec::new();
    for rt in [scene, prev].into_iter().flatten() {
        for id in &rt.pinned {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    patterns::set_layer_pins(&ids);
}

/// `/api/status` `engines` — what [`shared::ENGINE_HEAP`]'s sum is over.
fn note_engines(scene: &Option<scenes::Runtime>, engine: &Option<Engine>) {
    let n = u32::from(engine.is_some()) + scene.as_ref().map_or(0, scenes::Runtime::engines);
    shared::ENGINES.store(n, Ordering::Relaxed);
    // `/api/status`'s `jit` block describes the RESIDENT stack, so it has
    // to fall back to `state:"none"` the moment that stack empties — a
    // freeze, a rejected load, or a board nobody has given a pattern
    // (Gitea #718/#744). This is the one place that knows, and it is
    // called every loop iteration rather than only when the geometry
    // changed; `note_resident` is a single atomic load unless `n` is 0.
    #[cfg(feature = "jit")]
    jit::note_resident(n);
}

/// Install a stored scene as the live stack. Returns true when a crossfade
/// started, so the caller can stamp its clock.
///
/// Same discipline as `Msg::Library` — only the id travelled, and every layer
/// decodes in place from its mapped extent — except that the stack is N
/// engines deep, so a hard cut has to free ALL of them before the build
/// starts, where the most heap is free.
///
/// Deliberately NOT inlined into the render task's async body: its locals (a
/// `Scene`, the pin list, the built `Runtime`) would otherwise land in the
/// task's future, which is a `.bss` static that comes out of the main-task
/// stack floor. Measured at 2,232 B of `.stack` when it was an arm.
#[inline(never)]
fn install_scene(
    id: &str,
    ms: u32,
    engine: &mut Option<Engine>,
    scene: &mut Option<scenes::Runtime>,
    prev: &mut Option<Engine>,
    prev_scene: &mut Option<scenes::Runtime>,
) -> bool {
    let Some(sc) = scenes::get(id) else {
        println!("scene: {} is gone — activation dropped", id);
        set_vmerr(Some(alloc::string::String::from("no such scene")));
        return false;
    };
    let ms = transition_ms(
        ms,
        cur_pattern_layers(scene, engine),
        luxel_core::scene::pattern_layers(&sc),
    );
    drop_prev(prev, prev_scene); // never THREE stacks
    if ms == 0 {
        *engine = None;
        *scene = None;
    } else {
        patterns::pin_prev_from_running();
        *prev = engine.take();
        *prev_scene = scene.take();
    }
    // Measurable only on a hard cut; a fade deliberately keeps the outgoing
    // stack alive, so it leaves the last clean measurement alone (#287).
    let free_before = (ms == 0).then(|| esp_alloc::HEAP.free() as usize);
    let keep: alloc::vec::Vec<alloc::string::String> =
        prev_scene.as_ref().map(|r| r.pinned.clone()).unwrap_or_default();
    let grid = scene_grid(engine);
    let (rt, base, err) = scenes::build_runtime(
        &sc,
        PIXEL_COUNT.load(Ordering::Relaxed),
        Some(grid),
        &keep,
    );
    *engine = base;
    *scene = Some(rt);
    republish_layer_pins(scene, prev_scene);
    scenes::set_active(id);
    if let Some(e) = engine.as_ref() {
        publish(&CONTROLS_JSON, jsonview::controls_json(e));
    }
    if let Some(free_before) = free_before {
        note_engine_heap(free_before);
    }
    // Identity and read-back follow the BASE layer's pattern, so
    // `/api/pattern` and the console still show something real.
    let base_id = sc
        .layers
        .iter()
        .find(|l| l.kind() == luxel_core::scene::LayerKind::Pattern)
        .and_then(|l| l.pattern_id())
        .unwrap_or("");
    if !base_id.is_empty() {
        let (src_len, hash) = patterns::source_stat(base_id).unwrap_or((0, 0));
        shared::set_pattern_hash_raw(hash);
        shared::set_current_pattern_id(base_id);
        shared::set_current_library(src_len, 0);
        patterns::pin_running(base_id);
    }
    set_vmerr(err);
    devicemap::mark_dirty();
    if prev.is_none() && prev_scene.is_none() {
        patterns::unpin_prev();
    }
    ms > 0
}

/// The grid the compositor addresses: the engine's effective one (it knows
/// the fabricated square grid a Matrix layout implies), else the installed
/// map's. An empty grid makes every compositor kernel a silent no-op, the
/// same contract `bulk.rs` has — a scene on an irregular strip draws
/// nothing rather than erroring.
fn scene_grid(engine: &Option<Engine>) -> luxel_core::outpipe::GridMap {
    engine
        .as_ref()
        .and_then(|e| e.grid())
        .or_else(|| devicemap::shape().1)
        .unwrap_or(luxel_core::outpipe::GridMap {
            w: 0,
            h: 0,
            serpentine: false,
        })
}

/// [try_budgeted_engine] plus the user-facing "too large" vmerr on failure.
fn engine_or_vmerr(p: luxel_core::vm::Program) -> Option<Engine> {
    match try_budgeted_engine(p, PIXEL_COUNT.load(Ordering::Relaxed)) {
        Ok(e) => Some(e),
        Err(left) => {
            let mut m = alloc::string::String::new();
            jsonview::push_piece(&mut m, "pattern too large for this device — it left only ");
            jsonview::push_u32(&mut m, (left / 1024) as u32);
            jsonview::push_piece(&mut m, " KB of heap free (the firmware needs ");
            jsonview::push_u32(&mut m, (RUNTIME_FLOOR / 1024) as u32);
            jsonview::push_piece(&mut m, " KB to keep running)");
            set_vmerr(Some(m));
            None
        }
    }
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

/// Hand a frame to the sink. On a pipelined board this can WAIT for the
/// output task to give the travelling buffer back — that wait is the vsync
/// pacing (Gitea #387) — so the call is a suspension point. On every other
/// board the sink is synchronous and there is nothing to await; going
/// through a macro keeps the `.await` (and its state machine) out of those
/// builds entirely.
#[cfg(pipelined)]
macro_rules! emit {
    ($sink:expr, $frame:expr, $grid:expr) => {
        $sink.emit($frame, $grid).await
    };
}
#[cfg(not(pipelined))]
macro_rules! emit {
    ($sink:expr, $frame:expr, $grid:expr) => {
        $sink.emit($frame, $grid)
    };
}
#[cfg(pipelined)]
macro_rules! emit_staged {
    ($sink:expr, $grid:expr) => {
        $sink.emit_staged($grid).await
    };
}
#[cfg(not(pipelined))]
macro_rules! emit_staged {
    ($sink:expr, $grid:expr) => {
        $sink.emit_staged($grid)
    };
}

/// frames. Yields to the network tasks after every frame.
///
/// Output-agnostic: everything past the VM — the preview copy, the output
/// pipeline and the wire itself (encode buffers, DMA, clock reconfiguration)
/// — lives behind [`pipeline::RenderSink`], which on a pipelined board is
/// only a hand-off to the other core (Gitea #306). This task decides WHEN to
/// reconfigure/resize and keeps the engine-freeing retry policy on
/// tight-heap failures.
#[embassy_executor::task]
async fn render_task(mut sink: pipeline::RenderSink) -> ! {
    // Heap discipline: exactly ONE decoded Program lives at a time — inside
    // the engine. Rebuilds (pixel-count change, map clear) re-decode from
    // the running pattern's blob rather than keeping a second Program
    // resident; a resident copy + per-rebuild clones is what OOM'd soak v5
    // (Programs with debug info are several times their blob size). That blob
    // no longer sits in RAM either — it lives in the flash read-back slot
    // (shared::current_bc), read into a TRANSIENT Vec only for the rebuild.
    // deserialize_lean: no debug info on-device — halves a Program's RAM.
    //
    // NOTHING IS RESIDENT AT BOOT (Gitea #744). A shipped image carries no
    // default pattern, so the render task starts with `engine: None`: the
    // frame branch below is skipped, one black frame is clocked out (see
    // `blanked`), and the loop idles at 20 Hz until `playlist::init` /
    // `resume::resume_task` / an HTTP swap sends the first `Msg`. That state
    // costs ZERO heap — which is the property the rodata default used to buy
    // and the reason it survived this long: its code and constant pool were
    // mapped rodata (Gitea #260), so first boot never allocated for them.
    // Having no engine at all is strictly cheaper still.
    //
    // A build that passed `LUXEL_DEFAULT_PATTERN` (build.rs, the QEMU JIT
    // gate) keeps the old behaviour exactly.
    #[cfg(not(default_pattern))]
    let mut engine: Option<Engine> = None;
    #[cfg(default_pattern)]
    let mut engine = {
        // _static: PATTERN_BC is rodata the bootloader already maps, so the
        // built-in default's code and constant pool cost NO heap at all —
        // the Program is its header tables (Gitea #260).
        // Bracket the first build too, so `/api/status` reports
        // `engine_heap` from boot rather than only after the first swap
        // (Gitea #287).
        let boot_free = esp_alloc::HEAP.free() as usize;
        let engine = match luxel_core::bytecode::deserialize_lean_static(PATTERN_BC) {
            Ok(p) => {
                // The boot default is the ONE activation that does not go
                // through `try_budgeted_engine`: there is no heap floor to
                // fail against, because the blob is rodata and there is
                // nothing to fall back TO. So the JIT hook is repeated here —
                // without it the built-in pattern would be the only one on the
                // device that never compiled (Gitea #658).
                #[allow(unused_mut)]
                let mut e = budgeted_engine(p, PIXEL_COUNT.load(Ordering::Relaxed));
                // …and so is the per-slot recorder's arming, for the same
                // reason: the built-in default is a bare pattern, so it is
                // layer 0 of a one-layer stack (Gitea #718).
                #[cfg(feature = "jit")]
                jit::single();
                #[cfg(feature = "jit")]
                jit::try_compile(&mut e);
                Some(e)
            }
            Err(e) => {
                println!("embedded pattern bytecode error (build bug?): {}", e);
                None
            }
        };
        if engine.is_some() {
            note_engine_heap(boot_free);
        }
        // The boot default is `&'static` rodata: read-back serves it
        // directly, no heap and no flash write. Every later swap repoints
        // this at flash.
        shared::set_current_default(PATTERN, PATTERN_BC);
        engine
    };
    // Rebuild the engine from the running blob at the current pixel count.
    // The blob comes from wherever read-back currently points: the rodata
    // default (borrowed, no alloc), the flash slot, or the pattern store for
    // a library pattern — the latter two are transient fallible Vecs dropped
    // as soon as the Program is built. A flash-busy read (or a library
    // pattern deleted mid-session) yields None and the engine stays paused
    // until the next swap — never a panic.
    // Every mapped source is decoded with `deserialize_lean_static`: the
    // rebuilt Program BORROWS its code and constant pool out of flash and
    // costs only its header tables (Gitea #260). A rebuild's engine is as
    // long-lived as a swap's, so this is where a copy would hurt most.
    // Only the flash-controller fallbacks (a transient Vec) copy.
    let rebuild = || {
        let count = PIXEL_COUNT.load(Ordering::Relaxed);
        // Both callers drop the engine first, so this is free heap with none
        // resident — the bracket `engine_heap` needs, and the only place it
        // gets refreshed when the PIXEL COUNT changes (Gitea #287).
        let free_before = esp_alloc::HEAP.free() as usize;
        let built = (|| match shared::current_bc() {
            shared::BcLoc::Default(b) => luxel_core::bytecode::deserialize_lean_static(b)
                .ok()
                .and_then(|p| try_budgeted_engine(p, count).ok()),
            // the ad-hoc slot: mapped (no Vec) when the raw half is, else
            // a transient read through the flash controller
            shared::BcLoc::Flash(len) => {
                let p = match crate::patterns::current_slot_code(len) {
                    Some(code) => luxel_core::bytecode::deserialize_lean_static(code).ok()?,
                    None => {
                        let bc = crate::patterns::read_current_bc(len)?;
                        luxel_core::bytecode::deserialize_lean(&bc).ok()?
                    }
                };
                try_budgeted_engine(p, count).ok()
            }
            // library pattern: the store's CURRENT blob (not the snapshot
            // length — a re-save may have changed it, and the store's copy
            // is the truth), from its mapped arena extent when it has one.
            // The pattern is the running one, so it is already pinned.
            shared::BcLoc::Library(_) => {
                let id = shared::get_current_pattern_id();
                let p = match crate::patterns::code_of(&id) {
                    Some(code) => luxel_core::bytecode::deserialize_lean_static(code).ok()?,
                    None => crate::patterns::with_code(&id, |bc| {
                        luxel_core::bytecode::deserialize_lean(bc).ok()
                    })??,
                };
                try_budgeted_engine(p, count).ok()
            }
            shared::BcLoc::Gone => None,
        })();
        if built.is_some() {
            note_engine_heap(free_before);
        }
        built
    };
    if let Some(eng) = engine.as_ref() {
        publish(&CONTROLS_JSON, jsonview::controls_json(eng));
    }

    // Apply the seeded protocol — flash may specify one different from the
    // boot-time default the SPI was constructed with.
    if let Err(e) = sink.set_protocol(cur_protocol()) {
        // expected on fixed-format drivers (HUB75): the wire ignores the
        // protocol setting entirely
        println!("output: protocol config not applied: {}", e);
    }
    if !sink.resize(PIXEL_COUNT.load(Ordering::Relaxed) as usize) {
        // the driver retries lazily per frame once heap frees up
        println!("encode buffer alloc failed at boot — output paused");
    }
    // crossfade: the outgoing engine + blend timing + a reusable blend buffer
    let mut prev: Option<Engine> = None;
    let mut blend_start = Instant::now();
    let mut blend_ms: u32 = 0;
    // The live scene, when one is active (Gitea #478). `engine` above is
    // still the render task's PRIMARY engine — for a scene it renders the
    // first pattern layer (`scenes::Slot::Base`), so controls, vars,
    // sensors, events, the projection override and the published geometry
    // all keep working unchanged. A single pattern is a one-layer scene with
    // no record.
    let mut scene: Option<scenes::Runtime> = None;
    let mut prev_scene: Option<scenes::Runtime> = None;
    // Composite buffer for an OUTGOING scene during a crossfade. A bare
    // outgoing pattern needs none — its engine's own frame is the blend
    // source, exactly as before scenes existed — so this stays empty on
    // every board that never crossfades between scenes.
    let mut fade_buf: alloc::vec::Vec<[u8; 3]> = alloc::vec::Vec::new();
    // Real GPIO behind the pattern's pin builtins (Gitea #177 item 4):
    // synced with the running engine between frames, see gpio.rs.
    let mut pins = gpio::PinHost::new();
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    // Per-stage frame timing (Gitea #260): µs accumulated over the current
    // one-second window, averaged into shared::{FRAME,VM,PIPE,OUT}_US on the
    // same tick that publishes FPS. `timed_frames` is the divisor and is NOT
    // `frames` — the latter counts every loop iteration (live input, idle
    // with no engine); only the pattern branch below is instrumented.
    let mut timed_frames: u32 = 0;
    let mut frame_sum: u64 = 0;
    let mut vm_sum: u64 = 0;
    let mut pipe_sum: u64 = 0;
    let mut out_sum: u64 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();
    let mut sensor_seen: u32 = 0;
    // last reported vmerr site (fn, pc) — dedupes the per-frame report
    let mut vmerr_seen: Option<(u16, u32)> = None;
    // `/api/status` geom (Gitea #464): republished after any iteration that
    // could have swapped the engine or the map, which is every inbox message
    // and every map change — never per frame (see publish_geom). Starts true
    // so the boot engine's shape is published on the first pass.
    let mut geom_dirty = true;
    // "Nothing playing" is DARK, not "whatever was on the wire last"
    // (Gitea #744). Nothing is emitted while no engine and no scene are
    // resident, and an LED holds its last latched frame for as long as it
    // has power — so a reboot with nothing to resume used to need the
    // built-in default just to overwrite the previous session's pixels.
    // This is the black frame that replaces it: `Some(n)` = a black frame
    // for `n` pixels has been clocked out and nothing has been drawn since,
    // so the wire is already dark. Edge-triggered, so the idle path stays a
    // 20 Hz sleep rather than a 20 Hz strip write.
    let mut blanked: Option<usize> = None;
    // …except for an OTA/upload `Msg::Freeze`, whose whole contract is that
    // the strip HOLDS its last frame while its heap is handed over
    // (server.rs). Blanking there would turn every firmware update into a
    // visible blackout. Cleared by the next message that replaces the
    // stack, so a freeze followed by a swap that fails still goes dark.
    let mut frozen = false;

    loop {
        // Liveness for the RTC watchdog, which is fed from the OTHER core
        // (core1::watchdog_task, ProCpu): once per ITERATION, so an idle
        // loop with no engine — or a rejected pattern rendering nothing —
        // still counts as alive, and only a wedge stops it. No-op on
        // single-core boards, where a wedge here stops the feeder too
        // (Gitea #603).
        core1::beat();
        while let Ok(msg) = MSG_QUEUE.try_receive() {
            // any of these can replace, free or re-shape the engine
            geom_dirty = true;
            // A message that REPLACES the stack frees the staging buffer with
            // it (Gitea #704), before anything is decoded or built: the 12 KB
            // it holds at 4096 px is the difference between the incoming
            // pattern's JIT compiling and falling back to the interpreter.
            // Only these — a `Var` or a `TextSlot` arriving while a scene is
            // live must NOT pull the buffer out from under it.
            // A scene takes it back after its own teardown (`install_scene`)
            // and a crossfade does so fallibly on its first frame.
            if matches!(
                msg,
                Msg::Code { .. }
                    | Msg::Library { .. }
                    | Msg::Crossfade { .. }
                    | Msg::Scene { .. }
                    | Msg::Config(_)
                    | Msg::Freeze
            ) {
                sink.release_stage();
                // …and the same set decides whether the strip is being held
                // deliberately (a freeze) or is simply unlit (Gitea #744).
                frozen = matches!(msg, Msg::Freeze);
            }
            match msg {
                Msg::Code { env, id } => {
                    // Envelope-validated by the sender. Drop the outgoing
                    // engine BEFORE decoding the new program — peak heap
                    // lands here, where the most is free.
                    engine = None;
                    scene = None; // a bare pattern replaces the whole stack
                    drop_prev(&mut prev, &mut prev_scene);
                    // Free heap with no engine resident — the upload envelope
                    // is the only thing alive here and it is transient, so add
                    // it back. This is the base the NEXT load will start from,
                    // and what `engine_heap` is measured against (Gitea #287).
                    let free_before = esp_alloc::HEAP.free() as usize + env.len();
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
                            if let Some(e) = engine_or_vmerr(p) {
                                publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                                engine = Some(e);
                                note_engine_heap(free_before);
                                // this Program owns its words (the envelope
                                // was a Vec) — an empty id clears the pin
                                patterns::pin_running(&id);
                                set_vmerr(None);
                                vmerr_seen = None;
                                last = Instant::now();
                                devicemap::mark_dirty(); // re-apply the installed map
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
                    scene = None;
                    drop_prev(&mut prev, &mut prev_scene);
                    println!("engine frozen (heap released)");
                }
                Msg::Control(name, values) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_control(&name, &values);
                    }
                }
                // The ONE writer of `luxel_core::text`'s slot table
                // (Gitea #485): it is lock-free single-writer, and on a
                // dual-core board the web and MQTT tasks are on the other
                // core. Every resident engine and the compositor's `slot`
                // text source read that table directly, so one write here
                // reaches the whole stack with no per-engine copy.
                Msg::TextSlot { n, text } => luxel_core::text::set_slot(n, &text),
                Msg::Var(name, value) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_var(&name, value);
                    }
                }
                // Live pixel-count change (no reboot): resize the output
                // buffers and rebuild the engine at the new count from the
                // current source. This task is the sole writer of PIXEL_COUNT.
                Msg::Config(count) => {
                    let count = count.clamp(1, MAX_PIXELS);
                    PIXEL_COUNT.store(count, Ordering::Relaxed);
                    engine = None; // free before re-decoding (peak heap)
                    // Every layer engine is built at the OLD pixel count and
                    // the compositor's scratch is grid-sized: a live resize
                    // tears the scene down and revives the single-pattern
                    // resume path, which is what `rebuild()` below restores.
                    scene = None;
                    drop_prev(&mut prev, &mut prev_scene);
                    // resize AFTER freeing the engines — at 2048 px the new
                    // buffer is a multi-KB alloc that wants the peak heap too
                    if !sink.resize(count as usize) {
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
                    if let Err(e) = sink.set_protocol(p) {
                        println!(
                            "output: protocol switch rejected: {} — staying on {}",
                            e,
                            cur_protocol().name()
                        );
                    } else {
                        PROTOCOL.store(p.as_u8(), Ordering::Relaxed);
                        let count = PIXEL_COUNT.load(Ordering::Relaxed);
                        if !sink.resize(count as usize) {
                            // heap too tight for the bigger encoding: free the
                            // engines (Freeze semantics — the strip holds its
                            // last frame) and retry; the next Code/Crossfade
                            // revives rendering
                            engine = None;
                            scene = None;
                            drop_prev(&mut prev, &mut prev_scene);
                            if !sink.resize(count as usize) {
                                println!(
                                    "encode buffer alloc failed ({} px) — output paused",
                                    count
                                );
                            }
                        }
                        last = Instant::now();
                        println!("protocol → {}", p.name());
                    }
                }
                // Crossfade to a new pattern (playlist transition): keep the
                // outgoing engine and blend over `ms`.
                Msg::Library { id, ms } => {
                    // A library swap (playlist, activate, MQTT, resume):
                    // nothing travelled but the id. Decode straight from
                    // the pattern's mapped bytecode extent — no envelope,
                    // no blob Vec, no source Vec anywhere in the lifecycle
                    // (docs/research/flash-mmap.md "The VM consumer").
                    // The store never writes on this path: the extent was
                    // written once, at save (the wear rule).
                    //
                    // A bare pattern is a one-layer stack, so the transition
                    // rule applies here too: fading a three-layer scene out
                    // into it would need four resident engines.
                    let ms = transition_ms(ms, cur_pattern_layers(&scene, &engine), 1);
                    if ms == 0 {
                        engine = None;
                        scene = None;
                    }
                    drop_prev(&mut prev, &mut prev_scene);
                    // Only a non-crossfading swap has nothing resident to
                    // subtract; a fade keeps the outgoing engine alive on
                    // purpose, so it leaves the last clean measurement alone.
                    let free_before = (ms == 0 && engine.is_none())
                        .then(|| esp_alloc::HEAP.free() as usize);
                    // Pin BEFORE reading the mapping: the Program borrows
                    // these bytes in place, and a save on the other core
                    // compacts the arena without asking (Gitea #260). The
                    // outgoing engine's extent is pinned too, for as long
                    // as the crossfade renders from it.
                    if ms > 0 && engine.is_some() {
                        patterns::pin_prev_from_running();
                    }
                    patterns::pin_code(&id);
                    let mut bc_len = 0usize;
                    let decoded = match crate::patterns::code_of(&id) {
                        Some(code) => {
                            bc_len = code.len();
                            luxel_core::bytecode::deserialize_lean_static(code).map_err(Some)
                        }
                        // no mapping (flashmap-off / refused self-check):
                        // read the extent into a transient Vec instead
                        None => match crate::patterns::bytecode_of(&id) {
                            Some(bc) => {
                                bc_len = bc.len();
                                luxel_core::bytecode::deserialize_lean(&bc).map_err(Some)
                            }
                            None => {
                                println!("library: pattern {} is gone — swap dropped", id);
                                Err(None)
                            }
                        },
                    };
                    if decoded.is_ok() {
                        // identity + read-back: both are directory fields
                        // (the source extent's length and its FNV-1a), so
                        // this reads no flash and allocates nothing
                        let (src_len, hash) = crate::patterns::source_stat(&id).unwrap_or((0, 0));
                        shared::set_pattern_hash_raw(hash);
                        shared::set_current_pattern_id(&id);
                        shared::set_current_library(src_len, bc_len);
                    }
                    match decoded {
                        Ok(p) => {
                            if let Some(e) = engine_or_vmerr(p) {
                                publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                                if ms > 0 && (engine.is_some() || scene.is_some()) {
                                    prev = engine.take();
                                    prev_scene = scene.take();
                                    blend_start = Instant::now();
                                    blend_ms = ms;
                                }
                                engine = Some(e);
                                scene = None;
                                scenes::set_active("");
                                republish_layer_pins(&scene, &prev_scene);
                                if let Some(free_before) = free_before {
                                    note_engine_heap(free_before);
                                }
                                patterns::pin_running(&id);
                                set_vmerr(None);
                                vmerr_seen = None;
                                last = Instant::now();
                                devicemap::mark_dirty();
                            }
                        }
                        Err(Some(e)) => {
                            println!("library bytecode decode failed: {}", e);
                            set_vmerr(Some(alloc::format!("{}", e)));
                        }
                        Err(None) => {}
                    }
                    // no fade started (ms == 0, nothing was running, or the
                    // decode/build failed): the old engine is either gone or
                    // still `engine`, and slot 1 still names what it borrows
                    if prev.is_none() {
                        patterns::unpin_prev();
                    }
                    // The decode window is over: whatever it produced is
                    // installed (slot 1 names it) or dropped. Slot 0 must
                    // NOT outlive it — a decode pin that is never released
                    // freezes that file for the rest of the boot and costs
                    // the store every byte below it (Gitea #388).
                    patterns::unpin_code();
                }
                Msg::Crossfade { env, ms, id } => {
                    // the outgoing engine stays alive on purpose (it's the
                    // blend source) — this is the one path where two
                    // programs coexist, bounded by the crossfade duration
                    drop_prev(&mut prev, &mut prev_scene); // but never THREE (a fade in flight)
                    let ms = transition_ms(ms, cur_pattern_layers(&scene, &engine), 1);
                    if ms == 0 {
                        scene = None;
                    }
                    // The outgoing engine may be a LIBRARY pattern executing
                    // in place from its arena extent, and persist_current_pattern
                    // below moves the current-pattern id off it — pin the
                    // extent here or a save on the other core may compact it
                    // out from under the blend source (Gitea #260).
                    if ms > 0 && engine.is_some() {
                        patterns::pin_prev_from_running();
                    }
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
                            if let Some(e) = engine_or_vmerr(p) {
                                publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                                if ms > 0 && (engine.is_some() || scene.is_some()) {
                                    prev = engine.take();
                                    prev_scene = scene.take();
                                    blend_start = Instant::now();
                                    blend_ms = ms;
                                }
                                engine = Some(e);
                                scene = None;
                                scenes::set_active("");
                                republish_layer_pins(&scene, &prev_scene);
                                patterns::pin_running(&id);
                                set_vmerr(None);
                                vmerr_seen = None;
                                last = Instant::now();
                                devicemap::mark_dirty();
                            }
                        }
                        Err(Some(e)) => {
                            println!("crossfade bytecode decode failed: {}", e);
                            set_vmerr(Some(alloc::format!("{}", e)));
                        }
                        Err(None) => {}
                    }
                    // no fade started: whatever the old engine borrows is
                    // still named by slot 1 (or it is gone entirely)
                    if prev.is_none() {
                        patterns::unpin_prev();
                    }
                }
                // Show a stored SCENE (Gitea #478). The whole handler is a
                // separate, synchronous fn: an arm this size inside the
                // async body puts every one of its locals — a `Scene`, the
                // pin list, the built `Runtime` — into the render task's
                // FUTURE, which is a `.bss` static and comes straight out of
                // the main-task stack floor (tools/stack-check.sh).
                Msg::Scene { id, ms } => {
                    if install_scene(
                        &id,
                        ms,
                        &mut engine,
                        &mut scene,
                        &mut prev,
                        &mut prev_scene,
                    ) {
                        blend_start = Instant::now();
                        blend_ms = ms;
                    }
                    vmerr_seen = None;
                    last = Instant::now();
                }
            }
        }

        // apply (or clear) the installed pixel map when it changed
        if devicemap::take_dirty() {
            geom_dirty = true;
            if devicemap::has_map() {
                if let Some(eng) = engine.as_mut() {
                    devicemap::apply(eng);
                }
                // the map is the DEVICE's, so it reaches every layer engine,
                // not just the base
                if let Some(rt) = scene.as_mut() {
                    rt.for_each_engine(devicemap::apply);
                }
            } else if scene.is_some() {
                // a scene's layers are not rebuildable from `rebuild()`
                // (that path knows one pattern); re-point the compositor and
                // leave the engines alone
                if let Some(rt) = scene.as_mut() {
                    let g = scene_grid(&engine);
                    rt.set_grid(g);
                }
            } else {
                // cleared → rebuild without a map (do not re-mark dirty)
                drop(engine.take()); // free before re-decoding (peak heap)
                engine = rebuild();
            }
        }

        // The ONE live-projection path (Gitea #465/#470/#598): the Layout's
        // defaults changed (`POST /api/layout proj1d …`), or something asked
        // for an override on the RUNNING pattern — a playlist item's `P`, or
        // a `proj` line on the same endpoint. An override goes into the slot
        // for the pattern's OWN dimensionality, so one token survives
        // whatever is running; the engine re-derives its plan from the triple
        // on every map install, so it also survives the `devicemap::apply`
        // that follows a swap. It runs AFTER the message drain, so a playlist
        // item's override lands on the engine that item just built. Either
        // way the effective geometry moves.
        if let Some(code) = layout::take_projection() {
            geom_dirty = true;
            let apply = |eng: &mut Engine| match ProjectionMode::from_u8(code) {
                Some(mode) => {
                    let mut p = eng.projection();
                    p.set(eng.preferred_dims(), mode);
                    eng.set_projection(p);
                }
                None => eng.set_projection(layout::projection()),
            };
            if let Some(eng) = engine.as_mut() {
                apply(eng);
            }
            if let Some(rt) = scene.as_mut() {
                rt.for_each_engine(apply);
            }
        }

        // the engine and the map have settled for this iteration — publish
        // the shape `/api/status` reports (Gitea #464), and what it is
        // holding (Gitea #479/#718). The COUNT is published every
        // iteration, not only when the geometry moved: a `Msg::Freeze` and
        // a rejected load both empty the stack without changing its shape,
        // and the `jit` block has to stop describing a program that is no
        // longer there.
        note_engines(&scene, &engine);
        if geom_dirty {
            geom_dirty = false;
            publish_geom(engine.as_ref());
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
            blanked = None; // something is about to be drawn
            let count = PIXEL_COUNT.load(Ordering::Relaxed) as usize;
            // the staging buffer is released while a plain pattern runs
            // (Gitea #704), so claim it here — fallibly, because the fill
            // below pushes infallibly
            if sink.reserve_stage(count) {
                shared::LIVE_PIXELS.lock(|c| {
                    let live = c.borrow();
                    let stage = sink.stage();
                    stage.clear();
                    for i in 0..count {
                        let p = i * 3;
                        stage.push(match live.get(p..p + 3) {
                            Some(px) => [px[0], px[1], px[2]],
                            None => [0, 0, 0],
                        });
                    }
                });
                let grid = engine.as_ref().and_then(|e| e.grid());
                // through the same sink as a pattern frame, so live input is
                // pipelined too where the board pipelines
                emit_staged!(sink, grid);
            }
            last = Instant::now(); // keep the pattern clock fresh for resume
        } else if engine.is_some() || scene.is_some() {
            blanked = None; // something is about to be drawn
            let now = Instant::now();
            let delta_us = (now - last).as_micros();
            last = now;
            // µs → 16.16 ms
            let mut delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

            // sync follower: converge on the leader clock — big offsets
            // jump, small ones slew by stretching this delta ≤ ±25%
            if shared::SYNC_MODE.load(Ordering::Relaxed) == 2 {
                if let (Some((_, lt, at)), Some(eng)) = (shared::sync_leader(), engine.as_mut()) {
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
            // pads ↔ pattern pin state, before the frame reads them (the
            // outgoing crossfade engine keeps its last view — it is on its
            // way out and must not fight the incoming one for a pad)
            if let Some(eng) = engine.as_mut() {
                pins.sync(eng);
            }
            // read before the frame borrow: `grid` is a Copy descriptor
            let grid = engine.as_ref().and_then(|e| e.grid());
            let count = PIXEL_COUNT.load(Ordering::Relaxed) as usize;
            let dt_ms = (delta.raw() >> 16).max(0) as u32;
            if scene.is_some() || prev_scene.is_some() {
                let g = scene_grid(&engine);
                if let Some(rt) = scene.as_mut() {
                    rt.set_grid(g);
                }
                if let Some(rt) = prev_scene.as_mut() {
                    rt.set_grid(g);
                }
            }
            let vm_t0 = Instant::now();
            // The blend lives in the sink's staging buffer: on a pipelined
            // board that buffer IS the one handed to the output task, so a
            // crossfade costs the pipeline no extra copy.
            // A crossfade blends INTO the staging buffer, which a plain
            // pattern released (Gitea #704) — claim it fallibly here, and
            // treat a refusal as a hard cut. Everything below fills the
            // stage with infallible `extend_from_slice`/`resize` calls, and
            // a fade that cannot be afforded is worth less than the frame it
            // would panic on (#702).
            let fading = (prev.is_some() || prev_scene.is_some())
                && t < 65536
                && sink.reserve_stage(count);
            let (vm_t1, pipe_us, out_us, handoff_us) = if fading {
                // The INCOMING stack lands in the stage…
                match scene.as_mut() {
                    Some(rt) => rt.render(sink.stage(), engine.as_mut(), delta, dt_ms, count),
                    None => {
                        let stage = sink.stage();
                        stage.clear();
                        stage.extend_from_slice(engine.as_mut().unwrap().frame(delta));
                    }
                }
                // …and the OUTGOING one is blended over it with `dst` = the
                // outgoing pixel, which is what makes this bit-identical to
                // the `blend_px` this replaces: `compose::blend_px_mode`'s
                // Normal arm is `b + ((l-b)*a >> 16)`, the same expression
                // rearranged (proved by `compose::tests`).
                //
                // A bare outgoing pattern already owns a full frame buffer —
                // its engine's — so only an outgoing SCENE needs `fade_buf`.
                if let Some(prt) = prev_scene.as_mut() {
                    prt.render(&mut fade_buf, prev.as_mut(), delta, dt_ms, count);
                    let stage = sink.stage();
                    for i in 0..stage.len().min(fade_buf.len()) {
                        let mut px = fade_buf[i];
                        blend_over(&mut px, stage[i], t);
                        stage[i] = px;
                    }
                } else if let Some(pe) = prev.as_mut() {
                    let px_old = pe.frame(delta);
                    let stage = sink.stage();
                    for i in 0..stage.len().min(px_old.len()) {
                        let mut px = px_old[i];
                        blend_over(&mut px, stage[i], t);
                        stage[i] = px;
                    }
                }
                let vm_t1 = Instant::now();
                let (p, o, w) = emit_staged!(sink, grid);
                (vm_t1, p, o, w)
            } else {
                let had_scene = prev_scene.is_some();
                drop_prev(&mut prev, &mut prev_scene); // fade finished
                if had_scene {
                    fade_buf = alloc::vec::Vec::new(); // grid-sized; not held between fades
                    republish_layer_pins(&scene, &prev_scene);
                }
                match scene.as_mut() {
                    Some(rt) => {
                        rt.render(sink.stage(), engine.as_mut(), delta, dt_ms, count);
                        let vm_t1 = Instant::now();
                        let (p, o, w) = emit_staged!(sink, grid);
                        (vm_t1, p, o, w)
                    }
                    None => {
                        // Neither a scene nor a crossfade is live, so the
                        // staging buffer is nobody's — give it back rather
                        // than hold 3 B/px of the layer budget until the
                        // next reboot (Gitea #704). A no-op after the first
                        // frame; live input reclaims it the same way the
                        // outpipe chain reclaims its scratch.
                        sink.release_stage();
                        let frame = engine.as_mut().unwrap().frame(delta);
                        let vm_t1 = Instant::now();
                        let (p, o, w) = emit!(sink, frame, grid);
                        (vm_t1, p, o, w)
                    }
                }
            };
            // stage timing — a few Instant reads and integer adds; no
            // formatting, allocation or float work on the hot path
            let frame_t1 = Instant::now();
            vm_sum += (vm_t1 - vm_t0).as_micros();
            pipe_sum += pipe_us as u64;
            out_sum += out_us as u64;
            // `frame_us` measures WORK. Under vsync pacing the hand-off can
            // block for most of a rescan waiting for the output task to give
            // the travelling buffer back (Gitea #387) — that is the pacing,
            // not the frame's cost, so it comes back out.
            frame_sum += (frame_t1 - now).as_micros().saturating_sub(u64::from(handoff_us));
            timed_frames += 1;
            if let Some(e) = engine.as_mut().and_then(Engine::take_error) {
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
                        let mut m = alloc::string::String::new();
                        jsonview::push_piece(&mut m, "line ");
                        jsonview::push_u32(&mut m, e.line);
                        jsonview::push_piece(&mut m, ":");
                        jsonview::push_u32(&mut m, e.col);
                        jsonview::push_piece(&mut m, ": ");
                        jsonview::push_piece(&mut m, &e.message);
                        m
                    };
                    println!("vmerr: {}", msg);
                    set_vmerr(Some(msg));
                }
            }
            // publish the engine clock (leader beacons + /api/sync)
            if let Some(eng) = engine.as_ref() {
                shared::set_engine_time_ms(eng.time_ms());
            }
        } else if frozen {
            // deliberately holding the last frame (OTA / an upload that
            // needed the heap) — the staging buffer is nobody's here
            // either (Gitea #704)
            sink.release_stage();
        } else {
            // Nothing is playing. Clock out ONE black frame so the wire is
            // dark rather than holding the previous session's last pixels,
            // then go back to idling (Gitea #744). Re-armed by a pixel-count
            // change, because the tail beyond the old count was never
            // written. Fallible and released immediately: this is the boot
            // path of a device with nothing stored, and the heap it runs on
            // is the one the first real pattern is about to want.
            let count = PIXEL_COUNT.load(Ordering::Relaxed) as usize;
            if blanked != Some(count) && sink.reserve_stage(count) {
                let stage = sink.stage();
                stage.clear();
                stage.resize(count, [0, 0, 0]);
                emit_staged!(sink, None);
                blanked = Some(count);
            }
            sink.release_stage();
        }

        frames += 1;
        if (Instant::now() - fps_mark).as_millis() >= 1000 {
            FPS.store(frames, Ordering::Relaxed);
            // averages over the window; 0 when no pattern frame ran (live
            // input drove the strip, or there is no engine)
            let n = timed_frames as u64;
            let avg = |sum: u64| if n == 0 { 0 } else { (sum / n) as u32 };
            let _ = (&pipe_sum, &out_sum);
            shared::FRAME_US.store(avg(frame_sum), Ordering::Relaxed);
            shared::VM_US.store(avg(vm_sum), Ordering::Relaxed);
            // On a pipelined board the compose runs on the other core and
            // publishes its own averages (pipeline::output_task); `pipe_sum`
            // and `out_sum` are zero here and must not overwrite them.
            #[cfg(not(pipelined))]
            {
                shared::PIPE_US.store(avg(pipe_sum), Ordering::Relaxed);
                shared::OUT_US.store(avg(out_sum), Ordering::Relaxed);
            }
            frames = 0;
            timed_frames = 0;
            frame_sum = 0;
            vm_sum = 0;
            pipe_sum = 0;
            out_sum = 0;
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
        // Budgeted through the same `budget::array_budget` as
        // `budgeted_engine` so a check can't starve the live engine — and
        // so the two can't drift; a stale-format blob reports its decode error
        // (the fix — recompile — is the same user action either way).
        if let Some(id) = playlist::preflight_next() {
            // (from the mapped arena slot when the pattern has one)
            let violation = match patterns::with_code(&id, |bc| {
                match luxel_core::bytecode::deserialize_lean(bc) {
                    Ok(p) => {
                        let budget =
                            array_budget_now();
                        luxel_core::engine::check_asserts(
                            &p,
                            PIXEL_COUNT.load(Ordering::Relaxed),
                            budget,
                        )
                    }
                    Err(e) => Some(alloc::format!("{}", e)),
                }
            }) {
                Some(v) => v,
                None => None, // deleted pattern; the scheduler logs it
            };
            if let Some(m) = &violation {
                println!("playlist preflight: {} → {}", id, m);
            }
            playlist::preflight_record(&id, violation);
        }

        // Pace the loop. On a panel this is the panel's own frame boundary
        // — one composed frame per rescan, nothing rendered only to be
        // overwritten (Gitea #387); everywhere else it is the 8 ms floor,
        // because an uncapped render loop starves the network tasks for
        // frame rate nobody can see. No engine (rejected pattern) = nothing
        // to render — idle properly instead of busy-spinning.
        if engine.is_none() {
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }
        pipeline::pace(Instant::now() - last).await;
    }
}

/// Waits for the OTA handler's signal, gives the TCP stack a moment to
/// flush the response, then resets into the freshly activated slot.
///
/// **A deliberate reboot is not a failed boot** (Gitea #771). Every
/// API-triggered reboot goes through this one signal — `/api/ota`,
/// `/api/apmode`, `/api/reboot`, `/api/wifi` and `/api/datapin` all write
/// their response and then `REBOOT.signal(())` — so [`ota::boot_ok`] belongs
/// here: reaching this point means an HTTP handler asked for the reboot, i.e.
/// this image booted, brought up WiFi and served a request. Without it
/// [`ota::preboot_guard`]'s counter only cleared at the 60 s mark, and two
/// quick config reboots inside that minute looked exactly like a crash loop:
/// on 2026-09-26 Jeremy changed the pixel clock twice in under a minute and
/// the third boot ROLLED THE DEVICE BACK to the previous firmware, silently.
/// It cannot whitewash a genuinely bad image — an image that cannot serve
/// never gets here — and it is idempotent (the 60 s call may have run
/// already; both just write 0 and re-assert `OtaImageState::Valid`).
#[embassy_executor::task]
async fn reboot_task() -> ! {
    REBOOT.wait().await;
    println!("rebooting into new firmware…");
    // the response first: the flash write below takes the cross-core fence,
    // and the handler is already waiting on nothing but the wire
    Timer::after(Duration::from_millis(400)).await;
    ota::boot_ok(); // this reboot was ASKED for — don't count it against the guard
    esp_hal::system::software_reset()
}

/// esp-radio's `WifiError` by name, WITHOUT `{:?}`.
///
/// `WifiError::Disconnected` carries a `DisconnectedInfo`, so formatting the
/// error with `{:?}` links `Ssid`'s `Debug`, which formats a `&str`, which
/// links `char::escape_debug_ext` and the `DebugStruct`/`DebugTuple` builders
/// — 2.6 KB of the C6's OTA slot for three log lines (Gitea #438). A
/// `&'static str` table is a few dozen bytes and says the same thing.
/// `#[non_exhaustive]`, hence the catch-all.
fn wifi_error_name(e: &esp_radio::wifi::WifiError) -> &'static str {
    use esp_radio::wifi::WifiError as E;
    match e {
        E::Disconnected(_) => "disconnected",
        E::Unsupported => "unsupported",
        E::InvalidArguments => "invalid arguments",
        E::Failed => "failed",
        E::OutOfMemory => "out of memory",
        E::InvalidSsid => "invalid ssid",
        E::InvalidPassword => "invalid password",
        E::NotConnected => "not connected",
        _ => "unknown",
    }
}

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_info) => {
                // Never `{:?}` on the whole struct — see `wifi_error_name`.
                // The SSID is ours and already in the boot log.
                println!("wifi connected");
                match controller.wait_for_disconnect_async().await {
                    // `DisconnectReason` is a fieldless enum: its `Debug` is
                    // one switch table and drags nothing else in, and the
                    // reason is the whole point of the line.
                    Ok(d) => println!("wifi disconnected: {:?}", d.reason),
                    Err(e) => println!("wifi disconnected: {}", wifi_error_name(&e)),
                }
            }
            Err(e) => {
                println!("wifi connect failed: {}", wifi_error_name(&e));
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

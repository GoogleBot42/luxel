//! Luxel firmware, milestone 2: engine + LED output + WiFi live-coding.
//!
//! ESP32-C3 + SK9822/APA102 strip over SPI (data GPIO7, clock GPIO6 by
//! default — see board configuration below). Runs the Luxel engine and
//! pushes frames as fast as they render; when WiFi credentials are baked in
//! it joins the network, serves a live-code page on port 80, and swaps the
//! running pattern on upload without dropping frames.
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
use esp_hal::spi::master::{Config as SpiConfig, Spi};
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
mod config;
mod devicemap;
mod leds;
mod mqtt;
mod netin;
mod ota;
mod patterns;
mod playlist;
mod sensors;
mod server;
mod shared;

use leds::Protocol;
use luxel_core::jsonview;
use shared::{
    publish, set_pattern_src, set_pixels, set_vmerr, Msg, BRIGHTNESS, CONTROLS_JSON, FPS,
    MAX_PIXELS, MSG_QUEUE, PIXEL_COUNT, PROTOCOL, READOUTS_JSON, VARS_JSON,
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
/// Board default LED protocol; the live value lives in `shared::PROTOCOL`
/// (seeded at boot, overridable at runtime via `/api/protocol`).
const DEFAULT_PROTOCOL: Protocol = Protocol::Sk9822;
/// Board default pixel count; the live value lives in `shared::PIXEL_COUNT`
/// (seeded from this at boot, overridable at runtime via `/api/config`).
const DEFAULT_PIXEL_COUNT: u32 = 300;
/// Global brightness 0–31 (APA102 5-bit current limiter; ignored for
/// WS2812). Keep modest on USB power.
const APA_BRIGHTNESS: u8 = 4;

/// Baked-in WiFi credentials (station mode) until NVS + provisioning land
/// in M3. Set at build time; absent → offline render-only mode.
const SSID: Option<&str> = option_env!("LUXEL_SSID");
const PASSWORD: Option<&str> = option_env!("LUXEL_PASS");

const PATTERN: &str = include_str!("../../examples/rainbow.js");

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
    // buffers must be heap Vecs. 88 KB here (96 KB cost stack erosion as
    // features grew) leaves ~31 KB of stack with ~155 KB of statics;
    // /api/status heap_free shows the live heap margin.
    #[cfg(feature = "esp32")]
    esp_alloc::heap_allocator!(size: 88 * 1024);
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

    // Bisect knob: LUXEL_NO_OTA=1 at build time skips OTA init entirely —
    // no esp-storage FlashStorage construction, no boot-time partition
    // table read — to test whether flash-driver setup interacts with the
    // esp32 radio crashes (serving worked before the OTA commit).
    if option_env!("LUXEL_NO_OTA").is_none() {
        ota::init(esp_storage::FlashStorage::new(p.FLASH));
    // Boot-loop guard BEFORE the risky part of boot (WiFi init is where a
    // bad image dies): 3 consecutive boots that never reach ota::boot_ok →
    // roll back to the other OTA slot.
    ota::boot_guard();
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
    let (ssid, password) = match (&flash_creds, baked) {
        (Some((s, p)), _) => {
            println!("wifi: joining \"{}\" (flash-stored creds)", s);
            (s.as_str(), p.as_str())
        }
        (None, Some((s, p))) => {
            println!("wifi: joining \"{}\" (compile-time creds)", s);
            (s, p)
        }
        (None, None) => {
            println!("no wifi credentials (no flash record, LUXEL_SSID/LUXEL_PASS unset); offline mode");
            // offline is a stable state, not a failed boot — clear the guard
            Timer::after(Duration::from_secs(30)).await;
            ota::boot_ok();
            loop {
                Timer::after(Duration::from_secs(3600)).await;
            }
        }
    };

    let station = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_password(password.into()),
    );
    let wifi_interface = Interface::station();
    let controller = WifiController::new(
        p.WIFI,
        ControllerConfig::default().with_initial_config(station),
    )
    .expect("wifi controller");

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    // DHCP hostname (option 12): "luxel-" + the low MAC bytes, so the
    // device shows up recognizably (and uniquely) in router lease tables.
    let mac_addr = esp_hal::efuse::base_mac_address();
    let mac = mac_addr.as_bytes();
    let mut hostname = heapless::String::<32>::new();
    let _ = core::fmt::Write::write_fmt(
        &mut hostname,
        format_args!("luxel-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5]),
    );
    println!("hostname: {}", hostname);
    let mut dhcp = embassy_net::DhcpConfig::default();
    dhcp.hostname = Some(hostname);

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        embassy_net::Config::dhcpv4(dhcp),
        mk_static!(
            // +2 spare, +2 DDP/E1.31 UDP, +1 MQTT TCP, +1 its DNS queries
            StackResources<{ server::WEB_TASK_POOL_SIZE + 6 }>,
            StackResources::new()
        ),
        seed,
    );

    spawner.spawn(connection_task(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;
    if let Some(cfg) = stack.config_v4() {
        println!("ip: http://{}/", cfg.address.address());
    }
    println!("heap free: {}", esp_alloc::HEAP.free());

    for task_id in 0..server::WEB_TASK_POOL_SIZE {
        spawner.spawn(server::web_task(task_id, stack).unwrap());
    }
    spawner.spawn(netin::ddp_task(stack).unwrap());
    spawner.spawn(netin::e131_task(stack).unwrap());
    spawner.spawn(mqtt::mqtt_task(stack).unwrap());

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

/// SPI config for a protocol (only the clock rate differs; mode 0 for both).
fn spi_cfg(p: Protocol) -> SpiConfig {
    SpiConfig::default()
        .with_frequency(Rate::from_hz(p.spi_hz()))
        .with_mode(Mode::_0)
}

/// frames. Yields to the network tasks after every frame.
#[embassy_executor::task]
async fn render_task(mut spi: Spi<'static, Blocking>) -> ! {
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
    let mut engine = match Engine::new(PATTERN, PIXEL_COUNT.load(Ordering::Relaxed), 1) {
        Ok(e) => Some(e),
        Err(d) => {
            println!("embedded pattern compile error: {}", d.message);
            None
        }
    };
    // the source currently running — kept so a live pixel-count change can
    // recompile it at the new count without a round-trip through the client
    let mut current_src = alloc::string::String::from(PATTERN);
    set_pattern_src(PATTERN);
    if let Some(eng) = engine.as_ref() {
        publish(&CONTROLS_JSON, jsonview::controls_json(eng));
    }

    // Apply the seeded protocol — flash may specify one different from the
    // boot-time default the SPI was constructed with.
    if let Err(e) = spi.apply_config(&spi_cfg(cur_protocol())) {
        println!("spi config error: {:?}", e);
    }
    let mut buf =
        alloc::vec![0u8; cur_protocol().buf_len(PIXEL_COUNT.load(Ordering::Relaxed) as usize)];
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

    loop {
        while let Ok(msg) = MSG_QUEUE.try_receive() {
            match msg {
                Msg::Code(src) => {
                    // Compile-checked by the upload handler; failure here
                    // would mean non-determinism, so log and keep the old
                    // pattern.
                    match Engine::new(&src, PIXEL_COUNT.load(Ordering::Relaxed), 1) {
                        Ok(e) => {
                            publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                            engine = Some(e);
                            set_pattern_src(&src);
                            current_src = src;
                            set_vmerr(None);
                            last = Instant::now();
                            devicemap::mark_dirty(); // re-apply the installed map
                        }
                        Err(d) => println!("recompile error (bug?): {}", d.message),
                    }
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
                    buf = alloc::vec![0u8; cur_protocol().buf_len(count as usize)];
                    match Engine::new(&current_src, count, 1) {
                        Ok(e) => {
                            publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                            engine = Some(e);
                            set_vmerr(None);
                            last = Instant::now();
                            devicemap::mark_dirty(); // re-apply the installed map
                            println!("pixel count → {}", count);
                        }
                        Err(d) => println!("resize recompile error (bug?): {}", d.message),
                    }
                }
                // Live LED-protocol change (no reboot): reconfigure the SPI
                // clock and resize the buffer to the new encoding. Sole writer
                // of PROTOCOL.
                Msg::Protocol(code) => {
                    let p = Protocol::from_u8(code);
                    PROTOCOL.store(p.as_u8(), Ordering::Relaxed);
                    if let Err(e) = spi.apply_config(&spi_cfg(p)) {
                        println!("spi config error: {:?}", e);
                    }
                    buf = alloc::vec![0u8; p.buf_len(PIXEL_COUNT.load(Ordering::Relaxed) as usize)];
                    last = Instant::now();
                    println!("protocol → {}", p.name());
                }
                // Crossfade to a new pattern (playlist transition): keep the
                // outgoing engine and blend over `ms`.
                Msg::Crossfade(src, ms) => {
                    match Engine::new(&src, PIXEL_COUNT.load(Ordering::Relaxed), 1) {
                        Ok(e) => {
                            publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                            if ms > 0 && engine.is_some() {
                                prev = engine.take();
                                blend_start = Instant::now();
                                blend_ms = ms;
                            }
                            engine = Some(e);
                            set_pattern_src(&src);
                            current_src = src;
                            set_vmerr(None);
                            last = Instant::now();
                            devicemap::mark_dirty();
                        }
                        Err(d) => println!("crossfade compile error (bug?): {}", d.message),
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
            } else if let Ok(e) = Engine::new(&current_src, PIXEL_COUNT.load(Ordering::Relaxed), 1) {
                // cleared → rebuild without a map (do not re-mark dirty)
                engine = Some(e);
            }
        }

        // sensor data (sensor board / POST /api/sensors) lands between frames
        if let Some(sf) = shared::take_sensor_frame(&mut sensor_seen) {
            if let Some(eng) = engine.as_mut() {
                eng.set_sensors(&sf);
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
            cur_protocol().encode(&blend_buf, out_brightness(), &mut buf);
            if let Err(e) = spi.write(&buf) {
                println!("spi write error: {:?}", e);
            }
            last = Instant::now(); // keep the pattern clock fresh for resume
        } else if engine.is_some() {
            let now = Instant::now();
            let delta_us = (now - last).as_micros();
            last = now;
            // µs → 16.16 ms
            let delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

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
            cur_protocol().encode(out, out_brightness(), &mut buf);
            if let Err(e) = spi.write(&buf) {
                println!("spi write error: {:?}", e);
            }
            if let Some(e) = engine.as_mut().unwrap().take_error() {
                println!("vmerr: line {}:{}: {}", e.line, e.col, e.message);
                set_vmerr(Some(alloc::format!(
                    "line {}:{}: {}",
                    e.line,
                    e.col,
                    e.message
                )));
            }
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
            }
        }

        // Pace to ~120 fps: an uncapped render loop starves the network
        // tasks (choppy preview, timed-out polls) for frame rate nobody can
        // see. Slow patterns just yield.
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

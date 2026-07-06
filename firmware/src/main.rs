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
mod leds;
mod ota;
mod server;
mod shared;

use leds::Protocol;
use luxel_core::jsonview;
use shared::{
    publish, set_pattern_src, set_pixels, set_vmerr, Msg, CONTROLS_JSON, FPS, MSG_QUEUE,
    READOUTS_JSON, VARS_JSON,
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
const PROTOCOL: Protocol = Protocol::Sk9822;
pub const PIXEL_COUNT: u32 = 300;
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
    #[cfg(feature = "esp32")]
    esp_alloc::heap_allocator!(size: 120 * 1024);
    #[cfg(not(feature = "esp32"))]
    esp_alloc::heap_allocator!(size: 160 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_int = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!(
        "luxel-fw: boot ({} px, {} Hz SPI)",
        PIXEL_COUNT,
        PROTOCOL.spi_hz()
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
            .with_frequency(Rate::from_hz(PROTOCOL.spi_hz()))
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
    assets::init();
    } else {
        println!("LUXEL_NO_OTA: ota disabled");
    }
    spawner.spawn(reboot_task().unwrap());
    // Bisect knob: LUXEL_QUIET=1 at build time skips the render task
    // entirely (no SPI, no engine, no snapshot publishing) to isolate
    // whether it interacts with the esp32 radio crashes.
    if option_env!("LUXEL_QUIET").is_none() {
        spawner.spawn(render_task(spi).unwrap());
    } else {
        println!("LUXEL_QUIET: render task disabled");
    }

    // Treat empty strings like unset — `LUXEL_SSID='' …` shouldn't try to
    // join a network named "".
    let creds = match (SSID, PASSWORD) {
        (Some(s), Some(p)) if !s.is_empty() => Some((s, p)),
        _ => None,
    };
    let Some((ssid, password)) = creds else {
        println!("no wifi credentials (LUXEL_SSID/LUXEL_PASS unset); offline mode");
        loop {
            Timer::after(Duration::from_secs(3600)).await;
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
            StackResources<{ server::WEB_TASK_POOL_SIZE + 2 }>,
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

    loop {
        Timer::after(Duration::from_secs(60)).await;
        println!(
            "fps: {}  heap free: {}",
            FPS.load(Ordering::Relaxed),
            esp_alloc::HEAP.free()
        );
    }
}

/// Renders frames and drives the strip; picks up uploaded patterns between
/// frames. Yields to the network tasks after every frame.
#[embassy_executor::task]
async fn render_task(mut spi: Spi<'static, Blocking>) -> ! {
    let mut engine = match Engine::new(PATTERN, PIXEL_COUNT, 1) {
        Ok(e) => Some(e),
        Err(d) => {
            println!("embedded pattern compile error: {}", d.message);
            None
        }
    };
    set_pattern_src(PATTERN);
    if let Some(eng) = engine.as_ref() {
        publish(&CONTROLS_JSON, jsonview::controls_json(eng));
    }

    let mut buf = alloc::vec![0u8; PROTOCOL.buf_len(PIXEL_COUNT as usize)];
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();

    loop {
        while let Ok(msg) = MSG_QUEUE.try_receive() {
            match msg {
                Msg::Code(src) => {
                    // Compile-checked by the upload handler; failure here
                    // would mean non-determinism, so log and keep the old
                    // pattern.
                    match Engine::new(&src, PIXEL_COUNT, 1) {
                        Ok(e) => {
                            publish(&CONTROLS_JSON, jsonview::controls_json(&e));
                            engine = Some(e);
                            set_pattern_src(&src);
                            set_vmerr(None);
                            last = Instant::now();
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
            }
        }

        if let Some(eng) = engine.as_mut() {
            let now = Instant::now();
            let delta_us = (now - last).as_micros();
            last = now;
            // µs → 16.16 ms
            let delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

            let px = eng.frame(delta);
            set_pixels(px);
            PROTOCOL.encode(px, APA_BRIGHTNESS, &mut buf);
            if let Err(e) = spi.write(&buf) {
                println!("spi write error: {:?}", e);
            }
            if let Some(e) = eng.take_error() {
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

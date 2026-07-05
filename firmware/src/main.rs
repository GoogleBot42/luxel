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

mod leds;
mod server;
mod shared;

use leds::Protocol;
use shared::{set_pixels, set_vmerr, CODE_QUEUE, FPS};

esp_bootloader_esp_idf::esp_app_desc!();

// ---- board configuration ----
// esp32c3 (default feature): bare C3 devkit — CLOCK → GPIO6, DATA → GPIO7,
//   no relay. For the Athom LS4P (C3 gen, WS281x on GPIO21, no clock): set
//   PROTOCOL = Ws2812 and route MOSI to GPIO21.
// esp32 feature: Athom WLED ESP32 music-reactive controller — channel 1 is
//   DATA1 = GPIO18, CLK1 = GPIO5; channel 2 (DATA2 = GPIO17, CLK2 = GPIO16)
//   is not driven yet. A 16A relay on GPIO2 switches the strip VCC
//   terminals and MUST be driven high or the strip stays dark. Button
//   GPIO0, IR GPIO25, PDM mic I2S SD = GPIO32 / WS = GPIO15 (unused).
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
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
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
    #[cfg(feature = "esp32")]
    let _relay = esp_hal::gpio::Output::new(
        p.GPIO2,
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
    #[cfg(feature = "esp32c3")]
    let spi = spi.with_sck(p.GPIO6).with_mosi(p.GPIO7);
    #[cfg(feature = "esp32")]
    let spi = spi.with_sck(p.GPIO5).with_mosi(p.GPIO18);

    spawner.spawn(render_task(spi).unwrap());

    let (Some(ssid), Some(password)) = (SSID, PASSWORD) else {
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
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        embassy_net::Config::dhcpv4(Default::default()),
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

    for task_id in 0..server::WEB_TASK_POOL_SIZE {
        spawner.spawn(server::web_task(task_id, stack).unwrap());
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
        println!("fps: {}", FPS.load(Ordering::Relaxed));
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

    let mut buf = alloc::vec![0u8; PROTOCOL.buf_len(PIXEL_COUNT as usize)];
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();

    loop {
        if let Ok(src) = CODE_QUEUE.try_receive() {
            // Compile-checked by the upload handler; failure here would
            // mean non-determinism, so just log and keep the old pattern.
            match Engine::new(&src, PIXEL_COUNT, 1) {
                Ok(e) => {
                    engine = Some(e);
                    set_vmerr(None);
                    last = Instant::now();
                }
                Err(d) => println!("recompile error (bug?): {}", d.message),
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

        embassy_futures::yield_now().await;
    }
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

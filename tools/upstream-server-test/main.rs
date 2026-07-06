//! Minimal PURE-UPSTREAM repro for the esp32 serve-crash: the embassy_dhcp
//! example's exact WiFi/net boilerplate, with the reqwless client loop
//! replaced by a bare embassy-net TCP server that answers any request with
//! an 8 KiB HTTP response (the multi-frame TX burst that reliably crashes
//! the radio blob under Luxel). No picoserve, no Luxel code.
//!
//! Test: flash, then from another machine loop:
//!   while curl -s -o /dev/null http://<ip>/; do sleep 0.2; done

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, IpListenEndpoint, Runner, StackResources};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write as _;
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    ram,
    rng::Rng,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_radio::wifi::{
    scan::ScanConfig, Config, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(PASSWORD.into()),
    );

    println!("Starting wifi");
    let wifi_interface = Interface::station();
    let mut controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();
    println!("Wifi configured and started!");

    let config = embassy_net::Config::dhcpv4(Default::default());

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    println!("Scan");
    let scan_config = ScanConfig::default().with_max(10);
    let result = controller.scan_async(&scan_config).await.unwrap();
    for ap in result {
        println!("{:?}", ap);
    }

    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;
    if let Some(config) = stack.config_v4() {
        println!("Got IP: {}", config.address);
    }

    // ---- bare TCP server: 8 KiB response per connection ----
    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];
    let mut served: u32 = 0;
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if let Err(e) = socket
            .accept(IpListenEndpoint { addr: None, port: 80 })
            .await
        {
            println!("accept error: {:?}", e);
            continue;
        }
        let mut req = [0u8; 1024];
        let _ = socket.read(&mut req).await;

        let header =
            b"HTTP/1.1 200 OK\r\nContent-Length: 8192\r\nConnection: close\r\n\r\n";
        let body = [b'x'; 512];
        let mut ok = socket.write_all(header).await.is_ok();
        if ok {
            for _ in 0..16 {
                if socket.write_all(&body).await.is_err() {
                    ok = false;
                    break;
                }
            }
        }
        let _ = socket.flush().await;
        socket.close();
        Timer::after(Duration::from_millis(20)).await;
        socket.abort();
        served += 1;
        println!("served {} ({})", served, if ok { "ok" } else { "err" });
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    loop {
        match controller.connect_async().await {
            Ok(info) => {
                println!("Wifi connected to {:?}", info);
                let info = controller.wait_for_disconnect_async().await.ok();
                println!("Disconnected: {:?}", info);
            }
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
            }
        }
        Timer::after(Duration::from_millis(5000)).await
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}

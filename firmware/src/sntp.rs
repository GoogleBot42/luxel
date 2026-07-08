//! SNTP client: one 48-byte UDP exchange with pool.ntp.org at boot (and
//! every 6 h) seeds the wall clock, so `clockHour()`-family builtins work
//! on-device. Failures back off and retry — until the first sync the
//! clock builtins return 0, exactly as before.

use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Stack};
use embassy_time::{with_timeout, Duration, Timer};
use esp_println::println;

const NTP_HOST: &str = "pool.ntp.org";
/// Seconds between the NTP epoch (1900) and the unix epoch (1970).
const NTP_UNIX_DELTA: u32 = 2_208_988_800;

#[embassy_executor::task]
pub async fn sntp_task(stack: Stack<'static>) -> ! {
    static RX_META: static_cell::StaticCell<[PacketMetadata; 2]> = static_cell::StaticCell::new();
    static RX_BUF: static_cell::StaticCell<[u8; 128]> = static_cell::StaticCell::new();
    static TX_META: static_cell::StaticCell<[PacketMetadata; 2]> = static_cell::StaticCell::new();
    static TX_BUF: static_cell::StaticCell<[u8; 128]> = static_cell::StaticCell::new();
    let mut sock = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 2]),
        RX_BUF.init([0; 128]),
        TX_META.init([PacketMetadata::EMPTY; 2]),
        TX_BUF.init([0; 128]),
    );
    sock.bind(0).expect("bind sntp");

    let mut backoff = 5u64;
    loop {
        match sync_once(stack, &mut sock).await {
            Some(unix) => {
                crate::shared::set_wall_clock(unix);
                println!("sntp: wall clock synced (unix {})", unix);
                backoff = 5;
                Timer::after(Duration::from_secs(6 * 3600)).await;
            }
            None => {
                Timer::after(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(900);
            }
        }
    }
}

async fn sync_once(stack: Stack<'static>, sock: &mut UdpSocket<'_>) -> Option<i64> {
    let addrs = stack.dns_query(NTP_HOST, DnsQueryType::A).await.ok()?;
    let addr = *addrs.first()?;
    let mut pkt = [0u8; 48];
    pkt[0] = 0x1B; // LI 0, version 3, mode 3 (client)
    sock.send_to(&pkt, IpEndpoint::new(addr, 123)).await.ok()?;
    let mut resp = [0u8; 64];
    let (n, _) = with_timeout(Duration::from_secs(4), sock.recv_from(&mut resp))
        .await
        .ok()?
        .ok()?;
    if n < 44 {
        return None;
    }
    // transmit timestamp, seconds field (big-endian, epoch 1900)
    let secs = u32::from_be_bytes(resp[40..44].try_into().ok()?);
    if secs == 0 {
        return None;
    }
    Some(secs.wrapping_sub(NTP_UNIX_DELTA) as i64)
}

//! Network pixel input: DDP (:4048) and E1.31/sACN (:5568) UDP listeners.
//! Receive-only — xLights / LedFx / Resolume paint the strip directly; while
//! packets flow the render task shows LIVE_PIXELS instead of the engine and
//! the running pattern resumes LIVE_TIMEOUT_MS after the stream stops.
//! Packet parsing is shared with the native mirror via luxel_core::netin.

use core::sync::atomic::Ordering;

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::Stack;
use embassy_time::Instant;
use luxel_core::netin::{parse_ddp, parse_e131, DDP_PORT, E131_CHANNELS, E131_PORT};

use crate::shared::{LIVE_MARK_MS, LIVE_PIXELS, LIVE_PROTO, PIXEL_COUNT};

/// Largest packet either protocol sends (DDP data ≤ 1440 + header; E1.31 is
/// 638). One recv buffer of this size per socket.
const PKT: usize = 1536;

/// Write `data` at byte `offset` of the live frame (grows as needed, bounded
/// by the strip) and stamp it fresh.
fn live_write(offset: usize, data: &[u8], proto: u8) {
    let max = PIXEL_COUNT.load(Ordering::Relaxed) as usize * 3;
    if offset >= max {
        return;
    }
    let n = data.len().min(max - offset);
    LIVE_PIXELS.lock(|c| {
        let mut buf = c.borrow_mut();
        if buf.len() < offset + n {
            buf.resize(offset + n, 0);
        }
        buf[offset..offset + n].copy_from_slice(&data[..n]);
    });
    // now() ms of 0 means "never" — skip that one tick in the epoch
    let now = (Instant::now().as_millis() as u32).max(1);
    LIVE_MARK_MS.store(now, Ordering::Relaxed);
    LIVE_PROTO.store(proto, Ordering::Relaxed);
}

/// Socket buffers: static, one set per call site (each task has its own
/// macro expansion — a shared fn would double-init its StaticCells).
macro_rules! bufs {
    () => {{
        use static_cell::StaticCell;
        static RX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
        static RX_BUF: StaticCell<[u8; PKT * 2]> = StaticCell::new();
        static TX_META: StaticCell<[PacketMetadata; 1]> = StaticCell::new();
        static TX_BUF: StaticCell<[u8; 16]> = StaticCell::new();
        (
            RX_META.init([PacketMetadata::EMPTY; 4]).as_mut_slice(),
            RX_BUF.init([0; PKT * 2]).as_mut_slice(),
            TX_META.init([PacketMetadata::EMPTY; 1]).as_mut_slice(),
            TX_BUF.init([0; 16]).as_mut_slice(),
        )
    }};
}

#[embassy_executor::task]
pub async fn ddp_task(stack: Stack<'static>) -> ! {
    let (rx_meta, rx_buf, tx_meta, tx_buf) = bufs!();
    let mut sock = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    sock.bind(DDP_PORT).expect("bind ddp");
    let mut pkt = [0u8; PKT];
    loop {
        let Ok((len, _peer)) = sock.recv_from(&mut pkt).await else {
            continue;
        };
        if let Some(d) = parse_ddp(&pkt[..len]) {
            live_write(d.offset, d.data, 1);
        }
    }
}

#[embassy_executor::task]
pub async fn e131_task(stack: Stack<'static>) -> ! {
    // sACN defaults to multicast 239.255.<universe-hi>.<universe-lo>; join
    // enough universes for the current strip. Unicast always works too.
    let px = PIXEL_COUNT.load(Ordering::Relaxed) as usize;
    let universes = (px * 3).div_ceil(E131_CHANNELS).max(1);
    for u in 1..=universes as u16 {
        let [hi, lo] = u.to_be_bytes();
        let _ = stack
            .join_multicast_group(embassy_net::Ipv4Address::new(239, 255, hi, lo))
            .inspect_err(|e| esp_println::println!("e131: multicast join {}: {:?}", u, e));
    }
    let (rx_meta, rx_buf, tx_meta, tx_buf) = bufs!();
    let mut sock = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    sock.bind(E131_PORT).expect("bind e131");
    let mut pkt = [0u8; PKT];
    loop {
        let Ok((len, _peer)) = sock.recv_from(&mut pkt).await else {
            continue;
        };
        if let Some(d) = parse_e131(&pkt[..len]) {
            let off = (d.universe.max(1) as usize - 1) * E131_CHANNELS;
            live_write(off, d.data, 2);
        }
    }
}

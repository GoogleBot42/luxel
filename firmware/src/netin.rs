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

use crate::shared::{self, LIVE_MARK_MS, LIVE_PIXELS, LIVE_PROTO, PIXEL_COUNT};

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
    // heap, not a future-local: task futures are statics, and statics eat
    // the DRAM that becomes the main task stack
    let mut pkt = alloc::vec![0u8; PKT];
    loop {
        let Ok((len, _peer)) = sock.recv_from(&mut pkt).await else {
            continue;
        };
        if let Some(d) = parse_ddp(&pkt[..len]) {
            live_write(d.offset, d.data, 1);
        }
    }
}

/// Luxel-to-Luxel sync (UDP :4049, LAN broadcast). Leader: beacon 4×/s
/// with the engine timebase + the latest sensor frame when it moved;
/// follower: record beacons (the render task slews toward them) and
/// inject relayed sensors. The mode atomic steers the task live.
#[embassy_executor::task]
pub async fn sync_task(stack: Stack<'static>, boot_id: u32) -> ! {
    use core::sync::atomic::Ordering;
    use embassy_futures::select::{select, Either};
    use embassy_time::{Duration, Timer};
    use luxel_core::netin::{build_sync, parse_sync, SYNC_PORT};

    let (rx_meta, rx_buf, tx_meta, tx_buf) = bufs!();
    let mut sock = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    sock.bind(SYNC_PORT).expect("bind sync");
    let mut pkt = alloc::vec![0u8; 256];
    let mut sensor_sent: u32 = 0;
    let mut last_pull: u32 = 0;
    let mut pull_cooldown = embassy_time::Instant::now();
    loop {
        match shared::SYNC_MODE.load(Ordering::Relaxed) {
            1 => {
                // leader: piggyback the sensor frame only when it moved
                let seq = shared::SENSOR_SEQ.load(Ordering::Relaxed);
                let sb = if seq != sensor_sent {
                    sensor_sent = seq;
                    shared::SENSOR_FRAME
                        .lock(|c| c.borrow().clone())
                        .map(|s| sensor_frame_to_sb(&s))
                } else {
                    None
                };
                let beacon = build_sync(
                    boot_id,
                    shared::engine_time_ms(),
                    shared::PATTERN_HASH.load(Ordering::Relaxed),
                    sb.as_deref(),
                );
                let dest = (embassy_net::Ipv4Address::BROADCAST, SYNC_PORT);
                if let Err(e) = sock.send_to(&beacon, dest).await {
                    esp_println::println!("sync: send {:?}", e);
                }
                Timer::after(Duration::from_millis(250)).await;
            }
            2 => {
                // follower: wait for a beacon (bounded, so a mode change
                // out of follower is noticed within a second)
                match select(sock.recv_from(&mut pkt), Timer::after(Duration::from_secs(1))).await
                {
                    Either::First(Ok((len, meta))) => {
                        if let Some(b) = parse_sync(&pkt[..len]) {
                            shared::set_sync_leader(b.boot_id, b.time_ms);
                            if let Some(sf) = b.sensor {
                                shared::set_sensor_frame(sf);
                            }
                            // pattern distribution: leader runs something we
                            // don't → pull its source and adopt it
                            if let Some(h) = b.pattern_hash {
                                let local = shared::PATTERN_HASH.load(Ordering::Relaxed);
                                if h != local
                                    && h != last_pull
                                    && pull_cooldown.elapsed().as_secs() >= 2
                                {
                                    last_pull = h;
                                    pull_cooldown = embassy_time::Instant::now();
                                    adopt_leader_pattern(stack, meta.endpoint.addr).await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => Timer::after(Duration::from_millis(500)).await,
        }
    }
}

/// Pull the leader's running pattern (GET /api/pattern.lxp — an LXP1
/// envelope of source + bytecode; this device has no compiler) and adopt
/// it: decode-validated, playlist stopped, same swap path as POST
/// /api/code. Failures just log — the next changed beacon retries.
async fn adopt_leader_pattern(stack: Stack<'static>, leader: embassy_net::IpAddress) {
    use embassy_net::tcp::TcpSocket;

    // heap buffers (task futures are statics — the main-stack rule)
    let mut rx = alloc::vec![0u8; 2048];
    let mut tx = alloc::vec![0u8; 1024];
    let mut sock = TcpSocket::new(stack, &mut rx, &mut tx);
    sock.set_timeout(Some(embassy_time::Duration::from_secs(5)));
    if sock.connect((leader, 80)).await.is_err() {
        esp_println::println!("sync: leader {:?} not reachable on :80", leader);
        return;
    }
    let req = alloc::format!(
        "GET /api/pattern.lxp HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        leader
    );
    let mut out = req.as_bytes();
    while !out.is_empty() {
        match sock.write(out).await {
            Ok(0) | Err(_) => return,
            Ok(n) => out = &out[n..],
        }
    }
    // read to close, capped at the envelope bound (source + bytecode + headers)
    let mut body = alloc::vec::Vec::with_capacity(4096);
    let mut chunk = alloc::vec![0u8; 1024];
    while body.len() < 48 * 1024 {
        match sock.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    // split HTTP headers from the binary envelope
    let Some(at) = body.windows(4).position(|w| w == b"\r\n\r\n") else {
        return;
    };
    let payload = &body[at + 4..];
    let env = match luxel_core::bytecode::decode_envelope(payload) {
        Ok(e) => e,
        Err(e) => {
            esp_println::println!("sync: leader envelope rejected: {}", e);
            return;
        }
    };
    match luxel_core::bytecode::validate(env.bytecode) {
        Ok(()) => {
            crate::playlist::stop(); // following the leader takes over
            // forward the envelope bytes as-is (one copy, no src/bc splits —
            // producer-side copies OOM under a heavy running pattern)
            shared::MSG_QUEUE
                .send(shared::Msg::Code { env: payload.to_vec() })
                .await;
            shared::set_current_pattern_id("");
            esp_println::println!("sync: adopted the leader's pattern");
        }
        Err(e) => esp_println::println!("sync: leader bytecode rejected: {}", e),
    }
}

/// Re-encode a SensorFrame as the 98-byte SB wire format (beacon payload;
/// mirrors serve.rs sensor_frame_to_sb).
fn sensor_frame_to_sb(s: &luxel_core::engine::SensorFrame) -> alloc::vec::Vec<u8> {
    use luxel_core::fixed::Fx;
    let mut p = alloc::vec::Vec::with_capacity(luxel_core::netin::SB_FRAME_LEN);
    p.extend_from_slice(luxel_core::netin::SB_MAGIC);
    let u16le = |v: Fx| (v.raw().clamp(0, 0xFFFF) as u16).to_le_bytes();
    for v in &s.frequency_data {
        p.extend_from_slice(&u16le(*v));
    }
    p.extend_from_slice(&u16le(s.energy_average));
    p.extend_from_slice(&u16le(s.max_frequency_magnitude));
    p.extend_from_slice(&(s.max_frequency.to_int_trunc().clamp(0, 65535) as u16).to_le_bytes());
    for v in &s.accelerometer {
        p.extend_from_slice(&(v.raw().clamp(-32768, 32767) as i16).to_le_bytes());
    }
    p.extend_from_slice(&u16le(s.light));
    for v in &s.analog_inputs {
        p.extend_from_slice(&u16le(*v));
    }
    p.extend_from_slice(b"END\0");
    p
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
    let mut pkt = alloc::vec![0u8; PKT];
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

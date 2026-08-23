//! WiFi AP-mode provisioning. When the device has no way onto a network
//! (no flash creds, no baked creds) — or the user asks via POST /api/apmode
//! (a one-shot flag, so a crash in this path can never wedge the device
//! off-net) — it boots as an open access point `luxel-XXXX` at
//! 192.168.4.1, runs a DHCP server (edge-dhcp) and a catch-all DNS
//! responder, and serves the normal web app: phones pop the captive-portal
//! sheet, Settings → WiFi stores the credentials, and the device reboots
//! onto the real network.

use core::net::Ipv4Addr;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Stack};
use esp_println::println;

/// True while running as a provisioning AP — the HTTP server then answers
/// unknown paths with a redirect to the portal (captive-portal detection).
pub static AP_MODE: AtomicBool = AtomicBool::new(false);

pub const AP_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);

/// Minimal DHCP server on :67 — hands out 192.168.4.50-200 leases pointing
/// gateway + DNS at us.
#[embassy_executor::task]
pub async fn dhcp_task(stack: Stack<'static>) -> ! {
    use edge_dhcp::server::{Server, ServerOptions};
    use edge_dhcp::{Options, Packet};

    // Payload buffers are ConstStaticCell: the zeroed array is a const
    // initializer living in the static (`.bss`), so nothing is built on the
    // stack and moved in the way `StaticCell::init([0; 1600])` would.
    static RX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static RX_BUF: static_cell::ConstStaticCell<[u8; 1600]> =
        static_cell::ConstStaticCell::new([0; 1600]);
    static TX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static TX_BUF: static_cell::ConstStaticCell<[u8; 1600]> =
        static_cell::ConstStaticCell::new([0; 1600]);
    let mut sock = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 4]),
        RX_BUF.take(),
        TX_META.init([PacketMetadata::EMPTY; 4]),
        TX_BUF.take(),
    );
    sock.bind(67).expect("bind dhcp");

    let mut server: Server<_, 16> = Server::new(
        || embassy_time::Instant::now().as_secs(),
        AP_IP,
    );
    let mut gw = [AP_IP];
    let dns = [AP_IP];
    let mut opts = ServerOptions::new(AP_IP, Some(&mut gw));
    opts.dns = &dns;

    let mut pkt = alloc::vec![0u8; 1536];
    let mut out = alloc::vec![0u8; 1536];
    loop {
        let Ok((n, _peer)) = sock.recv_from(&mut pkt).await else {
            continue;
        };
        let Ok(request) = Packet::decode(&pkt[..n]) else {
            continue;
        };
        let mut opt_buf = Options::buf();
        if let Some(reply) = server.handle_request(&mut opt_buf, &opts, &request) {
            if let Ok(bytes) = reply.encode(&mut out) {
                // DHCP replies go to the broadcast address (the client has
                // no IP yet)
                let dest = IpEndpoint::new(Ipv4Addr::BROADCAST.into(), 68);
                let _ = sock.send_to(bytes, dest).await;
            }
        }
    }
}

/// Catch-all DNS on :53 — every A query resolves to us, so any URL a phone
/// tries lands on the portal.
#[embassy_executor::task]
pub async fn dns_task(stack: Stack<'static>) -> ! {
    // Same ConstStaticCell reasoning as dhcp_task — these sit exactly at the
    // lint's 1 KB threshold, so keep them off the stack by construction.
    static RX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static RX_BUF: static_cell::ConstStaticCell<[u8; 1024]> =
        static_cell::ConstStaticCell::new([0; 1024]);
    static TX_META: static_cell::StaticCell<[PacketMetadata; 4]> = static_cell::StaticCell::new();
    static TX_BUF: static_cell::ConstStaticCell<[u8; 1024]> =
        static_cell::ConstStaticCell::new([0; 1024]);
    let mut sock = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 4]),
        RX_BUF.take(),
        TX_META.init([PacketMetadata::EMPTY; 4]),
        TX_BUF.take(),
    );
    sock.bind(53).expect("bind dns");

    let mut pkt = alloc::vec![0u8; 512];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut pkt).await else {
            continue;
        };
        if let Some(reply) = answer_a_query(&pkt[..n]) {
            let _ = sock.send_to(&reply, peer).await;
        }
    }
}

/// Build an authoritative A-record answer (→ AP_IP) for any standard query.
/// Hand-rolled: the fixed-shape reply echoes the question and appends one
/// answer with a name pointer — the whole of DNS we need for a portal.
fn answer_a_query(q: &[u8]) -> Option<alloc::vec::Vec<u8>> {
    if q.len() < 12 || q[2] & 0x80 != 0 {
        return None; // too short or already a response
    }
    let qdcount = u16::from_be_bytes([q[4], q[5]]);
    if qdcount == 0 {
        return None;
    }
    // find the end of the first question (labels, then QTYPE+QCLASS)
    let mut at = 12;
    while at < q.len() && q[at] != 0 {
        at += 1 + q[at] as usize;
    }
    let qend = at + 5; // zero byte + QTYPE(2) + QCLASS(2)
    if qend > q.len() {
        return None;
    }
    let mut r = alloc::vec::Vec::with_capacity(qend + 16);
    r.extend_from_slice(&q[0..2]); // id
    r.extend_from_slice(&[0x84, 0x00]); // response, authoritative, no error
    r.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 0]); // 1 question, 1 answer
    r.extend_from_slice(&q[12..qend]); // the question, echoed
    r.extend_from_slice(&[0xc0, 0x0c]); // name = pointer to the question
    r.extend_from_slice(&[0, 1, 0, 1]); // TYPE A, CLASS IN
    r.extend_from_slice(&[0, 0, 0, 30]); // TTL 30s
    r.extend_from_slice(&[0, 4]); // RDLENGTH
    r.extend_from_slice(&AP_IP.octets());
    Some(r)
}

pub fn log_started(ssid: &str) {
    AP_MODE.store(true, Ordering::Relaxed);
    println!("provisioning AP \"{}\" up: http://{}/ (open network)", ssid, AP_IP);
}

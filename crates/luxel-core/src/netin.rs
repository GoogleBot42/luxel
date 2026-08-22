//! DDP and E1.31 (sACN) packet parsing — pure `no_std` functions shared by
//! the firmware UDP listener and the native mirror (`luxel serve`), so the
//! wire behavior is identical and testable off-device.
//!
//! Both protocols carry raw RGB bytes for a pixel range:
//!   - DDP (port 4048): header carries a byte offset — one stream, arbitrary
//!     strip length, fragmented frames use offsets. xLights/LedFx/WLED speak
//!     it natively. Receive-only: query/reply/storage packets are ignored.
//!   - E1.31 (port 5568): DMX-over-ACN; each universe carries up to 510
//!     channels (170 RGB pixels), longer strips span consecutive universes
//!     starting at universe 1.

pub const DDP_PORT: u16 = 4048;
pub const E131_PORT: u16 = 5568;

/// Usable channels per E1.31 universe (170 whole RGB pixels).
pub const E131_CHANNELS: usize = 510;

/// A parsed DDP data packet: `data` goes at byte `offset` of the RGB buffer.
pub struct DdpData<'a> {
    pub offset: usize,
    pub data: &'a [u8],
    /// PUSH flag: display the assembled frame now.
    pub push: bool,
}

/// Parse a DDP packet; `None` for anything that isn't a plain data write
/// (bad version, query/reply/storage, JSON destination, truncated payload).
pub fn parse_ddp(pkt: &[u8]) -> Option<DdpData<'_>> {
    if pkt.len() < 10 {
        return None;
    }
    let flags = pkt[0];
    if flags & 0xc0 != 0x40 {
        return None; // version must be 1
    }
    if flags & 0b0000_1110 != 0 {
        return None; // storage / reply / query — not a data write
    }
    let header = if flags & 0x10 != 0 { 14 } else { 10 }; // timecode adds 4
    if pkt.len() < header {
        return None;
    }
    let id = pkt[3];
    if !(id == 0 || id == 1 || id == 255) {
        return None; // some other output / the JSON config endpoints
    }
    let offset = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]) as usize;
    let len = u16::from_be_bytes([pkt[8], pkt[9]]) as usize;
    let data = pkt.get(header..header + len)?;
    Some(DdpData {
        offset,
        data,
        push: flags & 0x01 != 0,
    })
}

/// A parsed E1.31 data packet: up to 510 RGB bytes for one universe.
pub struct E131Data<'a> {
    /// 1-based sACN universe number.
    pub universe: u16,
    pub data: &'a [u8],
}

/// Parse an E1.31 (sACN) data packet; `None` for discovery/sync packets,
/// preview data, non-zero start codes, or anything malformed.
pub fn parse_e131(pkt: &[u8]) -> Option<E131Data<'_>> {
    // root layer: preamble size, ACN packet identifier, data vector
    if pkt.len() < 126 {
        return None;
    }
    if pkt[0..2] != [0x00, 0x10] || &pkt[4..16] != b"ASC-E1.17\0\0\0" {
        return None;
    }
    if pkt[18..22] != [0, 0, 0, 0x04] {
        return None; // VECTOR_ROOT_E131_DATA
    }
    // framing layer: data vector, options (skip preview-only data)
    if pkt[40..44] != [0, 0, 0, 0x02] {
        return None; // VECTOR_E131_DATA_PACKET
    }
    if pkt[112] & 0x80 != 0 {
        return None; // Preview_Data: visualize, don't light
    }
    let universe = u16::from_be_bytes([pkt[113], pkt[114]]);
    // DMP layer: vector 0x02, DMX start code 0 (dimmer data)
    if pkt[117] != 0x02 || pkt[125] != 0x00 {
        return None;
    }
    let count = u16::from_be_bytes([pkt[123], pkt[124]]) as usize; // incl. start code
    let len = count.checked_sub(1)?.min(E131_CHANNELS);
    let data = pkt.get(126..126 + len)?;
    Some(E131Data { universe, data })
}

// ---- PB sensor expansion board (open source, MIT) serial frames ----
// One-way 115200-baud stream: "SB1.0\0" + 32×u16 freq + u16 energyAverage +
// u16 maxFreqMagnitude + u16 maxFreqHz + 3×s16 accel + u16 light +
// 5×u16 analog + "END\0" — 98 bytes, ~40 frames/s.

pub const SB_MAGIC: &[u8; 6] = b"SB1.0\0";
pub const SB_FRAME_LEN: usize = 98;

/// Parse one sensor-board frame starting at `pkt[0]` (use [sb_find] to
/// locate the header in a byte stream first).
///
/// Scaling: u16 fields land as raw 16.16 fractions (0..1), matching PB's
/// normalized bindings; `max_frequency` is in Hz; accelerometer s16 maps to
/// ±0.5 = ±full-scale. Accel/light scaling still needs pinning against a
/// real sensor board on the PB (blocked on hardware — Gitea ticket).
pub fn parse_sensor_board(pkt: &[u8]) -> Option<crate::engine::SensorFrame> {
    use crate::fixed::Fx;
    if pkt.len() < SB_FRAME_LEN || &pkt[..6] != SB_MAGIC || &pkt[94..98] != b"END\0" {
        return None;
    }
    let u16le = |at: usize| u16::from_le_bytes([pkt[at], pkt[at + 1]]);
    let mut s = crate::engine::SensorFrame::default();
    for (i, slot) in s.frequency_data.iter_mut().enumerate() {
        *slot = Fx::from_raw(u16le(6 + i * 2) as i32);
    }
    s.energy_average = Fx::from_raw(u16le(70) as i32);
    s.max_frequency_magnitude = Fx::from_raw(u16le(72) as i32);
    s.max_frequency = Fx::from_int(u16le(74) as i32);
    for (i, slot) in s.accelerometer.iter_mut().enumerate() {
        *slot = Fx::from_raw(u16le(76 + i * 2) as i16 as i32);
    }
    s.light = Fx::from_raw(u16le(82) as i32);
    for (i, slot) in s.analog_inputs.iter_mut().enumerate() {
        *slot = Fx::from_raw(u16le(84 + i * 2) as i32);
    }
    Some(s)
}

/// Index of the next sensor-board header in `buf`, if any.
pub fn sb_find(buf: &[u8]) -> Option<usize> {
    buf.windows(SB_MAGIC.len()).position(|w| w == SB_MAGIC)
}

// ---- Luxel-to-Luxel sync beacons ----
// A leader broadcasts its engine timebase (and, when fresh, its latest
// sensor frame) on UDP :4049 a few times a second; followers slew their
// clock to it so identical patterns stay phase-locked across controllers.
//   v2: "LXS2" + u32-LE boot_id + u64-LE time_ms + u32-LE pattern_hash
//       + u8 flags [+ 98-byte SB frame]
//   v1: "LXS1" — same minus pattern_hash (still parsed; hash = None).
// boot_id is random per boot — followers use it to notice leader restarts
// (a fresh leader clock needs a hard jump, not a slew). pattern_hash is
// FNV-1a over the leader's running source: when it changes, followers pull
// http://<leader>/api/pattern and adopt it (pattern distribution).

pub const SYNC_PORT: u16 = 4049;
pub const SYNC_MAGIC_V1: &[u8; 4] = b"LXS1";
pub const SYNC_MAGIC: &[u8; 4] = b"LXS2";
const SYNC_FLAG_SENSOR: u8 = 0x01;

pub struct SyncBeacon {
    pub boot_id: u32,
    pub time_ms: u64,
    /// FNV-1a of the leader's running pattern source (None from v1 beacons).
    pub pattern_hash: Option<u32>,
    pub sensor: Option<crate::engine::SensorFrame>,
}

/// FNV-1a — the pattern-identity hash carried in beacons.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}

pub fn build_sync(
    boot_id: u32,
    time_ms: u64,
    pattern_hash: u32,
    sensor_frame: Option<&[u8]>,
) -> alloc::vec::Vec<u8> {
    let mut p = alloc::vec::Vec::with_capacity(21 + SB_FRAME_LEN);
    p.extend_from_slice(SYNC_MAGIC);
    p.extend_from_slice(&boot_id.to_le_bytes());
    p.extend_from_slice(&time_ms.to_le_bytes());
    p.extend_from_slice(&pattern_hash.to_le_bytes());
    match sensor_frame {
        Some(sb) if sb.len() == SB_FRAME_LEN => {
            p.push(SYNC_FLAG_SENSOR);
            p.extend_from_slice(sb);
        }
        _ => p.push(0),
    }
    p
}

pub fn parse_sync(pkt: &[u8]) -> Option<SyncBeacon> {
    let (hash, flags_at) = match pkt.get(..4)? {
        m if m == SYNC_MAGIC => (
            Some(u32::from_le_bytes(pkt.get(16..20)?.try_into().ok()?)),
            20,
        ),
        m if m == SYNC_MAGIC_V1 => (None, 16),
        _ => return None,
    };
    let boot_id = u32::from_le_bytes(pkt.get(4..8)?.try_into().ok()?);
    let time_ms = u64::from_le_bytes(pkt.get(8..16)?.try_into().ok()?);
    let flags = *pkt.get(flags_at)?;
    let sensor = if flags & SYNC_FLAG_SENSOR != 0 {
        parse_sensor_board(pkt.get(flags_at + 1..flags_at + 1 + SB_FRAME_LEN)?)
    } else {
        None
    };
    Some(SyncBeacon {
        boot_id,
        time_ms,
        pattern_hash: hash,
        sensor,
    })
}

// ---- External event injection ----
// POST /api/events body (and a future UDP surface): "EV1\0" + u8 count +
// count × 4×i32-LE raw 16.16 [type, x, y, value]. Length must match
// exactly; count is capped at the engine queue size — send more and the
// oldest would be dropped anyway, so the cap loses nothing.

pub const EV_MAGIC: &[u8; 4] = b"EV1\0";
/// Max events per frame — mirrors `vm::MAX_EVENTS` (the engine queue size).
pub const EV_MAX_BATCH: usize = crate::vm::MAX_EVENTS;

/// Parse an event-injection frame into `[type, x, y, value]` quads.
pub fn parse_events(pkt: &[u8]) -> Option<alloc::vec::Vec<[crate::fixed::Fx; 4]>> {
    use crate::fixed::Fx;
    if pkt.len() < 5 || &pkt[..4] != EV_MAGIC {
        return None;
    }
    let count = pkt[4] as usize;
    if count > EV_MAX_BATCH || pkt.len() != 5 + count * 16 {
        return None;
    }
    let mut out = alloc::vec::Vec::new();
    out.try_reserve_exact(count).ok()?;
    for i in 0..count {
        let mut ev = [Fx::ZERO; 4];
        for (j, slot) in ev.iter_mut().enumerate() {
            let at = 5 + i * 16 + j * 4;
            *slot = Fx::from_raw(i32::from_le_bytes(pkt[at..at + 4].try_into().unwrap()));
        }
        out.push(ev);
    }
    Some(out)
}

/// Build an event-injection frame (test + mirror + beacon-relay helper;
/// the web client encodes the same layout in TS). Truncates to
/// [`EV_MAX_BATCH`].
pub fn build_events(events: &[[crate::fixed::Fx; 4]]) -> alloc::vec::Vec<u8> {
    let events = &events[..events.len().min(EV_MAX_BATCH)];
    let mut p = alloc::vec::Vec::with_capacity(5 + events.len() * 16);
    p.extend_from_slice(EV_MAGIC);
    p.push(events.len() as u8);
    for ev in events {
        for v in ev {
            p.extend_from_slice(&v.raw().to_le_bytes());
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_frame_round_trip() {
        use crate::fixed::Fx;
        let evs = [
            [Fx::from_int(1), Fx::from_f64(0.25), Fx::from_f64(0.5), Fx::ONE],
            [Fx::from_int(2), Fx::ZERO, Fx::from_f64(-0.5), Fx::from_f64(0.125)],
        ];
        let frame = build_events(&evs);
        assert_eq!(frame.len(), 5 + 2 * 16);
        assert_eq!(parse_events(&frame).unwrap(), evs);
        // rejects: bad magic, short body, count/length mismatch, oversize count
        assert!(parse_events(b"EV2\0\0").is_none());
        assert!(parse_events(&frame[..frame.len() - 1]).is_none());
        let mut wrong = frame.clone();
        wrong[4] = 1;
        assert!(parse_events(&wrong).is_none());
        let mut big = build_events(&[[Fx::ZERO; 4]; 40]);
        assert_eq!(big[4] as usize, EV_MAX_BATCH, "builder truncates");
        big[4] = 40; // forged oversize count
        assert!(parse_events(&big).is_none());
        // empty frame is valid and empty
        assert_eq!(parse_events(&build_events(&[])).unwrap().len(), 0);
    }

    fn ddp_pkt(flags: u8, id: u8, offset: u32, data: &[u8]) -> alloc::vec::Vec<u8> {
        let mut p = alloc::vec![flags, 0, 1, id];
        p.extend_from_slice(&offset.to_be_bytes());
        p.extend_from_slice(&(data.len() as u16).to_be_bytes());
        p.extend_from_slice(data);
        p
    }

    #[test]
    fn ddp_basic_push() {
        let p = ddp_pkt(0x41, 1, 6, &[10, 20, 30]);
        let d = parse_ddp(&p).unwrap();
        assert_eq!(d.offset, 6);
        assert_eq!(d.data, &[10, 20, 30]);
        assert!(d.push);
    }

    #[test]
    fn ddp_rejects_query_reply_storage_and_bad_version() {
        assert!(parse_ddp(&ddp_pkt(0x42, 1, 0, &[1])).is_none()); // query
        assert!(parse_ddp(&ddp_pkt(0x44, 1, 0, &[1])).is_none()); // reply
        assert!(parse_ddp(&ddp_pkt(0x48, 1, 0, &[1])).is_none()); // storage
        assert!(parse_ddp(&ddp_pkt(0x81, 1, 0, &[1])).is_none()); // version 2
        assert!(parse_ddp(&ddp_pkt(0x41, 251, 0, &[1])).is_none()); // JSON id
    }

    #[test]
    fn ddp_timecode_header_and_truncation() {
        // timecode flag: 4 extra header bytes before the payload
        let mut p = alloc::vec![0x51u8, 0, 1, 1];
        p.extend_from_slice(&0u32.to_be_bytes());
        p.extend_from_slice(&3u16.to_be_bytes());
        p.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]); // timecode
        p.extend_from_slice(&[7, 8, 9]);
        assert_eq!(parse_ddp(&p).unwrap().data, &[7, 8, 9]);
        // declared length longer than the packet → reject
        let short = ddp_pkt(0x41, 1, 0, &[1, 2, 3]);
        assert!(parse_ddp(&short[..short.len() - 1]).is_none());
    }

    fn e131_pkt(universe: u16, data: &[u8]) -> alloc::vec::Vec<u8> {
        let mut p = alloc::vec![0u8; 126];
        p[0..2].copy_from_slice(&[0x00, 0x10]);
        p[4..16].copy_from_slice(b"ASC-E1.17\0\0\0");
        p[21] = 0x04; // root vector
        p[43] = 0x02; // framing vector
        p[113..115].copy_from_slice(&universe.to_be_bytes());
        p[117] = 0x02; // DMP vector
        p[118] = 0xa1;
        let count = (data.len() + 1) as u16;
        p[123..125].copy_from_slice(&count.to_be_bytes());
        p[125] = 0x00; // start code
        p.extend_from_slice(data);
        p
    }

    #[test]
    fn e131_basic() {
        let p = e131_pkt(2, &[1, 2, 3, 4, 5, 6]);
        let d = parse_e131(&p).unwrap();
        assert_eq!(d.universe, 2);
        assert_eq!(d.data, &[1, 2, 3, 4, 5, 6]);
    }

    pub fn sb_frame(freq0: u16, energy: u16, max_hz: u16) -> alloc::vec::Vec<u8> {
        let mut p = alloc::vec::Vec::new();
        p.extend_from_slice(SB_MAGIC);
        p.extend_from_slice(&freq0.to_le_bytes());
        p.extend_from_slice(&[0u8; 62]); // bins 1..32
        p.extend_from_slice(&energy.to_le_bytes());
        p.extend_from_slice(&0x8000u16.to_le_bytes()); // maxFreqMagnitude 0.5
        p.extend_from_slice(&max_hz.to_le_bytes());
        p.extend_from_slice(&(-16384i16).to_le_bytes()); // accel x
        p.extend_from_slice(&[0u8; 4]); // accel y, z
        p.extend_from_slice(&0xFFFFu16.to_le_bytes()); // light ≈ 1
        p.extend_from_slice(&[0u8; 10]); // analog
        p.extend_from_slice(b"END\0");
        p
    }

    #[test]
    fn sensor_board_frame() {
        let p = sb_frame(0x4000, 0x2000, 440);
        assert_eq!(p.len(), SB_FRAME_LEN);
        let s = parse_sensor_board(&p).unwrap();
        assert_eq!(s.frequency_data[0], crate::fixed::Fx::from_f64(0.25));
        assert_eq!(s.energy_average, crate::fixed::Fx::from_f64(0.125));
        assert_eq!(s.max_frequency, crate::fixed::Fx::from_int(440));
        assert_eq!(s.accelerometer[0], crate::fixed::Fx::from_f64(-0.25));
        // resync: header found mid-stream
        let mut stream = alloc::vec![0xAAu8; 7];
        stream.extend_from_slice(&p);
        assert_eq!(sb_find(&stream), Some(7));
        // corrupt trailer rejected
        let mut bad = p.clone();
        bad[95] = b'X';
        assert!(parse_sensor_board(&bad).is_none());
    }

    #[test]
    fn sync_beacon_round_trip() {
        // bare beacon
        let p = build_sync(0xdead_beef, 123_456_789_012, 0xabcd, None);
        let b = parse_sync(&p).unwrap();
        assert_eq!(b.boot_id, 0xdead_beef);
        assert_eq!(b.time_ms, 123_456_789_012);
        assert_eq!(b.pattern_hash, Some(0xabcd));
        assert!(b.sensor.is_none());
        // with a piggybacked sensor frame
        let sb = sb_frame(0x4000, 0x2000, 440);
        let p = build_sync(7, 1000, 1, Some(&sb));
        let b = parse_sync(&p).unwrap();
        let s = b.sensor.expect("sensor present");
        assert_eq!(s.max_frequency, crate::fixed::Fx::from_int(440));
        // truncated sensor payload rejected as a whole
        assert!(parse_sync(&p[..p.len() - 1]).is_none());
        assert!(parse_sync(b"LXS0aaaaaaaaaaaaaaaaaaaa").is_none());
        // a v1 beacon still parses (no hash)
        let mut v1 = alloc::vec::Vec::new();
        v1.extend_from_slice(SYNC_MAGIC_V1);
        v1.extend_from_slice(&7u32.to_le_bytes());
        v1.extend_from_slice(&1000u64.to_le_bytes());
        v1.push(0);
        let b = parse_sync(&v1).unwrap();
        assert_eq!(b.pattern_hash, None);
        assert_eq!(b.time_ms, 1000);
        // fnv sanity: known vector ("" → offset basis, stability pin)
        assert_eq!(fnv1a(b""), 0x811c_9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    }

    #[test]
    fn e131_rejects_preview_and_nonzero_start_code() {
        let mut p = e131_pkt(1, &[1, 2, 3]);
        p[112] = 0x80; // preview
        assert!(parse_e131(&p).is_none());
        let mut p = e131_pkt(1, &[1, 2, 3]);
        p[125] = 0xdd; // RDM / non-dimmer start code
        assert!(parse_e131(&p).is_none());
    }
}

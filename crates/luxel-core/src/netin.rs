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

#[cfg(test)]
mod tests {
    use super::*;

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

//! Device configuration persisted in flash — WiFi credentials first.
//!
//! Lives in the `nvs` partition (0x9000, 16 KB), used as a raw record
//! store: no_std land has no ESP-IDF NVS, and the partition is ours. One
//! record in the first sector:
//!
//!   "LXCF"  u8 version=1  u8 ssid_len  u8 pass_len  u8 0
//!   ssid bytes  pass bytes  u32-LE checksum (wrapping byte sum of header+data)
//!
//! Boot precedence (main.rs): flash record → compile-time LUXEL_SSID/PASS
//! → offline. Flash wins so a reconfigured device never falls back to a
//! stale baked network. This is what ends the credless-image lockout
//! class: once a creds record is written, ANY future image — even one
//! built without env creds — joins the network.

use alloc::string::String;
use alloc::vec::Vec;

pub const RECORD_OFFSET: u32 = 0x9000;
const MAGIC: &[u8; 4] = b"LXCF";
pub const MAX_SSID: usize = 32;
pub const MAX_PASS: usize = 64;

/// Device settings live in the next nvs sector, so writing them never disturbs
/// the WiFi record. Record (v3): "LXDV" u8 version=3  u8 brightness  u8 protocol
/// u8 0  u32-LE pixel_count  u32-LE checksum (16 bytes). Older versions fail the
/// version check and read as "no record" → defaults.
const DEV_OFFSET: u32 = 0xA000;
const DEV_MAGIC: &[u8; 4] = b"LXDV";
const DEV_VER: u8 = 3;

/// Persisted device settings. All fields are written together (read-modify-
/// write) so setting one never clobbers the others.
#[derive(Clone, Copy)]
pub struct DeviceConfig {
    pub brightness: u8,
    /// LED protocol code (leds::Protocol::as_u8).
    pub protocol: u8,
    pub pixel_count: u32,
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |a, &b| a.wrapping_add(b as u32))
}

/// Read the stored WiFi credentials, if a valid record exists.
pub fn read_wifi() -> Option<(String, String)> {
    let mut header = [0u8; 8];
    if !crate::assets::read_chunk(RECORD_OFFSET, &mut header) {
        return None;
    }
    if &header[0..4] != MAGIC || header[4] != 1 {
        return None;
    }
    let (slen, plen) = (header[5] as usize, header[6] as usize);
    if slen == 0 || slen > MAX_SSID || plen > MAX_PASS {
        return None;
    }
    let mut body = alloc::vec![0u8; slen + plen + 4];
    if !crate::assets::read_chunk(RECORD_OFFSET + 8, &mut body) {
        return None;
    }
    let (data, ck) = body.split_at(slen + plen);
    let stored = u32::from_le_bytes(ck.try_into().ok()?);
    if stored != checksum(&header).wrapping_add(checksum(data)) {
        return None;
    }
    let ssid = String::from_utf8(data[..slen].to_vec()).ok()?;
    let pass = String::from_utf8(data[slen..].to_vec()).ok()?;
    Some((ssid, pass))
}

/// Persist WiFi credentials (applied on next boot). Erases the record
/// sector, then writes via a word-aligned staging buffer (see
/// assets::read_chunk for why the unaligned esp-storage paths are off
/// limits: they put a 4 KiB bounce buffer on the task stack).
pub fn write_wifi(ssid: &str, pass: &str) -> Result<(), &'static str> {
    if ssid.is_empty() || ssid.len() > MAX_SSID {
        return Err("ssid must be 1..=32 bytes");
    }
    if pass.len() > MAX_PASS {
        return Err("password too long (max 64 bytes)");
    }
    let mut rec: Vec<u8> = Vec::with_capacity(8 + ssid.len() + pass.len() + 8);
    rec.extend_from_slice(MAGIC);
    rec.push(1);
    rec.push(ssid.len() as u8);
    rec.push(pass.len() as u8);
    rec.push(0);
    rec.extend_from_slice(ssid.as_bytes());
    rec.extend_from_slice(pass.as_bytes());
    let ck = checksum(&rec);
    rec.extend_from_slice(&ck.to_le_bytes());
    while rec.len() % 4 != 0 {
        rec.push(0xFF);
    }
    // word-aligned staging so write_nor takes the direct path
    let mut stage = alloc::vec![0u32; rec.len() / 4];
    let stage_bytes = unsafe {
        core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), rec.len())
    };
    stage_bytes.copy_from_slice(&rec);
    let ok = crate::ota::with_flash(|f| {
        use embedded_storage::nor_flash::NorFlash;
        NorFlash::erase(f, RECORD_OFFSET, RECORD_OFFSET + 4096).is_ok()
            && NorFlash::write(f, RECORD_OFFSET, stage_bytes).is_ok()
    })
    .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("flash write failed (update in progress?)")
    }
}

/// MQTT broker settings live in the third nvs sector. Record (v1):
/// "LXMQ" u8 version=1  u8 host_len  u8 user_len  u8 pass_len
/// u16-LE port  u8 0  u8 0  host bytes  user bytes  pass bytes
/// u32-LE checksum. No/invalid record = MQTT disabled.
const MQTT_OFFSET: u32 = 0xB000;
const MQTT_MAGIC: &[u8; 4] = b"LXMQ";
pub const MAX_MQTT_HOST: usize = 64;
pub const MAX_MQTT_CRED: usize = 64;

#[derive(Clone, Default)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

/// Read the stored MQTT broker settings, if a valid record exists.
pub fn read_mqtt() -> Option<MqttConfig> {
    let mut header = [0u8; 12];
    if !crate::assets::read_chunk(MQTT_OFFSET, &mut header) {
        return None;
    }
    if &header[0..4] != MQTT_MAGIC || header[4] != 1 {
        return None;
    }
    let (hlen, ulen, plen) = (header[5] as usize, header[6] as usize, header[7] as usize);
    if hlen == 0 || hlen > MAX_MQTT_HOST || ulen > MAX_MQTT_CRED || plen > MAX_MQTT_CRED {
        return None;
    }
    let port = u16::from_le_bytes([header[8], header[9]]);
    let mut body = alloc::vec![0u8; hlen + ulen + plen + 4];
    if !crate::assets::read_chunk(MQTT_OFFSET + 12, &mut body) {
        return None;
    }
    let (data, ck) = body.split_at(hlen + ulen + plen);
    let stored = u32::from_le_bytes(ck.try_into().ok()?);
    if stored != checksum(&header).wrapping_add(checksum(data)) {
        return None;
    }
    Some(MqttConfig {
        host: String::from_utf8(data[..hlen].to_vec()).ok()?,
        port: if port == 0 { 1883 } else { port },
        user: String::from_utf8(data[hlen..hlen + ulen].to_vec()).ok()?,
        pass: String::from_utf8(data[hlen + ulen..].to_vec()).ok()?,
    })
}

/// Persist MQTT broker settings; `None` clears the record (disables MQTT).
pub fn write_mqtt(cfg: Option<&MqttConfig>) -> Result<(), &'static str> {
    let mut rec: Vec<u8> = Vec::new();
    if let Some(c) = cfg {
        if c.host.is_empty() || c.host.len() > MAX_MQTT_HOST {
            return Err("host must be 1..=64 bytes");
        }
        if c.user.len() > MAX_MQTT_CRED || c.pass.len() > MAX_MQTT_CRED {
            return Err("user/password too long (max 64 bytes)");
        }
        rec.extend_from_slice(MQTT_MAGIC);
        rec.push(1);
        rec.push(c.host.len() as u8);
        rec.push(c.user.len() as u8);
        rec.push(c.pass.len() as u8);
        rec.extend_from_slice(&c.port.to_le_bytes());
        rec.push(0);
        rec.push(0);
        rec.extend_from_slice(c.host.as_bytes());
        rec.extend_from_slice(c.user.as_bytes());
        rec.extend_from_slice(c.pass.as_bytes());
        let ck = checksum(&rec);
        rec.extend_from_slice(&ck.to_le_bytes());
        while rec.len() % 4 != 0 {
            rec.push(0xFF);
        }
    }
    // word-aligned staging (same rationale as write_wifi); an empty record
    // is just the erase
    let mut stage = alloc::vec![0u32; rec.len() / 4];
    let stage_bytes =
        unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), rec.len()) };
    stage_bytes.copy_from_slice(&rec);
    let ok = crate::ota::with_flash(|f| {
        use embedded_storage::nor_flash::NorFlash;
        NorFlash::erase(f, MQTT_OFFSET, MQTT_OFFSET + 4096).is_ok()
            && (rec.is_empty() || NorFlash::write(f, MQTT_OFFSET, stage_bytes).is_ok())
    })
    .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("flash write failed (update in progress?)")
    }
}

/// Read the persisted device settings, if a valid record exists.
pub fn read_device() -> Option<DeviceConfig> {
    let mut rec = [0u8; 16]; // 12-byte body + 4-byte checksum
    if !crate::assets::read_chunk(DEV_OFFSET, &mut rec) {
        return None;
    }
    if &rec[0..4] != DEV_MAGIC || rec[4] != DEV_VER {
        return None;
    }
    let stored = u32::from_le_bytes(rec[12..16].try_into().ok()?);
    if stored != checksum(&rec[0..12]) {
        return None;
    }
    let brightness = rec[5];
    let protocol = rec[6];
    let pixel_count = u32::from_le_bytes(rec[8..12].try_into().ok()?);
    if brightness > 31 {
        return None;
    }
    Some(DeviceConfig {
        brightness,
        protocol,
        pixel_count,
    })
}

/// Persist device settings (brightness applied live; pixel count applied live
/// by the render task). Callers do a read-modify-write via read_device().
pub fn write_device(cfg: &DeviceConfig) -> Result<(), &'static str> {
    if cfg.brightness > 31 {
        return Err("brightness must be 0..=31");
    }
    let mut rec: Vec<u8> = Vec::with_capacity(16);
    rec.extend_from_slice(DEV_MAGIC);
    rec.push(DEV_VER);
    rec.push(cfg.brightness);
    rec.push(cfg.protocol);
    rec.push(0);
    rec.extend_from_slice(&cfg.pixel_count.to_le_bytes());
    let ck = checksum(&rec);
    rec.extend_from_slice(&ck.to_le_bytes());
    // word-aligned staging (same rationale as write_wifi)
    let mut stage = alloc::vec![0u32; rec.len() / 4];
    let stage_bytes =
        unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), rec.len()) };
    stage_bytes.copy_from_slice(&rec);
    let ok = crate::ota::with_flash(|f| {
        use embedded_storage::nor_flash::NorFlash;
        NorFlash::erase(f, DEV_OFFSET, DEV_OFFSET + 4096).is_ok()
            && NorFlash::write(f, DEV_OFFSET, stage_bytes).is_ok()
    })
    .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("flash write failed (update in progress?)")
    }
}

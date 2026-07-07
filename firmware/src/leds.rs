//! LED strip drivers. Both speak SPI so one peripheral covers both chip
//! families; the C3's GPIO matrix routes MOSI to any pin (e.g. the Athom
//! LS4P's GPIO21).

/// Which strip protocol to drive. Selected by `PROTOCOL` in main.rs; the
/// unselected variant is intentionally uninstantiated.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Protocol {
    /// APA102/SK9822: DATA+CLOCK, SPI mode 0 at ~8 MHz.
    Sk9822,
    /// WS281x family: single wire; each LED bit becomes 3 SPI bits
    /// (`100` = 0, `110` = 1) at 2.4 MHz → 416 ns units, inside WS2812
    /// timing tolerances. MOSI must idle low between frames.
    Ws2812,
}

impl Protocol {
    pub fn spi_hz(self) -> u32 {
        match self {
            Protocol::Sk9822 => 8_000_000,
            Protocol::Ws2812 => 2_400_000,
        }
    }

    /// Short lowercase name for the API (`/api/config`, `/api/protocol`).
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Sk9822 => "sk9822",
            Protocol::Ws2812 => "ws2812",
        }
    }

    /// Parse an API name (aliases: apa102 = sk9822, ws2811/ws2815 = ws2812).
    pub fn from_name(s: &str) -> Option<Protocol> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sk9822" | "apa102" => Some(Protocol::Sk9822),
            "ws2812" | "ws2811" | "ws2815" | "ws281x" => Some(Protocol::Ws2812),
            _ => None,
        }
    }

    /// Compact code for atomic storage / flash persistence.
    pub fn as_u8(self) -> u8 {
        match self {
            Protocol::Sk9822 => 0,
            Protocol::Ws2812 => 1,
        }
    }

    pub fn from_u8(v: u8) -> Protocol {
        match v {
            1 => Protocol::Ws2812,
            _ => Protocol::Sk9822,
        }
    }

    pub fn buf_len(self, pixels: usize) -> usize {
        match self {
            // 4B start + 4B/px + 4B SK9822 reset + px/16 end-clock bytes
            Protocol::Sk9822 => 4 + pixels * 4 + 4 + pixels.div_ceil(16),
            // 9 bytes per pixel (24 bits × 3), plus >280 µs low for latch:
            // 2.4 MHz → 300 µs ≈ 90 bytes of zeros
            Protocol::Ws2812 => pixels * 9 + 90,
        }
    }

    pub fn encode(self, rgb: &[[u8; 3]], brightness5: u8, out: &mut [u8]) {
        match self {
            Protocol::Sk9822 => encode_sk9822(rgb, brightness5, out),
            Protocol::Ws2812 => encode_ws2812(rgb, brightness5, out),
        }
    }
}

/// Scale an 8-bit channel by a 0–31 brightness level (31 = unchanged). Used
/// for WS2812, which has no hardware current field like SK9822's.
#[inline]
fn scale5(channel: u8, brightness5: u8) -> u8 {
    ((channel as u16 * (brightness5 & 0x1F) as u16) / 31) as u8
}

fn encode_sk9822(rgb: &[[u8; 3]], brightness5: u8, out: &mut [u8]) {
    let mut i = 4; // leading zeros already in place
    for px in rgb {
        out[i] = 0xE0 | (brightness5 & 0x1F);
        out[i + 1] = px[2]; // B
        out[i + 2] = px[1]; // G
        out[i + 3] = px[0]; // R
        i += 4;
    }
    // trailing reset + end clocks stay zero
}

/// Expand one byte into 24 SPI bits (3 per LED bit): `1` → `110`, `0` → `100`.
/// `brightness5` (0–31) scales each channel in software — WS2812 has no
/// hardware brightness field.
fn encode_ws2812(rgb: &[[u8; 3]], brightness5: u8, out: &mut [u8]) {
    let mut o = 0;
    for px in rgb {
        // WS2812 wants GRB
        for byte in [
            scale5(px[1], brightness5),
            scale5(px[0], brightness5),
            scale5(px[2], brightness5),
        ] {
            let mut acc: u32 = 0;
            for bit in 0..8 {
                let one = (byte >> (7 - bit)) & 1 == 1;
                acc = (acc << 3) | if one { 0b110 } else { 0b100 };
            }
            out[o] = (acc >> 16) as u8;
            out[o + 1] = (acc >> 8) as u8;
            out[o + 2] = acc as u8;
            o += 3;
        }
    }
    // latch tail stays zero
}

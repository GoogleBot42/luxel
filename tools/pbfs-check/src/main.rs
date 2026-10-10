//! Exercise firmware/src/pbfs.rs against a Pixelblaze v3 flash dump (or a
//! bare SPIFFS partition image). Prints what the takeover's inheritance
//! would see. The device name is never printed — only its length — so the
//! output is safe for logs; a dumped file has its `"name"` value redacted
//! the same way.

extern crate alloc;

#[path = "../../../firmware/src/pbfs.rs"]
mod pbfs;

/// Pixelblaze v3 partition table: SPIFFS data partition.
const SPIFFS_OFF: usize = 0x29_0000;
const SPIFFS_LEN: usize = 0x17_0000;

/// `"name":"…"` → `"name":"<redacted:N>"` for printing.
fn redact_name(bytes: &[u8]) -> Vec<u8> {
    let key = b"\"name\":\"";
    let Some(at) = bytes.windows(key.len()).position(|w| w == key) else {
        return bytes.to_vec();
    };
    let start = at + key.len();
    let Some(end) = bytes[start..].iter().position(|&c| c == b'"').map(|e| start + e) else {
        return bytes.to_vec();
    };
    let mut out = bytes[..start].to_vec();
    out.extend_from_slice(format!("<redacted:{}>", end - start).as_bytes());
    out.extend_from_slice(&bytes[end..]);
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: pbfs-check <flash-dump.bin | spiffs-image.bin> [file-to-dump ...]");
        std::process::exit(2);
    };
    let img = std::fs::read(&path).expect("read image");
    let region: &[u8] = if img.len() >= SPIFFS_OFF + SPIFFS_LEN {
        println!("full-flash image: SPIFFS at {SPIFFS_OFF:#x}+{SPIFFS_LEN:#x}");
        &img[SPIFFS_OFF..SPIFFS_OFF + SPIFFS_LEN]
    } else {
        println!("partition image: {} bytes", img.len());
        &img
    };
    let len = region.len() as u32;
    let mut read = |off: u32, buf: &mut [u8]| {
        let o = off as usize;
        match region.get(o..o + buf.len()) {
            Some(s) => {
                buf.copy_from_slice(s);
                true
            }
            None => false,
        }
    };

    match pbfs::extract_wiring(&mut read, len) {
        Some(w) => println!(
            "wiring: pixels={:?} protocol={:?} order={:?} bri_255={:?} max_bri_255={:?}",
            w.pixels, w.protocol, w.order, w.bri_255, w.max_bri_255
        ),
        None => println!("wiring: none (no Pixelblaze config found)"),
    }

    // provenance of what was parsed: which rev, how long, and the name's
    // length only
    if let Some(mut fs) = pbfs::PbFs::open(&mut read, len) {
        let live = fs.read_file_all("/config.json").len();
        let twin = fs.read_file_all("/config2.json").len();
        match fs.read_config() {
            Some(cfg) => println!(
                "config: rev={:?} {} bytes, live copies: config.json={live} config2.json={twin}, name_len={:?}, colorOrder={:?} ledType={:?} dataSpeed={:?}",
                pbfs::json_int(&cfg, "rev"),
                cfg.len(),
                pbfs::json_str(&cfg, "name").map(<[u8]>::len),
                pbfs::json_str(&cfg, "colorOrder").map(String::from_utf8_lossy),
                pbfs::json_int(&cfg, "ledType"),
                pbfs::json_int(&cfg, "dataSpeed"),
            ),
            None => println!("config: none"),
        }
    }

    for name in args {
        let Some(mut fs) = pbfs::PbFs::open(&mut read, len) else {
            println!("{name}: mount failed");
            continue;
        };
        match fs.read_file(&name) {
            Some(bytes) => {
                println!("{name}: {} bytes", bytes.len());
                std::io::Write::write_all(&mut std::io::stdout(), &redact_name(&bytes)).unwrap();
                println!();
            }
            None => println!("{name}: not found"),
        }
    }
}

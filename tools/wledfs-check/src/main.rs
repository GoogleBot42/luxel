//! Exercise firmware/src/wledfs.rs against a littlefs image on disk.
//! Prints what the takeover's inheritance would see. Secrets are never
//! printed — only lengths — so output is safe for logs.

extern crate alloc;

#[path = "../../../firmware/src/wledfs.rs"]
mod wledfs;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: wledfs-check <fs-image.bin> [file-to-dump ...]");
        std::process::exit(2);
    };
    let img = std::fs::read(&path).expect("read image");
    let len = img.len() as u32;
    let mut read = |off: u32, buf: &mut [u8]| {
        let o = off as usize;
        match img.get(o..o + buf.len()) {
            Some(s) => {
                buf.copy_from_slice(s);
                true
            }
            None => false,
        }
    };

    match wledfs::extract_wifi(&mut read, len) {
        Some((ssid, psk)) => println!("wifi: ssid={:?} psk_len={}", ssid, psk.len()),
        None => println!("wifi: none (factory-fresh or unreadable)"),
    }

    for name in args {
        let mut read = |off: u32, buf: &mut [u8]| {
            let o = off as usize;
            match img.get(o..o + buf.len()) {
                Some(s) => {
                    buf.copy_from_slice(s);
                    true
                }
                None => false,
            }
        };
        let Some(mut fs) = wledfs::WledFs::open(&mut read, len) else {
            println!("{name}: mount failed");
            continue;
        };
        match fs.read_file(&name) {
            Some(bytes) => {
                println!("{name}: {} bytes", bytes.len());
                std::io::Write::write_all(&mut std::io::stdout(), &bytes).unwrap();
                println!();
            }
            None => println!("{name}: not found"),
        }
    }
}

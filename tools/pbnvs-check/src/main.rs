//! Exercise firmware/src/pbnvs.rs against a real Pixelblaze flash dump.
//! Prints what the takeover's WiFi inheritance would see. Secrets are
//! NEVER printed — only credential lengths and the NVS page each came
//! from — so the output is safe for logs.
//!
//!   cargo run --release -- <flash-dump.bin> [--wipe-guard]
//!
//! --wipe-guard reproduces ota::preboot_guard's first-boot clobber of the
//! partition-relative 0x3000 page (absolute flash 0xC000): it erases that
//! page to 0xFF and stamps a foreign "LXBG" record over the top, then
//! extracts — proving the reader still recovers the creds from the
//! surviving pages.

extern crate alloc;

#[path = "../../../firmware/src/pbnvs.rs"]
mod pbnvs;

use std::cell::Cell;

// PB NVS partition geometry (absolute offsets in a 4 MiB dump).
const NVS_OFF: usize = 0x9000;
const NVS_LEN: usize = 0x5000; // 5 × 4096-byte pages
const PAGE: usize = 0x1000;
const GUARD_REL: usize = 0x3000; // preboot_guard page, partition-relative

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: pbnvs-check <flash-dump.bin> [--wipe-guard]");
        std::process::exit(2);
    };
    let wipe_guard = args.any(|a| a == "--wipe-guard");

    let img = std::fs::read(&path).expect("read flash dump");
    assert!(img.len() >= NVS_OFF + NVS_LEN, "dump too small for NVS partition");
    let mut region = img[NVS_OFF..NVS_OFF + NVS_LEN].to_vec();

    if wipe_guard {
        // Erase the page, then overlay a non-NVS "LXBG" guard record — the
        // harder case (page is written, not uninitialized, so the reader
        // must reject it on the version byte rather than the empty state).
        for b in &mut region[GUARD_REL..GUARD_REL + PAGE] {
            *b = 0xFF;
        }
        region[GUARD_REL..GUARD_REL + 4].copy_from_slice(b"LXBG");
        region[GUARD_REL + 8] = 0x00; // version byte: not 0xFE/0xFF
        println!("[--wipe-guard] clobbered partition-relative 0x{GUARD_REL:04X} (abs 0x{:X})", NVS_OFF + GUARD_REL);
    }

    let len = region.len() as u32;

    // Track the NVS page index of the last full-page (4096 B, page-aligned)
    // read, so we can report which page a value was lifted from. A
    // successful read_value / namespace_index returns on the page that
    // yielded the match, so the last full-page read during that call is it.
    let last_page: Cell<Option<u32>> = Cell::new(None);
    let mut read = |off: u32, buf: &mut [u8]| {
        let o = off as usize;
        if buf.len() == PAGE && o % PAGE == 0 {
            last_page.set(Some(off / PAGE as u32));
        }
        match region.get(o..o + buf.len()) {
            Some(s) => {
                buf.copy_from_slice(s);
                true
            }
            None => false,
        }
    };

    let fmt_page = |p: Option<u32>| match p {
        Some(p) => format!("page {p} (rel 0x{:04X}, abs 0x{:X})", p as usize * PAGE, NVS_OFF + p as usize * PAGE),
        None => "none".to_string(),
    };

    let mut nvs = pbnvs::PbNvs::open(&mut read, len);

    last_page.set(None);
    let ns = nvs.namespace_index("nvs.net80211");
    let ns_page = last_page.get();

    let (ssid_len, ssid_page, pass_len, pass_page) = match ns {
        Some(ns) => {
            last_page.set(None);
            let ssid = nvs.read_value(ns, "sta.ssid");
            let ssid_page = last_page.get();
            last_page.set(None);
            let pass = nvs.read_value(ns, "sta.pswd");
            let pass_page = last_page.get();
            (ssid.map(|v| v.len()), ssid_page, pass.map(|v| v.len()), pass_page)
        }
        None => (None, None, None, None),
    };

    // The decoded-credential lengths (what the takeover actually inherits)
    // come from extract_wifi; the raw-blob lengths above are only used to
    // confirm the page each key was read from.
    let _ = (ssid_len, pass_len);
    match pbnvs::extract_wifi(&mut read, len) {
        Some((ssid, pass)) => {
            // LENGTHS ONLY — never the bytes.
            println!("ssid_len={} pass_len={}", ssid.len(), pass.len());
            println!(
                "namespace nvs.net80211 -> index {} from {}",
                ns.unwrap(),
                fmt_page(ns_page)
            );
            println!("sta.ssid from {}", fmt_page(ssid_page));
            println!("sta.pswd from {}", fmt_page(pass_page));
        }
        None => println!("wifi: none (unprovisioned or unreadable)"),
    }
}

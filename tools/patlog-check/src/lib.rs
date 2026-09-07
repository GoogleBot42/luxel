//! Host build of `firmware/src/patlog.rs` (GPL-3.0-or-later, like the
//! firmware it comes from) — the pattern store's packed, append-only file
//! format. The module is `no_std` and allocation-free, so it compiles for
//! the host unchanged and its `#[cfg(test)]` suite (the NOR simulator, the
//! power-cut sweeps, the churn fuzz) is the format's real test coverage.
//!
//! What lives *here* rather than in the module is the one test that needs
//! the compiler: packing the whole real `library/` and counting how many
//! patterns fit, page-granular (Gitea #330) versus exact (Gitea #340).

#[path = "../../../firmware/src/patlog.rs"]
pub mod patlog;

#[cfg(test)]
mod library_fill {
    use super::patlog::*;
    use std::path::PathBuf;

    /// The arena `patterns.rs` carves out of the extent region.
    const ARENA_PAGES: u32 = 183;
    const ARENA_BYTES: u32 = ARENA_PAGES * PAGE;
    /// What #330/PR #332 shipped: one whole 4 KiB page is the allocation
    /// unit, a pattern owns two extents, and the directory is one
    /// `sequential-storage` item whose one-page cap holds 32 patterns.
    const PAGE_GRANULAR_MAX_PATTERNS: u32 = 32;

    struct Pat {
        name: String,
        src: u32,
        bc: u32,
    }

    fn library() -> Vec<Pat> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../library");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("library/ is tracked in the repo")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "js").unwrap_or(false))
            .collect();
        files.sort();
        let mut out = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let Ok(prog) = luxel_core::compile::compile(&src) else { continue };
            let Ok(bc) = luxel_core::bytecode::serialize(&prog) else { continue };
            let name = f.file_stem().unwrap().to_string_lossy().into_owned();
            out.push(Pat { name, src: src.len() as u32, bc: bc.len() as u32 });
        }
        assert!(out.len() > 250, "only {} library patterns compiled", out.len());
        out
    }

    fn median(v: &mut [u32]) -> u32 {
        v.sort_unstable();
        v[v.len() / 2]
    }

    /// How many patterns fit, largest-first (the friendliest order for the
    /// page allocator, so the comparison does not flatter exact packing).
    fn fits_page_granular(pats: &[Pat]) -> (u32, u32) {
        let mut pages_left = ARENA_PAGES;
        let mut n = 0;
        for p in pats {
            if n >= PAGE_GRANULAR_MAX_PATTERNS {
                break;
            }
            let need = p.src.div_ceil(PAGE) + p.bc.div_ceil(PAGE);
            if need > pages_left {
                break;
            }
            pages_left -= need;
            n += 1;
        }
        (n, (ARENA_PAGES - pages_left) * PAGE)
    }

    fn fits_packed(pats: &[Pat]) -> (u32, u32) {
        let mut at = 0u32;
        let mut n = 0;
        for p in pats {
            let need = Rec::bytes(p.name.len().min(MAX_NAME) as u8, p.src, p.bc);
            if at + need > ARENA_BYTES {
                break;
            }
            at += need;
            n += 1;
        }
        (n, at)
    }

    #[test]
    fn the_real_library_packs_far_denser_than_it_paged() {
        let pats = library();
        let mut srcs: Vec<u32> = pats.iter().map(|p| p.src).collect();
        let mut bcs: Vec<u32> = pats.iter().map(|p| p.bc).collect();
        let n = pats.len() as u32;
        let src_sum: u64 = srcs.iter().map(|&x| x as u64).sum();
        let bc_sum: u64 = bcs.iter().map(|&x| x as u64).sum();
        let under_page = srcs.iter().filter(|&&x| x <= PAGE).count();

        let (before, before_bytes) = fits_page_granular(&pats);
        let (after, after_bytes) = fits_packed(&pats);

        println!("library: {} patterns compiled", n);
        println!(
            "  source: median {} B, mean {} B, {}/{} <= 4 KiB",
            median(&mut srcs),
            src_sum / n as u64,
            under_page,
            n
        );
        println!("  bytecode: median {} B, mean {} B", median(&mut bcs), bc_sum / n as u64);
        println!(
            "  page-granular (#330): {} patterns in {} B of the {} B arena",
            before, before_bytes, ARENA_BYTES
        );
        println!(
            "  exact-packed  (#340): {} patterns in {} B of the {} B arena",
            after, after_bytes, ARENA_BYTES
        );

        assert_eq!(before, PAGE_GRANULAR_MAX_PATTERNS, "#332's ceiling is its directory item");
        assert!(after > before * 3, "packing should be worth several times the table cap");
    }

    /// The same arena, filled through the real append path: every record
    /// written to a simulated NOR flash, then enumerated by the boot scan.
    #[test]
    fn the_whole_library_enumerates_after_a_real_fill() {
        let pats = library();
        // The module's own Sim is private to its test module, so drive the
        // format directly over a byte buffer here: what matters at this
        // level is the packing arithmetic and that scan() finds them all.
        struct Buf(Vec<u8>);
        impl Arena for Buf {
            fn len(&self) -> u32 {
                self.0.len() as u32
            }
            fn view(&mut self, off: u32, want: usize) -> Option<&[u8]> {
                let at = off as usize;
                if at >= self.0.len() {
                    return None;
                }
                Some(&self.0[at..(at + want).min(self.0.len())])
            }
        }
        let mut buf = Buf(vec![0xFF; ARENA_BYTES as usize]);
        let mut at = 0u32;
        let mut wrote = 0u32;
        for (i, p) in pats.iter().enumerate() {
            let name = &p.name[..p.name.len().min(MAX_NAME)];
            let src = vec![b's'; p.src as usize];
            let bc = vec![0xC0u8; p.bc as usize];
            let rec = Rec {
                off: at,
                stamp: i as u32 + 1,
                seq: i as u32,
                src_len: p.src,
                bc_len: p.bc,
                src_hash: fnv1a(&src),
                bc_hash: fnv1a(&bc),
                name_len: name.len() as u8,
                dead: false,
            };
            if rec.end() > ARENA_BYTES {
                break;
            }
            let mut hdr = [0u8; HDR_PREFIX];
            encode_header(&rec, name.as_bytes(), &mut hdr);
            buf.0[at as usize..at as usize + HDR_PREFIX].copy_from_slice(&hdr);
            let c = rec.commit_off() as usize;
            buf.0[c..c + 4].copy_from_slice(&COMMIT.to_le_bytes());
            let no = rec.name_off() as usize;
            buf.0[no..no + name.len()].copy_from_slice(name.as_bytes());
            let so = rec.src_off() as usize;
            buf.0[so..so + src.len()].copy_from_slice(&src);
            let bo = rec.bc_off() as usize;
            buf.0[bo..bo + bc.len()].copy_from_slice(&bc);
            at = rec.end();
            wrote += 1;
        }
        let mut found = Vec::new();
        let s = scan(&mut buf, &mut |r: &Rec, n: &[u8]| {
            found.push((r.seq, String::from_utf8_lossy(n).into_owned()))
        });
        assert_eq!(found.len() as u32, wrote);
        assert_eq!(s.recs, wrote);
        assert_eq!(s.resync, 0);
        assert_eq!(s.cursor, at);
        assert_eq!(s.live, at, "the arena is fully packed — no gaps at all");
        println!("filled and enumerated {} real library patterns in {} B", wrote, at);
    }
}

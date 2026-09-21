//! LXBC construct census + the whole-program kind report over a set of
//! patterns — the phase-0 measurement behind docs/jit-design.md (Gitea
//! #607).
//!
//! Run: cargo run --release -p luxel-cli --example jitcensus -- library/*.js
//! (`--explain file.js` names every slot the inference leaves `Dyn`.)
//!
//! The inference, the verifier and the `Dyn` reasons are NOT reimplemented
//! here any more: this drives `luxel_core::kinds::{infer, verify, explain}`,
//! the same code the compiler runs and the decoder checks. What stays local
//! is the construct scan (which opcodes a pattern uses, static stack depth,
//! the call graph) — plain counting over the word stream.

use luxel_core::kinds::{self, DynCause, DynSlot, Kind};
use luxel_core::vm::Program;
use std::collections::{BTreeMap, BTreeSet};

// ---- opcodes (mirror of bytecode::op, which is pub(crate)) ----
const CONST_NUM: u8 = 0x01;
const CONST_FUN: u8 = 0x02;
const LOAD_G: u8 = 0x04;
const STORE_G: u8 = 0x05;
const LOAD_L: u8 = 0x06;
const STORE_L: u8 = 0x07;
const LOAD_IDX: u8 = 0x08;
const STORE_IDX: u8 = 0x09;
const ARR_LEN: u8 = 0x0A;
const NEW_ARRAY: u8 = 0x0B;
const DUP: u8 = 0x0C;
const DUP2: u8 = 0x0D;
const POP: u8 = 0x0E;
const CONST_ARR: u8 = 0x0F;
const ADD: u8 = 0x10;
const POW_: u8 = 0x15;
const NEG: u8 = 0x16;
const BIT_NOT: u8 = 0x18;
const BIT_AND: u8 = 0x19;
const SHR: u8 = 0x1D;
const LT: u8 = 0x20;
const NE: u8 = 0x25;
const JMP: u8 = 0x30;
const JMP_IF_FALSE: u8 = 0x31;
const JMP_IF_TRUE_PEEK: u8 = 0x32;
const JMP_IF_FALSE_PEEK: u8 = 0x33;
const CALL_FN: u8 = 0x38;
const CALL_BUILTIN: u8 = 0x39;
const CALL_VALUE: u8 = 0x3A;
const RET: u8 = 0x3E;
const RET_NULL: u8 = 0x3F;
const ASSERT: u8 = 0x40;
const STORE_L_POP: u8 = 0x41;
const STORE_G_POP: u8 = 0x42;
const LOAD_LL: u8 = 0x43;
const LOAD_LG: u8 = 0x44;
const LOAD_GL: u8 = 0x45;
const LOAD_L_IDX: u8 = 0x46;
const LOAD_G_L_IDX: u8 = 0x47;
const CONST_OP: u8 = 0x48;
const LOAD_L_CONST_OP: u8 = 0x49;
const LOAD_G_CONST_OP: u8 = 0x4A;
const CALL_BUILTIN_C: u8 = 0x4B;
const CALL_BUILTIN_CC: u8 = 0x4C;
const CMP_JF: u8 = 0x4D;
const POP_RET_NULL: u8 = 0x4E;
const BOX: u8 = 0x4F;

fn opcode(w: u32) -> u8 {
    w as u8
}
fn imm8(w: u32) -> u8 {
    (w >> 8) as u8
}
fn imm16(w: u32) -> u16 {
    (w >> 8) as u16
}
fn imm24(w: u32) -> u32 {
    w >> 8
}
fn argcf(w: u32) -> u8 {
    (w >> 24) as u8
}
fn imm8b(w: u32) -> u8 {
    (w >> 16) as u8
}
fn imm16hi(w: u32) -> u16 {
    (w >> 16) as u16
}

fn ilen(op: u8) -> usize {
    match op {
        CONST_NUM | CONST_OP | LOAD_L_CONST_OP | LOAD_G_CONST_OP | CALL_BUILTIN_C | CMP_JF => 2,
        CALL_BUILTIN_CC => 3,
        _ => 1,
    }
}

fn fn_code(prog: &Program, fi: usize) -> &[u32] {
    let f = &prog.fns[fi];
    let s = f.code_start as usize;
    &prog.words[s..s + f.code_len as usize]
}

/// Net stack effect and pops of one instruction — enough for the static
/// depth walk and nothing more.
fn stack_effect(w: u32) -> (usize, usize) {
    let op = opcode(w);
    let argc = argcf(w) as usize;
    match op {
        CONST_NUM | CONST_FUN | 0x03 | LOAD_G | LOAD_L | CONST_ARR | LOAD_L_CONST_OP
        | LOAD_G_CONST_OP => (0, 1),
        NEW_ARRAY => (imm16(w) as usize, 1),
        LOAD_LL | LOAD_LG | LOAD_GL => (0, 2),
        DUP => (0, 1),
        DUP2 => (0, 2),
        POP | ASSERT | STORE_L_POP | STORE_G_POP | JMP_IF_FALSE | RET | POP_RET_NULL => (1, 0),
        STORE_L | STORE_G | JMP_IF_TRUE_PEEK | JMP_IF_FALSE_PEEK | BOX => (0, 0),
        LOAD_IDX => (2, 1),
        LOAD_L_IDX => (1, 1),
        LOAD_G_L_IDX => (0, 1),
        STORE_IDX => (3, 1),
        ARR_LEN | NEG | 0x17 | BIT_NOT | CONST_OP => (1, 1),
        CMP_JF => (2, 0),
        CALL_FN => (argc, 1),
        CALL_BUILTIN => (argc, 1),
        CALL_BUILTIN_C => (argc.saturating_sub(1), 1),
        CALL_BUILTIN_CC => (argc.saturating_sub(2), 1),
        CALL_VALUE => (imm8(w) as usize + 1, 1),
        JMP | RET_NULL => (0, 0),
        o if (ADD..=POW_).contains(&o) || (BIT_AND..=SHR).contains(&o) || (LT..=NE).contains(&o) => {
            (2, 1)
        }
        _ => (0, 0),
    }
}

/// What the construct scan finds in one compiled program: which slots each
/// function touches, who it calls, and the opcodes that decide the v1
/// exclusion set.
#[derive(Default)]
struct Scan {
    uses_call_value: bool,
    uses_const_fun: bool,
    hof_builtins: BTreeSet<String>,
    hof_with_fun_cb: bool,
    nonnum_arrstore: bool,
    boxes: usize,
    const_fun_fns: BTreeSet<usize>,
    ref_locals: Vec<BTreeSet<usize>>,
    ref_globals: Vec<BTreeSet<usize>>,
    callees: Vec<BTreeSet<usize>>,
    fn_uses_dyncall: Vec<bool>,
    max_stack: Vec<u32>,
}

fn scan(prog: &Program) -> Scan {
    let nf = prog.fns.len();
    let mut s = Scan {
        ref_locals: vec![BTreeSet::new(); nf],
        ref_globals: vec![BTreeSet::new(); nf],
        callees: vec![BTreeSet::new(); nf],
        fn_uses_dyncall: vec![false; nf],
        max_stack: vec![0; nf],
        ..Default::default()
    };
    for fi in 0..nf {
        let code = fn_code(prog, fi);
        // static depth: a worklist walk, same shape the verifier makes
        let mut depth: BTreeMap<usize, u32> = BTreeMap::new();
        depth.insert(0, 0);
        let mut work = vec![0usize];
        let mut maxd = 0u32;
        while let Some(at) = work.pop() {
            if at >= code.len() {
                continue;
            }
            let d = depth[&at];
            maxd = maxd.max(d);
            let w = code[at];
            let op = opcode(w);
            let (pops, pushes) = stack_effect(w);
            let nd = d.saturating_sub(pops as u32) + pushes as u32;
            maxd = maxd.max(nd);
            let mut succ: Vec<usize> = Vec::new();
            match op {
                JMP => succ.push(imm24(w) as usize),
                JMP_IF_FALSE | JMP_IF_TRUE_PEEK | JMP_IF_FALSE_PEEK => {
                    succ.push(imm24(w) as usize);
                    succ.push(at + ilen(op));
                }
                CMP_JF => {
                    succ.push(code.get(at + 1).copied().unwrap_or(0) as usize);
                    succ.push(at + ilen(op));
                }
                RET | RET_NULL | POP_RET_NULL => {}
                _ => succ.push(at + ilen(op)),
            }
            for t in succ {
                if t < code.len() && depth.insert(t, nd).is_none() {
                    work.push(t);
                }
            }
        }
        s.max_stack[fi] = maxd;

        let mut at = 0usize;
        while at < code.len() {
            let w = code[at];
            let op = opcode(w);
            match op {
                CONST_FUN => {
                    s.uses_const_fun = true;
                    s.const_fun_fns.insert(imm16(w) as usize);
                }
                CALL_VALUE => {
                    s.uses_call_value = true;
                    s.fn_uses_dyncall[fi] = true;
                }
                CALL_FN => {
                    s.callees[fi].insert(imm16(w) as usize);
                }
                STORE_IDX => s.nonnum_arrstore |= true,
                BOX => s.boxes += 1,
                _ => {}
            }
            match op {
                LOAD_L | STORE_L | STORE_L_POP | LOAD_L_IDX | LOAD_L_CONST_OP => {
                    s.ref_locals[fi].insert(imm8(w) as usize);
                }
                LOAD_LL => {
                    s.ref_locals[fi].insert(imm8(w) as usize);
                    s.ref_locals[fi].insert(imm8b(w) as usize);
                }
                LOAD_LG => {
                    s.ref_locals[fi].insert(imm8(w) as usize);
                    s.ref_globals[fi].insert(imm16hi(w) as usize);
                }
                LOAD_GL | LOAD_G_L_IDX => {
                    s.ref_globals[fi].insert(imm16(w) as usize);
                    s.ref_locals[fi].insert(argcf(w) as usize);
                }
                LOAD_G | STORE_G | STORE_G_POP | LOAD_G_CONST_OP => {
                    s.ref_globals[fi].insert(imm16(w) as usize);
                }
                _ => {}
            }
            at += ilen(op);
        }
    }
    // "stores a provably non-Num into an array" needs kinds; recomputed by
    // the caller from the inference. The scan only flags that a StoreIdx
    // exists at all, which it then refines.
    s.nonnum_arrstore = false;
    s
}

const ENTRY_NAMES: [&str; 4] = ["render", "render2D", "render3D", "renderFrame"];

/// Functions the engine can reach from a render entry.
fn render_path(prog: &Program, s: &Scan) -> BTreeSet<usize> {
    let mut roots: BTreeSet<usize> = BTreeSet::new();
    for n in ENTRY_NAMES {
        if let Some(f) = prog.exported_fn(n) {
            roots.insert(f as usize);
        }
        // PB also dispatches through a GLOBAL of that name holding a fn
        if prog.global_index(n).is_some() {
            for &f in &s.const_fun_fns {
                roots.insert(f);
            }
        }
    }
    let mut seen = roots.clone();
    let mut stack: Vec<usize> = roots.into_iter().collect();
    let mut dyn_on_path = false;
    while let Some(f) = stack.pop() {
        if s.fn_uses_dyncall[f] {
            dyn_on_path = true;
        }
        for &c in &s.callees[f] {
            if seen.insert(c) {
                stack.push(c);
            }
        }
    }
    if dyn_on_path {
        // conservative: any function value could be the callee
        for &f in &s.const_fun_fns {
            seen.insert(f);
        }
        let mut stack: Vec<usize> = seen.iter().copied().collect();
        while let Some(f) = stack.pop() {
            for &c in &s.callees[f] {
                if seen.insert(c) {
                    stack.push(c);
                }
            }
        }
    }
    seen
}

#[derive(Default, serde::Serialize)]
struct Rec {
    file: String,
    ok: bool,
    err: Option<String>,
    entries: Vec<String>,
    renderframe: bool,
    n_fns: usize,
    n_globals: usize,
    n_locals: usize,
    code_words: u32,
    max_stack: u32,
    uses_call_value: bool,
    uses_const_fun: bool,
    hofs: Vec<String>,
    hof_fun_cb: bool,
    nonnum_arrstore: bool,
    boxes: usize,
    verified: bool,
    typed: bool,
    locals_num: usize,
    locals_tot: usize,
    g_num: usize,
    g_arrnum: usize,
    g_arr: usize,
    g_other: usize,
    g_dyn: usize,
    g_tot: usize,
    gu_num: usize,
    gu_dyn: usize,
    gu_tot: usize,
    rp_fns: usize,
    rp_slots: usize,
    rp_dyn: usize,
    rp_locals: usize,
    rp_locals_num: usize,
    ref_locals_tot: usize,
    ref_locals_num: usize,
    fn_stacks: Vec<u32>,
    fully_typed: bool,
    reasons: Vec<String>,
    slot_reasons: Vec<String>,
}

fn analyze(path: &str, src: &str) -> Rec {
    let mut r = Rec {
        file: path.to_string(),
        ..Default::default()
    };
    let prog = match luxel_core::compile::compile(src) {
        Ok(p) => p,
        Err(d) => {
            r.err = Some(d.message);
            return r;
        }
    };
    r.ok = true;
    r.n_fns = prog.fns.len();
    r.n_globals = prog.globals.len();
    r.n_locals = prog.fns.iter().map(|f| f.locals as usize).sum();
    r.code_words = prog.fns.iter().map(|f| f.code_len).sum();
    for n in ENTRY_NAMES {
        if prog.exported_fn(n).is_some() {
            r.entries.push(n.to_string());
        }
    }
    r.renderframe = r.entries.iter().any(|e| e == "renderFrame");

    let s = scan(&prog);
    r.uses_call_value = s.uses_call_value;
    r.uses_const_fun = s.uses_const_fun;
    r.hofs = s.hof_builtins.iter().cloned().collect();
    r.hof_fun_cb = s.hof_with_fun_cb;
    r.boxes = s.boxes;
    r.fn_stacks = s.max_stack.clone();
    r.max_stack = s.max_stack.iter().copied().max().unwrap_or(0);

    // the compiler attached its own inference; recompute to prove the two
    // agree, and verify the result
    let kinds = kinds::infer(&prog);
    r.typed = prog.kinds.as_ref() == Some(&kinds);
    r.verified = kinds::verify(&prog, &kinds).is_ok();
    let reasons = kinds::explain(&prog, &kinds);

    // an array that holds anything but numbers
    r.nonnum_arrstore = kinds.globals.iter().any(|&k| k == Kind::Arr);

    r.locals_tot = r.n_locals;
    r.locals_num = (0..prog.fns.len())
        .map(|f| {
            (0..prog.fns[f].locals as usize)
                .filter(|&i| kinds.slot(f, i) == Kind::Num)
                .count()
        })
        .sum();
    r.g_tot = prog.globals.len();
    for gi in 0..prog.globals.len() {
        let pre = prog.globals[gi].predefined;
        if !pre {
            r.gu_tot += 1;
        }
        match kinds.global(gi) {
            Kind::Num => {
                r.g_num += 1;
                if !pre {
                    r.gu_num += 1;
                }
            }
            Kind::ArrNum => r.g_arrnum += 1,
            Kind::Arr => r.g_arr += 1,
            Kind::Dyn => {
                r.g_dyn += 1;
                if !pre {
                    r.gu_dyn += 1;
                }
            }
            _ => r.g_other += 1,
        }
    }
    r.ref_locals_tot = (0..prog.fns.len()).map(|f| s.ref_locals[f].len()).sum();
    r.ref_locals_num = (0..prog.fns.len())
        .map(|f| {
            s.ref_locals[f]
                .iter()
                .filter(|&&i| kinds.slot(f, i) == Kind::Num)
                .count()
        })
        .sum();

    let seen = render_path(&prog, &s);
    r.rp_fns = seen.len();
    let why = |slot: DynSlot| -> Option<DynCause> {
        reasons.iter().find(|d| d.slot == slot).map(|d| d.cause)
    };
    let mut causes: BTreeSet<String> = BTreeSet::new();
    for &f in &seen {
        for &i in &s.ref_locals[f] {
            if i >= prog.fns[f].locals as usize {
                continue;
            }
            r.rp_slots += 1;
            r.rp_locals += 1;
            let k = kinds.slot(f, i);
            if k == Kind::Num {
                r.rp_locals_num += 1;
            }
            if k == Kind::Dyn {
                r.rp_dyn += 1;
                let t = why(DynSlot::Local {
                    fn_idx: f as u16,
                    slot: i as u8,
                })
                .map(|c| c.text().to_string())
                .unwrap_or_else(|| "unattributed".to_string());
                causes.insert(t.clone());
                r.slot_reasons.push(t);
            }
        }
        for &g in &s.ref_globals[f] {
            r.rp_slots += 1;
            if kinds.global(g) == Kind::Dyn {
                r.rp_dyn += 1;
                let t = why(DynSlot::Global(g as u16))
                    .map(|c| c.text().to_string())
                    .unwrap_or_else(|| "unattributed".to_string());
                causes.insert(t.clone());
                r.slot_reasons.push(t);
            }
        }
    }
    r.fully_typed = r.rp_dyn == 0;
    r.reasons = causes.into_iter().collect();
    r
}

/// Print every `Dyn` slot on one pattern's render path, named.
fn explain_one(path: &str) {
    let src = std::fs::read_to_string(path).unwrap();
    let prog = luxel_core::compile::compile(&src).unwrap();
    let kinds = kinds::infer(&prog);
    let reasons = kinds::explain(&prog, &kinds);
    let s = scan(&prog);
    let seen = render_path(&prog, &s);
    let why = |slot: DynSlot| -> String {
        reasons
            .iter()
            .find(|d| d.slot == slot)
            .map(|d| d.cause.text().to_string())
            .unwrap_or_else(|| "unattributed".to_string())
    };
    println!(
        "{path}: render-path fns {:?}",
        seen.iter()
            .map(|&f| prog.fns[f].name.clone())
            .collect::<Vec<_>>()
    );
    for &f in &seen {
        for &i in &s.ref_locals[f] {
            if i >= prog.fns[f].locals as usize || kinds.slot(f, i) != Kind::Dyn {
                continue;
            }
            let nm = prog.fns[f].local_names.get(i).cloned().unwrap_or_default();
            println!(
                "  Dyn local {}:{} ({}) -- {}",
                prog.fns[f].name,
                nm,
                i,
                why(DynSlot::Local {
                    fn_idx: f as u16,
                    slot: i as u8
                })
            );
        }
        for &g in &s.ref_globals[f] {
            if kinds.global(g) != Kind::Dyn {
                continue;
            }
            println!(
                "  Dyn global {} -- {}",
                prog.globals[g].name,
                why(DynSlot::Global(g as u16))
            );
        }
    }
    for g in 0..prog.globals.len() {
        if prog.globals[g].predefined {
            continue;
        }
        println!(
            "  global {} : {}",
            prog.globals[g].name,
            kinds.global(g).name()
        );
    }
    for f in 0..prog.fns.len() {
        println!("  ret {} : {}", prog.fns[f].name, kinds.ret(f).name());
    }
    let boxes: usize = (0..prog.fns.len())
        .map(|f| {
            let code = fn_code(&prog, f);
            let mut at = 0;
            let mut n = 0;
            while at < code.len() {
                if opcode(code[at]) == BOX {
                    n += 1;
                }
                at += ilen(opcode(code[at]));
            }
            n
        })
        .sum();
    println!("  Box instructions: {boxes}");
    match kinds::verify(&prog, &kinds) {
        Ok(()) => println!("  verify: ok"),
        Err(e) => println!("  verify: FAILED {e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--explain") {
        for p in &args[1..] {
            explain_one(p);
        }
        return;
    }
    let mut recs = Vec::new();
    for p in &args {
        let src = std::fs::read_to_string(p).unwrap_or_default();
        recs.push(analyze(p, &src));
    }
    let jsonl: Vec<String> = recs
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    std::fs::write("/tmp/jitcensus.jsonl", jsonl.join("\n")).ok();

    let ok: Vec<&Rec> = recs.iter().filter(|r| r.ok).collect();
    let n = ok.len();
    println!(
        "patterns: {} total, {} compiled, {} failed",
        recs.len(),
        n,
        recs.len() - n
    );
    for r in recs.iter().filter(|r| !r.ok) {
        println!("  FAIL {}: {}", r.file, r.err.clone().unwrap_or_default());
    }

    let cnt = |f: &dyn Fn(&&Rec) -> bool| ok.iter().filter(|r| f(r)).count();
    println!("\n== construct census (patterns) ==");
    println!(
        "uses CallValue                      {}",
        cnt(&|r| r.uses_call_value)
    );
    println!(
        "uses ConstFun (function values)     {}",
        cnt(&|r| r.uses_const_fun)
    );
    println!(
        "calls a higher-order builtin        {}",
        cnt(&|r| !r.hofs.is_empty())
    );
    println!(
        "holds a non-Num array (Arr, not ArrNum) {}",
        cnt(&|r| r.nonnum_arrstore)
    );
    println!(
        "needs a Box (kind-differing join)   {} patterns, {} sites",
        cnt(&|r| r.boxes > 0),
        ok.iter().map(|r| r.boxes).sum::<usize>()
    );
    println!(
        "renderFrame                          {}",
        cnt(&|r| r.renderframe)
    );
    println!(
        "render/render2D/render3D             {}",
        cnt(&|r| r.entries.iter().any(|e| e != "renderFrame"))
    );
    println!(
        "no render entry at all               {}",
        cnt(&|r| r.entries.is_empty())
    );
    let mut hofhist: BTreeMap<String, usize> = BTreeMap::new();
    for r in &ok {
        for h in &r.hofs {
            *hofhist.entry(h.clone()).or_default() += 1;
        }
    }
    println!("HOF builtin usage: {hofhist:?}");
    let mut ehist: BTreeMap<String, usize> = BTreeMap::new();
    for r in &ok {
        for e in &r.entries {
            *ehist.entry(e.clone()).or_default() += 1;
        }
    }
    println!("entries: {ehist:?}");

    let stat = |v: &mut Vec<u64>| {
        v.sort();
        (v[0], v[v.len() / 2], v[v.len() - 1], v.iter().sum::<u64>())
    };
    let mut cw: Vec<u64> = ok.iter().map(|r| r.code_words as u64).collect();
    let (a, b, c, d) = stat(&mut cw);
    println!("\ncode words/pattern: min {a} median {b} max {c} total {d}");
    let mut fs: Vec<u64> = ok
        .iter()
        .flat_map(|r| r.fn_stacks.iter().map(|&x| x as u64))
        .collect();
    let nfn = fs.len();
    let over8 = fs.iter().filter(|&&x| x > 8).count();
    let (a, b, c, _) = stat(&mut fs);
    println!(
        "max operand-stack depth (per function, n={nfn}): min {a} median {b} max {c}; >8 deep: {over8}"
    );
    let mut nl: Vec<u64> = ok.iter().map(|r| r.n_locals as u64).collect();
    let (a, b, c, d) = stat(&mut nl);
    println!("locals/pattern: min {a} median {b} max {c} total {d}");
    let mut ng: Vec<u64> = ok.iter().map(|r| r.n_globals as u64).collect();
    let (a, b, c, d) = stat(&mut ng);
    println!("globals/pattern: min {a} median {b} max {c} total {d}");
    let mut nf: Vec<u64> = ok.iter().map(|r| r.n_fns as u64).collect();
    let (a, b, c, d) = stat(&mut nf);
    println!("fns/pattern: min {a} median {b} max {c} total {d}");

    println!("\n== kind inference (luxel_core::kinds) ==");
    println!(
        "compiler's attached kinds == a fresh infer(): {} / {n}",
        cnt(&|r| r.typed)
    );
    println!("verify() passes:                             {} / {n}", cnt(&|r| r.verified));
    let lt: usize = ok.iter().map(|r| r.locals_tot).sum();
    let ln: usize = ok.iter().map(|r| r.locals_num).sum();
    println!(
        "locals proven Num: {ln}/{lt} = {:.1}%",
        100.0 * ln as f64 / lt as f64
    );
    let rlt: usize = ok.iter().map(|r| r.ref_locals_tot).sum();
    let rln: usize = ok.iter().map(|r| r.ref_locals_num).sum();
    println!(
        "locals actually REFERENCED, proven Num: {rln}/{rlt} = {:.1}%",
        100.0 * rln as f64 / rlt as f64
    );
    let rpl: usize = ok.iter().map(|r| r.rp_locals).sum();
    let rpn: usize = ok.iter().map(|r| r.rp_locals_num).sum();
    println!(
        "locals on the RENDER PATH, proven Num:  {rpn}/{rpl} = {:.1}%",
        100.0 * rpn as f64 / rpl as f64
    );
    let gt: usize = ok.iter().map(|r| r.g_tot).sum();
    let gn: usize = ok.iter().map(|r| r.g_num).sum();
    let gan: usize = ok.iter().map(|r| r.g_arrnum).sum();
    let ga: usize = ok.iter().map(|r| r.g_arr).sum();
    let go: usize = ok.iter().map(|r| r.g_other).sum();
    let gdyn: usize = ok.iter().map(|r| r.g_dyn).sum();
    println!(
        "globals: {gt} total | Num {gn} ({:.1}%) | ArrNum {gan} | Arr {ga} | Fun/Builtin {go} | Dyn {gdyn}",
        100.0 * gn as f64 / gt as f64
    );
    let gut: usize = ok.iter().map(|r| r.gu_tot).sum();
    let gun: usize = ok.iter().map(|r| r.gu_num).sum();
    let gudyn: usize = ok.iter().map(|r| r.gu_dyn).sum();
    println!(
        "  user globals only (predefined excluded): {gut} | Num {gun} ({:.1}%) | Dyn {gudyn}",
        100.0 * gun as f64 / gut as f64
    );
    println!(
        "fully typed render path: {} / {n}",
        cnt(&|r| r.fully_typed && !r.entries.is_empty())
    );
    println!(
        "any Dyn slot on render path: {}",
        cnt(&|r| !r.fully_typed)
    );
    let mut rh: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for r in &ok {
        for whyt in &r.reasons {
            let e = rh.entry(whyt.clone()).or_default();
            e.0 += 1;
            if e.1.len() < 3 {
                e.1.push(r.file.rsplit('/').next().unwrap().to_string());
            }
        }
    }
    let mut rv: Vec<_> = rh.into_iter().collect();
    rv.sort_by_key(|(_, (c, _))| std::cmp::Reverse(*c));
    println!("\n== top Dyn reasons on the render path ==");
    for (whyt, (c, ex)) in rv.iter().take(12) {
        println!("{c:4} patterns  {whyt}  e.g. {}", ex.join(", "));
    }
    let mut sh: BTreeMap<String, usize> = BTreeMap::new();
    for r in &ok {
        for w in &r.slot_reasons {
            *sh.entry(w.clone()).or_default() += 1;
        }
    }
    let mut sv: Vec<_> = sh.into_iter().collect();
    sv.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    println!(
        "-- same, counted per Dyn SLOT (n={}) --",
        sv.iter().map(|(_, c)| c).sum::<usize>()
    );
    for (whyt, c) in sv.iter().take(12) {
        println!("{c:4} slots  {whyt}");
    }

    println!("\n== first-JIT exclusion (bail on CallValue; no builtin reaches pattern code since #626) ==");
    let excl = |r: &&Rec| r.uses_call_value || r.hof_fun_cb;
    let e = cnt(&excl);
    println!("excluded: {e} / {n} ({:.1}%)", 100.0 * e as f64 / n as f64);
    println!(
        "fully typed render path AND not excluded: {}",
        cnt(&|r| r.fully_typed && !r.entries.is_empty() && !excl(r))
    );
    println!(
        "\nEXCLUDED patterns: {:?}",
        ok.iter()
            .filter(|r| excl(r))
            .map(|r| r.file.rsplit('/').next().unwrap())
            .collect::<Vec<_>>()
    );
    println!(
        "\nDyn-on-render-path patterns: {:?}",
        ok.iter()
            .filter(|r| !r.fully_typed)
            .map(|r| r.file.rsplit('/').next().unwrap())
            .collect::<Vec<_>>()
    );
    println!(
        "\nBox sites: {:?}",
        ok.iter()
            .filter(|r| r.boxes > 0)
            .map(|r| (r.file.rsplit('/').next().unwrap(), r.boxes))
            .collect::<Vec<_>>()
    );
}

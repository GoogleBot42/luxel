//! LXBC construct census + prototype whole-program kind inference over a set of
//! patterns — the phase-0 measurement behind docs/jit-design.md (Gitea #607).
//! Run: cargo run --release -p luxel-cli --example jitcensus -- library/*.js
//! (`--explain file.js` names every slot the inference leaves Unknown.)
#![allow(clippy::too_many_arguments)]

use luxel_core::vm::{Program, BUILTINS};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ---- opcodes (mirror of bytecode::op, which is pub(crate)) ----
const CONST_NUM: u8 = 0x01;
const CONST_FUN: u8 = 0x02;
const CONST_BUILTIN: u8 = 0x03;
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
const NOT: u8 = 0x17;
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

// ---- lattice ----
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Set {
    top: bool,
    bits: u128,
}
impl Set {
    fn one(i: usize) -> Set {
        if i < 128 {
            Set {
                top: false,
                bits: 1u128 << i,
            }
        } else {
            Set { top: true, bits: 0 }
        }
    }
    fn union(a: Set, b: Set) -> Set {
        Set {
            top: a.top || b.top,
            bits: a.bits | b.bits,
        }
    }
    fn ids(&self) -> Vec<usize> {
        (0..128).filter(|i| (self.bits >> i) & 1 == 1).collect()
    }
    fn empty(&self) -> bool {
        !self.top && self.bits == 0
    }
}

#[derive(Clone, Copy, Debug)]
enum K {
    Bot,
    Num,
    Arr(Set),
    Fun(Set),
    Bi,
    Top(&'static str),
}

impl PartialEq for K {
    fn eq(&self, o: &K) -> bool {
        match (self, o) {
            (K::Bot, K::Bot) | (K::Num, K::Num) | (K::Bi, K::Bi) | (K::Top(_), K::Top(_)) => true,
            (K::Arr(a), K::Arr(b)) => a == b,
            (K::Fun(a), K::Fun(b)) => a == b,
            _ => false,
        }
    }
}

fn join(a: K, b: K, ctx: &'static str) -> K {
    match (a, b) {
        (K::Bot, x) => x,
        (x, K::Bot) => x,
        (K::Top(r), _) => K::Top(r),
        (_, K::Top(r)) => K::Top(r),
        (K::Num, K::Num) => K::Num,
        (K::Bi, K::Bi) => K::Bi,
        (K::Arr(x), K::Arr(y)) => K::Arr(Set::union(x, y)),
        (K::Fun(x), K::Fun(y)) => K::Fun(Set::union(x, y)),
        _ => K::Top(ctx),
    }
}

const R_SLOT: &'static str = "slot holds two different kinds (assignment merge)";
const R_JOIN: &'static str = "ternary / short-circuit join of differing kinds";
const R_CALLVALUE: &'static str = "CallValue result (calling a function value)";
const R_CBPARAM: &'static str = "param of a function reached only via callback/CallValue";
const R_BIUNKNOWN: &'static str = "builtin whose return we classify Unknown (arrayReduce)";
const R_AOA: &'static str =
    "load from an array whose elements are not all Num (array of arrays / of functions / mixed)";
const R_POISON: &'static str = "arrays poisoned by a store of unknown provenance";
const R_TOPARR: &'static str = "ArrLoad from an array of unknown provenance";
const R_ARGS: &'static str = "call argument merge";
const R_ELEM: &'static str = "array holds both Num and non-Num elements";

// ---- builtin classification ----
// The only builtin that ALLOCATES an array is `array(n)`.
// These return one of their array arguments verbatim (verified by reading
// every `Ok(a(i))` arm in vm.rs).
fn passthrough_arg(name: &str) -> Option<usize> {
    Some(match name {
        "arrayForEach" | "arrayMutate" | "arrayReplace" | "arrayReplaceAt" | "arraySort"
        | "arraySortBy" | "blur1D" | "feedback" | "arrayScale" | "blur2D" | "arrayAdd"
        | "arraySub" | "arrayMix" | "fillNoise2D" | "fillNoise3D" | "stencil2D" => 0,
        "arrayMapTo" => 1,
        "curl2" => 2,
        "hsv2rgb" | "rgb2hsv" | "curl3" => 3,
        "canvasSet" => 4,
        "mixColors" => 7,
        _ => return None,
    })
}
/// (builtin name, argument index holding the callback)
fn hof_cb_arg(name: &str) -> Option<usize> {
    Some(match name {
        "arrayForEach" | "arrayMutate" | "arrayReduce" | "arraySortBy" => 1,
        "arrayMapTo" => 2,
        "mapPixels" => 0,
        _ => return None,
    })
}

// ---- per-pattern analysis ----
#[derive(Default, Clone)]
struct Census {
    uses_call_value: bool,
    uses_const_fun: bool,
    hof_builtins: BTreeSet<String>,
    hof_with_fun_cb: bool,
    nonnum_arrstore: bool,
    value_joins: usize,
    kind_joins: usize,
    max_stack: Vec<u32>,
    entries: BTreeSet<String>,
    site_overflow: bool,
    depth_mismatch: bool,
    insns_total: usize,
    insns_seen: usize,
    walk_truncated: bool,
    unreached: BTreeMap<u8, usize>,
}

struct An<'p> {
    prog: &'p Program,
    precise: bool,
    /// Optimistic: do NOT count `array(n)`'s zero-fill as a Num element.
    opt_zerofill: bool,
    globals: Vec<Option<K>>,
    locals: Vec<Vec<Option<K>>>,
    rets: Vec<Option<K>>,
    site_elem: Vec<Option<K>>,
    sites: HashMap<(usize, usize), usize>,
    poison: bool,
    escaped: Set,
    const_fun_fns: Set,
    changed: bool,
    c: Census,
    // per-fn referenced slots + callees
    ref_locals: Vec<BTreeSet<usize>>,
    ref_globals: Vec<BTreeSet<usize>>,
    callees: Vec<BTreeSet<usize>>,
    fn_uses_dyncall: Vec<bool>,
}

impl<'p> An<'p> {
    fn new(prog: &'p Program, precise: bool, opt_zerofill: bool) -> Self {
        let nf = prog.fns.len();
        let mut sites = HashMap::new();
        let mut nsites = 0usize;
        let mut const_fun_fns = Set::default();
        for (fi, f) in prog.fns.iter().enumerate() {
            let code = &prog.words[f.code_start as usize..(f.code_start + f.code_len) as usize];
            let mut at = 0usize;
            while at < code.len() {
                let w = code[at];
                let op = opcode(w);
                let alloc = match op {
                    NEW_ARRAY | CONST_ARR => true,
                    CALL_BUILTIN | CALL_BUILTIN_C | CALL_BUILTIN_CC => {
                        BUILTINS[imm16(w) as usize].name == "array"
                    }
                    _ => false,
                };
                if alloc {
                    sites.insert((fi, at), nsites);
                    nsites += 1;
                }
                if op == CONST_FUN {
                    const_fun_fns = Set::union(const_fun_fns, Set::one(imm16(w) as usize));
                }
                at += ilen(op);
            }
        }
        An {
            prog,
            precise,
            opt_zerofill,
            globals: vec![None; prog.globals.len()],
            locals: prog
                .fns
                .iter()
                .map(|f| vec![None; f.locals as usize])
                .collect(),
            rets: vec![None; nf],
            site_elem: vec![None; nsites],
            sites,
            poison: false,
            escaped: Set::default(),
            const_fun_fns,
            changed: false,
            c: Census::default(),
            ref_locals: vec![BTreeSet::new(); nf],
            ref_globals: vec![BTreeSet::new(); nf],
            callees: vec![BTreeSet::new(); nf],
            fn_uses_dyncall: vec![false; nf],
        }
    }

    fn bump_g(&mut self, g: usize, k: K) {
        let n = match self.globals[g] {
            None => k,
            Some(o) => join(o, k, R_SLOT),
        };
        if self.globals[g] != Some(n) {
            self.globals[g] = Some(n);
            self.changed = true;
        }
    }
    fn bump_l(&mut self, f: usize, i: usize, k: K) {
        if i >= self.locals[f].len() {
            return;
        }
        let n = match self.locals[f][i] {
            None => k,
            Some(o) => join(o, k, R_SLOT),
        };
        if self.locals[f][i] != Some(n) {
            self.locals[f][i] = Some(n);
            self.changed = true;
        }
    }
    /// A slot never written holds `Value::default()` = Num(0) at runtime.
    fn gk(&self, g: usize) -> K {
        self.globals[g].unwrap_or(K::Bot)
    }
    fn lk(&self, f: usize, i: usize) -> K {
        self.locals[f].get(i).copied().flatten().unwrap_or(K::Bot)
    }
    fn bump_ret(&mut self, f: usize, k: K) {
        let n = match self.rets[f] {
            None => k,
            Some(o) => join(o, k, R_SLOT),
        };
        if self.rets[f] != Some(n) {
            self.rets[f] = Some(n);
            self.changed = true;
        }
    }
    fn bump_site(&mut self, s: usize, k: K) {
        if matches!(k, K::Bot) {
            return;
        }
        let n = match self.site_elem[s] {
            None => k,
            Some(o) => join(o, k, R_ELEM),
        };
        if self.site_elem[s] != Some(n) {
            self.site_elem[s] = Some(n);
            self.changed = true;
        }
    }
    fn se(&self, s: usize) -> K {
        self.site_elem[s].unwrap_or(K::Bot)
    }
    fn set_poison(&mut self) {
        if !self.poison {
            self.poison = true;
            self.changed = true;
        }
    }
    fn escape_all(&mut self) {
        let n = Set::union(self.escaped, self.const_fun_fns);
        if n != self.escaped {
            self.escaped = n;
            self.changed = true;
        }
    }
    fn ret_of(&self, f: usize) -> K {
        self.rets[f].unwrap_or(K::Bot)
    }

    /// Element kind read out of an array-typed value.
    fn elem_of(&mut self, a: K) -> K {
        match a {
            K::Arr(s) => {
                if self.poison {
                    return K::Top(R_POISON);
                }
                if s.top || s.empty() {
                    return K::Top(R_TOPARR);
                }
                let mut k: Option<K> = None;
                for i in s.ids() {
                    let e = self.se(i);
                    k = Some(match k {
                        None => e,
                        Some(p) => join(p, e, R_AOA),
                    });
                }
                match k.unwrap() {
                    K::Num => K::Num,
                    K::Top(_) => K::Top(R_AOA),
                    other => other,
                }
            }
            K::Top(_) => K::Top(R_TOPARR),
            K::Bot => K::Bot, // not reached yet
            _ => K::Num,      // indexing a non-array always traps; unreachable value
        }
    }

    fn store_into(&mut self, arr: K, val: K) {
        if matches!(val, K::Num | K::Bot) {
            return;
        }
        match arr {
            K::Arr(s) if !s.top && !s.empty() => {
                for i in s.ids() {
                    self.bump_site(i, val);
                }
            }
            K::Bot => {}
            _ => self.set_poison(),
        }
    }

    /// Resolve a callback value: join modeled args into its params (precise
    /// mode) or mark every function value escaped (the brief's default).
    fn callback(&mut self, cb: K, modeled: &[K]) -> Option<K> {
        if let K::Fun(s) = cb {
            if self.precise && !s.top && !s.empty() {
                let mut r: Option<K> = None;
                for f in s.ids() {
                    let np = self.prog.fns[f].params as usize;
                    for (j, m) in modeled.iter().enumerate().take(np) {
                        self.bump_l(f, j, *m);
                    }
                    let rv = self.ret_of(f);
                    r = Some(match r {
                        None => rv,
                        Some(p) => join(p, rv, R_ARGS),
                    });
                }
                return r;
            }
            self.escape_all();
            return None;
        }
        if matches!(cb, K::Bi) {
            return None;
        }
        self.escape_all();
        None
    }

    fn classify_builtin(&mut self, fi: usize, at: usize, id: u16, args: &[K]) -> K {
        let name = BUILTINS[id as usize].name;
        let a = |i: usize| args.get(i).copied().unwrap_or(K::Bot);
        if let Some(cbi) = hof_cb_arg(name) {
            self.c.hof_builtins.insert(name.to_string());
            self.fn_uses_dyncall[fi] = true;
            if matches!(a(cbi), K::Fun(_) | K::Top(_)) {
                self.c.hof_with_fun_cb = true;
            }
            let modeled: Vec<K> = match name {
                "arrayForEach" | "arrayMutate" => {
                    let e = self.elem_of(a(0));
                    vec![e, K::Num, a(0)]
                }
                "arrayMapTo" => {
                    let e = self.elem_of(a(0));
                    vec![e, K::Num, a(0)]
                }
                "arrayReduce" => {
                    let e = self.elem_of(a(0));
                    vec![K::Top(R_BIUNKNOWN), e, K::Num, a(0)]
                }
                "arraySortBy" => {
                    let e = self.elem_of(a(0));
                    vec![e, e]
                }
                _ => vec![K::Num, K::Num, K::Num, K::Num], // mapPixels
            };
            let r = self.callback(a(cbi), &modeled);
            let cbret = r.unwrap_or(K::Top(R_CBPARAM));
            match name {
                "arrayMutate" => {
                    self.store_into(a(0), cbret);
                }
                "arrayMapTo" => {
                    self.store_into(a(1), cbret);
                }
                "arrayReduce" => {
                    let init = a(2);
                    return match r {
                        Some(rv) => join(rv, init, R_JOIN),
                        None => K::Top(R_BIUNKNOWN),
                    };
                }
                _ => {}
            }
        }
        match name {
            // `array(n)` zero-fills, so its elements start out Num.
            "array" => {
                let s = self.sites[&(fi, at)];
                if !self.opt_zerofill {
                    self.bump_site(s, K::Num);
                }
                K::Arr(Set::one(s))
            }
            "arrayReplace" | "arrayReplaceAt" => {
                let mut v = K::Bot;
                for x in args.iter().skip(1) {
                    v = join(v, *x, R_ARGS);
                }
                self.store_into(a(0), v);
                a(0)
            }
            "canvasSet" => {
                self.store_into(a(0), a(4));
                a(4)
            }
            _ => match passthrough_arg(name) {
                Some(i) => a(i),
                None => K::Num,
            },
        }
    }

    fn analyze_fn(&mut self, fi: usize) {
        let f = &self.prog.fns[fi];
        let (cs, cl) = (f.code_start as usize, f.code_len as usize);
        let code: Vec<u32> = self.prog.words[cs..cs + cl].to_vec();
        let nparams = f.params as usize;
        // escaped params: a function reached only through a callback or a
        // call-value has no visible call sites, so its params are Unknown.
        if self.escaped.top || (fi < 128 && (self.escaped.bits >> fi) & 1 == 1) {
            for j in 0..nparams {
                self.bump_l(fi, j, K::Top(R_CBPARAM));
            }
        }
        let mut states: HashMap<usize, Vec<K>> = HashMap::new();
        let mut preds: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        let mut vjoins: BTreeSet<usize> = BTreeSet::new();
        let mut kjoins: BTreeSet<usize> = BTreeSet::new();
        states.insert(0, Vec::new());
        let mut work = vec![0usize];
        let mut maxd = 0u32;
        let mut guard = 0usize;
        while let Some(at) = work.pop() {
            guard += 1;
            if guard > 400_000 {
                break;
            }
            if at >= code.len() {
                continue;
            }
            let mut st = states[&at].clone();
            maxd = maxd.max(st.len() as u32);
            let w = code[at];
            let op = opcode(w);
            let next = at + ilen(op);
            let mut succ: Vec<(usize, Vec<K>)> = Vec::new();
            macro_rules! pop {
                () => {
                    st.pop().unwrap_or(K::Bot)
                };
            }
            macro_rules! push {
                ($v:expr) => {
                    st.push($v)
                };
            }
            match op {
                CONST_NUM => push!(K::Num),
                CONST_FUN => {
                    self.c.uses_const_fun = true;
                    push!(K::Fun(Set::one(imm16(w) as usize)));
                }
                CONST_BUILTIN => push!(K::Bi),
                LOAD_G => {
                    let g = imm16(w) as usize;
                    self.ref_globals[fi].insert(g);
                    push!(self.gk(g));
                }
                STORE_G => {
                    let g = imm16(w) as usize;
                    self.ref_globals[fi].insert(g);
                    let v = *st.last().unwrap_or(&K::Bot);
                    self.bump_g(g, v);
                }
                STORE_G_POP => {
                    let g = imm16(w) as usize;
                    self.ref_globals[fi].insert(g);
                    let v = pop!();
                    self.bump_g(g, v);
                }
                LOAD_L => {
                    let i = imm8(w) as usize;
                    self.ref_locals[fi].insert(i);
                    push!(self.lk(fi, i));
                }
                STORE_L => {
                    let i = imm8(w) as usize;
                    self.ref_locals[fi].insert(i);
                    let v = *st.last().unwrap_or(&K::Bot);
                    self.bump_l(fi, i, v);
                }
                STORE_L_POP => {
                    let i = imm8(w) as usize;
                    self.ref_locals[fi].insert(i);
                    let v = pop!();
                    self.bump_l(fi, i, v);
                }
                LOAD_IDX => {
                    let _i = pop!();
                    let a = pop!();
                    let e = self.elem_of(a);
                    push!(e);
                }
                STORE_IDX => {
                    let v = pop!();
                    let _i = pop!();
                    let a = pop!();
                    if matches!(v, K::Arr(_) | K::Fun(_) | K::Bi) {
                        self.c.nonnum_arrstore = true;
                    }
                    self.store_into(a, v);
                    push!(v);
                }
                ARR_LEN => {
                    let _ = pop!();
                    push!(K::Num);
                }
                NEW_ARRAY => {
                    let n = imm16(w) as usize;
                    let mut e = if n == 0 { K::Num } else { K::Bot };
                    for _ in 0..n {
                        e = join(e, pop!(), R_ELEM);
                    }
                    let s = self.sites[&(fi, at)];
                    self.bump_site(s, e);
                    push!(K::Arr(Set::one(s)));
                }
                // const-pool arrays are all-numeric by construction
                CONST_ARR => {
                    let s = self.sites[&(fi, at)];
                    self.bump_site(s, K::Num);
                    push!(K::Arr(Set::one(s)));
                }
                DUP => {
                    let v = *st.last().unwrap_or(&K::Bot);
                    push!(v);
                }
                DUP2 => {
                    let n = st.len();
                    let a = if n >= 2 { st[n - 2] } else { K::Bot };
                    let b = if n >= 1 { st[n - 1] } else { K::Bot };
                    push!(a);
                    push!(b);
                }
                POP => {
                    let _ = pop!();
                }
                ADD..=POW_ | BIT_AND..=SHR | LT..=NE => {
                    let _ = pop!();
                    let _ = pop!();
                    push!(K::Num);
                }
                NEG | NOT | BIT_NOT => {
                    let _ = pop!();
                    push!(K::Num);
                }
                ASSERT => {
                    let _ = pop!();
                }
                JMP => {
                    succ.push((imm24(w) as usize, st.clone()));
                }
                JMP_IF_FALSE => {
                    let _ = pop!();
                    succ.push((imm24(w) as usize, st.clone()));
                    succ.push((next, st.clone()));
                }
                JMP_IF_TRUE_PEEK | JMP_IF_FALSE_PEEK => {
                    succ.push((imm24(w) as usize, st.clone()));
                    succ.push((next, st.clone()));
                }
                CMP_JF => {
                    let _ = pop!();
                    let _ = pop!();
                    let t = code.get(at + 1).copied().unwrap_or(0) as usize;
                    succ.push((t, st.clone()));
                    succ.push((next, st.clone()));
                }
                CALL_FN => {
                    let f2 = imm16(w) as usize;
                    let argc = argcf(w) as usize;
                    self.callees[fi].insert(f2);
                    let mut args = vec![K::Num; argc];
                    for j in (0..argc).rev() {
                        args[j] = pop!();
                    }
                    let np = self.prog.fns[f2].params as usize;
                    for (j, a) in args.iter().enumerate().take(np) {
                        self.bump_l(f2, j, *a);
                    }
                    push!(self.ret_of(f2));
                }
                CALL_BUILTIN | CALL_BUILTIN_C | CALL_BUILTIN_CC => {
                    let b = imm16(w);
                    let argc = argcf(w) as usize;
                    let nconst = match op {
                        CALL_BUILTIN_C => 1,
                        CALL_BUILTIN_CC => 2,
                        _ => 0,
                    };
                    let from_stack = argc.saturating_sub(nconst);
                    let mut args = vec![K::Num; argc];
                    for j in (0..from_stack).rev() {
                        args[j] = pop!();
                    }
                    let r = self.classify_builtin(fi, at, b, &args);
                    push!(r);
                }
                CALL_VALUE => {
                    self.c.uses_call_value = true;
                    self.fn_uses_dyncall[fi] = true;
                    let argc = imm8(w) as usize;
                    let mut args = vec![K::Num; argc];
                    for j in (0..argc).rev() {
                        args[j] = pop!();
                    }
                    let callee = pop!();
                    let r = self.callback(callee, &args);
                    push!(r.unwrap_or(K::Top(R_CALLVALUE)));
                }
                RET => {
                    let v = pop!();
                    self.bump_ret(fi, v);
                }
                RET_NULL => {
                    self.bump_ret(fi, K::Num);
                }
                POP_RET_NULL => {
                    let _ = pop!();
                    self.bump_ret(fi, K::Num);
                }
                LOAD_LL => {
                    let a = imm8(w) as usize;
                    let b = imm8b(w) as usize;
                    self.ref_locals[fi].insert(a);
                    self.ref_locals[fi].insert(b);
                    push!(self.lk(fi, a));
                    push!(self.lk(fi, b));
                }
                LOAD_LG => {
                    let a = imm8(w) as usize;
                    let g = imm16hi(w) as usize;
                    self.ref_locals[fi].insert(a);
                    self.ref_globals[fi].insert(g);
                    push!(self.lk(fi, a));
                    push!(self.gk(g));
                }
                LOAD_GL => {
                    let g = imm16(w) as usize;
                    let a = argcf(w) as usize;
                    self.ref_locals[fi].insert(a);
                    self.ref_globals[fi].insert(g);
                    push!(self.gk(g));
                    push!(self.lk(fi, a));
                }
                LOAD_L_IDX => {
                    let a = imm8(w) as usize;
                    self.ref_locals[fi].insert(a);
                    let arr = pop!();
                    let e = self.elem_of(arr);
                    push!(e);
                }
                LOAD_G_L_IDX => {
                    let g = imm16(w) as usize;
                    let a = argcf(w) as usize;
                    self.ref_locals[fi].insert(a);
                    self.ref_globals[fi].insert(g);
                    let gv = self.gk(g);
                    let e = self.elem_of(gv);
                    push!(e);
                }
                CONST_OP => {
                    let _ = pop!();
                    push!(K::Num);
                }
                LOAD_L_CONST_OP => {
                    let a = imm8(w) as usize;
                    self.ref_locals[fi].insert(a);
                    push!(K::Num);
                }
                LOAD_G_CONST_OP => {
                    let g = imm16(w) as usize;
                    self.ref_globals[fi].insert(g);
                    push!(K::Num);
                }
                _ => {}
            }
            let terminal = matches!(op, JMP | RET | RET_NULL | POP_RET_NULL);
            if succ.is_empty() && !terminal {
                succ.push((next, st.clone()));
            }
            maxd = maxd.max(st.len() as u32);
            for (t, s) in succ {
                if t >= code.len() {
                    continue;
                }
                preds.entry(t).or_default().insert(at);
                let isjoin = preds[&t].len() > 1;
                match states.get(&t) {
                    None => {
                        states.insert(t, s);
                        work.push(t);
                    }
                    Some(old) => {
                        if old.len() != s.len() {
                            self.c.depth_mismatch = true;
                            continue;
                        }
                        if isjoin && !s.is_empty() {
                            vjoins.insert(t);
                        }
                        let mut merged = Vec::with_capacity(s.len());
                        let mut diff = false;
                        for (i, k) in s.iter().enumerate() {
                            if old[i] != *k
                                && !matches!(old[i], K::Top(_))
                                && !matches!(k, K::Top(_))
                            {
                                diff = true;
                            }
                            merged.push(join(old[i], *k, R_JOIN));
                        }
                        if diff {
                            kjoins.insert(t);
                        }
                        if merged != *old {
                            states.insert(t, merged);
                            work.push(t);
                        }
                    }
                }
            }
        }
        // self-check: how much of the function's code the walk actually
        // reached (unreached code would silently under-approximate)
        {
            let mut at = 0usize;
            let mut tot = 0usize;
            let mut hit = 0usize;
            while at < code.len() {
                tot += 1;
                if states.contains_key(&at) {
                    hit += 1;
                } else {
                    *self.c.unreached.entry(opcode(code[at])).or_default() += 1;
                }
                at += ilen(opcode(code[at]));
            }
            self.c.insns_total += tot;
            self.c.insns_seen += hit;
            if guard > 400_000 {
                self.c.walk_truncated = true;
            }
        }
        self.c.value_joins += vjoins.len();
        self.c.kind_joins += kjoins.len();
        while self.c.max_stack.len() <= fi {
            self.c.max_stack.push(0);
        }
        self.c.max_stack[fi] = self.c.max_stack[fi].max(maxd);
    }

    fn run(&mut self) {
        if !self.precise {
            self.escape_all();
        }
        for _ in 0..80 {
            self.changed = false;
            // join counters are per-round (analyze_fn re-walks every round)
            self.c.value_joins = 0;
            self.c.insns_total = 0;
            self.c.insns_seen = 0;
            self.c.unreached.clear();
            self.c.kind_joins = 0;
            for fi in 0..self.prog.fns.len() {
                self.analyze_fn(fi);
            }
            if !self.changed {
                break;
            }
        }
    }
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
    value_joins: usize,
    kind_joins: usize,
    locals_num: usize,
    locals_tot: usize,
    g_num: usize,
    g_arrnum: usize,
    g_arr: usize,
    g_other: usize,
    g_top: usize,
    g_tot: usize,
    gu_num: usize,
    gu_top: usize,
    gu_tot: usize,
    poison: bool,
    rp_fns: usize,
    rp_slots: usize,
    rp_unknown: usize,
    rp_locals: usize,
    rp_locals_num: usize,
    ref_locals_tot: usize,
    ref_locals_num: usize,
    fn_stacks: Vec<u32>,
    fully_typed: bool,
    reasons: Vec<String>,
    slot_reasons: Vec<String>,
    // precise-mode variant
    p_fully_typed: bool,
    p_rp_unknown: usize,
    z_fully_typed: bool,
    z_rp_unknown: usize,
    z_arrnum: usize,
    p_arrnum: usize,
    insns_total: usize,
    insns_seen: usize,
    truncated: bool,
    unreached: Vec<(u8, usize)>,
    p_locals_num: usize,
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

    let entry_names = ["render", "render2D", "render3D", "renderFrame"];
    for n in entry_names {
        if prog.exported_fn(n).is_some() {
            r.entries.push(n.to_string());
        }
    }
    r.renderframe = r.entries.iter().any(|e| e == "renderFrame");

    for (precise, zf, rec) in [
        (false, false, 0usize),
        (true, false, 1usize),
        (false, true, 2usize),
    ] {
        let mut an = An::new(&prog, precise, zf);
        an.run();
        // render-path closure
        let mut roots: BTreeSet<usize> = BTreeSet::new();
        for n in entry_names {
            if let Some(f) = prog.exported_fn(n) {
                roots.insert(f as usize);
            }
            // PB also dispatches through a GLOBAL of that name holding a fn
            if let Some(g) = prog.global_index(n) {
                if let K::Fun(s) = an.gk(g as usize) {
                    if s.top {
                        for f in 0..prog.fns.len() {
                            roots.insert(f);
                        }
                    } else {
                        for f in s.ids() {
                            roots.insert(f);
                        }
                    }
                }
            }
        }
        let mut seen = roots.clone();
        let mut stack: Vec<usize> = roots.iter().copied().collect();
        let mut dyn_on_path = false;
        while let Some(f) = stack.pop() {
            if an.fn_uses_dyncall[f] {
                dyn_on_path = true;
            }
            for &c in &an.callees[f] {
                if seen.insert(c) {
                    stack.push(c);
                }
            }
        }
        if dyn_on_path {
            // conservative: any function value could be the callee
            let cf = an.const_fun_fns;
            if cf.top {
                for f in 0..prog.fns.len() {
                    seen.insert(f);
                }
            } else {
                for f in cf.ids() {
                    if seen.insert(f) {}
                }
            }
            // re-close
            let mut stack: Vec<usize> = seen.iter().copied().collect();
            while let Some(f) = stack.pop() {
                for &c in &an.callees[f] {
                    if seen.insert(c) {
                        stack.push(c);
                    }
                }
            }
        }
        let mut unknown = 0usize;
        let mut slots = 0usize;
        let mut rpl = 0usize;
        let mut rpl_num = 0usize;
        let mut reasons: BTreeSet<String> = BTreeSet::new();
        let mut slot_reasons: Vec<String> = Vec::new();
        for &f in &seen {
            for &i in &an.ref_locals[f] {
                if i >= an.locals[f].len() {
                    continue;
                }
                slots += 1;
                rpl += 1;
                if matches!(an.lk(f, i), K::Num | K::Bot) {
                    rpl_num += 1;
                }
                if let K::Top(why) = an.lk(f, i) {
                    unknown += 1;
                    reasons.insert(why.to_string());
                    slot_reasons.push(why.to_string());
                }
            }
            for &g in &an.ref_globals[f] {
                slots += 1;
                if let K::Top(why) = an.gk(g) {
                    unknown += 1;
                    reasons.insert(why.to_string());
                    slot_reasons.push(why.to_string());
                }
            }
        }
        let locals_num: usize = (0..prog.fns.len())
            .map(|f| {
                (0..an.locals[f].len())
                    .filter(|&i| matches!(an.lk(f, i), K::Num | K::Bot))
                    .count()
            })
            .sum();
        if rec == 0 {
            r.uses_call_value = an.c.uses_call_value;
            r.uses_const_fun = an.c.uses_const_fun;
            r.hofs = an.c.hof_builtins.iter().cloned().collect();
            r.hof_fun_cb = an.c.hof_with_fun_cb;
            r.nonnum_arrstore = an.c.nonnum_arrstore;
            r.value_joins = an.c.value_joins;
            r.kind_joins = an.c.kind_joins;
            r.max_stack = an.c.max_stack.iter().copied().max().unwrap_or(0);
            r.locals_num = locals_num;
            r.locals_tot = r.n_locals;
            r.g_tot = prog.globals.len();
            for gi in 0..prog.globals.len() {
                let pre = prog.globals[gi].predefined;
                if !pre {
                    r.gu_tot += 1;
                }
                match an.gk(gi) {
                    K::Num | K::Bot => {
                        r.g_num += 1;
                        if !pre {
                            r.gu_num += 1;
                        }
                    }
                    K::Arr(s) => {
                        let arrnum = !an.poison
                            && !s.top
                            && !s.empty()
                            && s.ids().iter().all(|&i| matches!(an.se(i), K::Num | K::Bot));
                        if arrnum {
                            r.g_arrnum += 1
                        } else {
                            r.g_arr += 1
                        }
                    }
                    K::Top(_) => {
                        r.g_top += 1;
                        if !pre {
                            r.gu_top += 1;
                        }
                    }
                    _ => r.g_other += 1,
                }
            }
            r.poison = an.poison;
            r.rp_fns = seen.len();
            r.rp_slots = slots;
            r.rp_unknown = unknown;
            r.rp_locals = rpl;
            r.rp_locals_num = rpl_num;
            r.ref_locals_tot = (0..prog.fns.len()).map(|f| an.ref_locals[f].len()).sum();
            r.ref_locals_num = (0..prog.fns.len())
                .map(|f| {
                    an.ref_locals[f]
                        .iter()
                        .filter(|&&i| matches!(an.lk(f, i), K::Num | K::Bot))
                        .count()
                })
                .sum();
            r.fn_stacks = an.c.max_stack.clone();
            r.insns_total = an.c.insns_total;
            r.insns_seen = an.c.insns_seen;
            r.truncated = an.c.walk_truncated || an.c.depth_mismatch;
            r.unreached = an.c.unreached.iter().map(|(&k, &v)| (k, v)).collect();
            r.fully_typed = unknown == 0;
            r.reasons = reasons.into_iter().collect();
            r.slot_reasons = slot_reasons;
        } else if rec == 1 {
            r.p_fully_typed = unknown == 0;
            r.p_rp_unknown = unknown;
            r.p_locals_num = locals_num;
            r.p_arrnum = (0..prog.globals.len())
                .filter(|&g| match an.gk(g) {
                    K::Arr(s) => {
                        !an.poison
                            && !s.top
                            && !s.empty()
                            && s.ids().iter().all(|&i| matches!(an.se(i), K::Num | K::Bot))
                    }
                    _ => false,
                })
                .count();
        } else {
            r.z_fully_typed = unknown == 0;
            r.z_rp_unknown = unknown;
            r.z_arrnum = (0..prog.globals.len())
                .filter(|&g| match an.gk(g) {
                    K::Arr(s) => {
                        !an.poison
                            && !s.top
                            && !s.empty()
                            && s.ids().iter().all(|&i| matches!(an.se(i), K::Num | K::Bot))
                    }
                    _ => false,
                })
                .count();
        }
    }
    r
}

/// Print every Unknown slot on one pattern's render path, named.
fn explain(path: &str) {
    let src = std::fs::read_to_string(path).unwrap();
    let prog = luxel_core::compile::compile(&src).unwrap();
    let mut an = An::new(&prog, false, false);
    an.run();
    let entry_names = ["render", "render2D", "render3D", "renderFrame"];
    let mut roots: BTreeSet<usize> = BTreeSet::new();
    for n in entry_names {
        if let Some(f) = prog.exported_fn(n) {
            roots.insert(f as usize);
        }
        if let Some(g) = prog.global_index(n) {
            if let K::Fun(s) = an.gk(g as usize) {
                for f in s.ids() {
                    roots.insert(f);
                }
            }
        }
    }
    let mut seen = roots.clone();
    let mut stack: Vec<usize> = roots.iter().copied().collect();
    let mut dynp = false;
    while let Some(f) = stack.pop() {
        if an.fn_uses_dyncall[f] {
            dynp = true;
        }
        for &c in &an.callees[f] {
            if seen.insert(c) {
                stack.push(c);
            }
        }
    }
    if dynp {
        for f in an.const_fun_fns.ids() {
            seen.insert(f);
        }
    }
    println!(
        "{path}: render-path fns {:?}",
        seen.iter()
            .map(|&f| prog.fns[f].name.clone())
            .collect::<Vec<_>>()
    );
    for &f in &seen {
        for &i in &an.ref_locals[f] {
            let k = an.lk(f, i);
            let nm = prog.fns[f].local_names.get(i).cloned().unwrap_or_default();
            if let K::Top(w) = k {
                println!("  UNKNOWN local {}:{} ({}) -- {w}", prog.fns[f].name, nm, i);
            }
        }
        for &g in &an.ref_globals[f] {
            if let K::Top(w) = an.gk(g) {
                println!("  UNKNOWN global {} -- {w}", prog.globals[g].name);
            }
        }
    }
    for g in 0..prog.globals.len() {
        if prog.globals[g].predefined {
            continue;
        }
        let d = match an.gk(g) {
            K::Bot => "Num (never written)".to_string(),
            K::Num => "Num".to_string(),
            K::Arr(s) => {
                let arrnum = !an.poison
                    && !s.top
                    && s.ids().iter().all(|&i| matches!(an.se(i), K::Num | K::Bot));
                format!(
                    "{} sites={:?} elems={:?}",
                    if arrnum { "ArrNum" } else { "Arr" },
                    s.ids(),
                    s.ids().iter().map(|&i| an.se(i)).collect::<Vec<_>>()
                )
            }
            K::Fun(s) => format!("Fun{:?}", s.ids()),
            K::Bi => "Builtin".to_string(),
            K::Top(w) => format!("UNKNOWN -- {w}"),
        };
        println!("  global {} : {d}", prog.globals[g].name);
    }
    for f in 0..prog.fns.len() {
        println!("  ret {} : {:?}", prog.fns[f].name, an.ret_of(f));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--explain") {
        for p in &args[1..] {
            explain(p);
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
        "  ... with a pattern-fn callback    {}",
        cnt(&|r| r.hof_fun_cb)
    );
    println!(
        "stores a provably non-Num into arr  {}",
        cnt(&|r| r.nonnum_arrstore)
    );
    println!(
        "has >=1 value join (stack non-empty) {}",
        cnt(&|r| r.value_joins > 0)
    );
    println!(
        "has >=1 kind-differing join          {}",
        cnt(&|r| r.kind_joins > 0)
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
    println!("HOF builtin usage: {:?}", hofhist);
    let mut ehist: BTreeMap<String, usize> = BTreeMap::new();
    for r in &ok {
        for e in &r.entries {
            *ehist.entry(e.clone()).or_default() += 1;
        }
    }
    println!("entries: {:?}", ehist);

    let stat = |v: &mut Vec<u64>| {
        v.sort();
        (v[0], v[v.len() / 2], v[v.len() - 1], v.iter().sum::<u64>())
    };
    let mut cw: Vec<u64> = ok.iter().map(|r| r.code_words as u64).collect();
    let (a, b, c, d) = stat(&mut cw);
    println!("\ncode words/pattern: min {a} median {b} max {c} total {d}");
    let mut ms: Vec<u64> = ok.iter().map(|r| r.max_stack as u64).collect();
    let (a, b, c, _) = stat(&mut ms);
    println!("max operand-stack depth (per pattern): min {a} median {b} max {c}");
    let mut fs: Vec<u64> = ok
        .iter()
        .flat_map(|r| r.fn_stacks.iter().map(|&x| x as u64))
        .collect();
    let nfn = fs.len();
    let over8 = fs.iter().filter(|&&x| x > 8).count();
    let (a, b, c, _) = stat(&mut fs);
    println!("max operand-stack depth (per function, n={nfn}): min {a} median {b} max {c}; >8 deep: {over8}");
    let mut hist: BTreeMap<u64, usize> = BTreeMap::new();
    for x in &fs {
        *hist.entry(*x).or_default() += 1;
    }
    println!("  per-function depth histogram: {hist:?}");
    let mut nl: Vec<u64> = ok.iter().map(|r| r.n_locals as u64).collect();
    let (a, b, c, d) = stat(&mut nl);
    println!("locals/pattern: min {a} median {b} max {c} total {d}");
    let mut ng: Vec<u64> = ok.iter().map(|r| r.n_globals as u64).collect();
    let (a, b, c, d) = stat(&mut ng);
    println!("globals/pattern: min {a} median {b} max {c} total {d}");
    let mut nf: Vec<u64> = ok.iter().map(|r| r.n_fns as u64).collect();
    let (a, b, c, d) = stat(&mut nf);
    println!("fns/pattern: min {a} median {b} max {c} total {d}");

    println!("\n== kind inference (conservative / brief spec) ==");
    let lt: usize = ok.iter().map(|r| r.locals_tot).sum();
    let ln: usize = ok.iter().map(|r| r.locals_num).sum();
    println!(
        "locals proven Num: {ln}/{lt} = {:.1}%",
        100.0 * ln as f64 / lt as f64
    );
    let plt: usize = ok.iter().map(|r| r.p_locals_num).sum();
    println!(
        "  (HOF-precise variant: {plt}/{lt} = {:.1}%)",
        100.0 * plt as f64 / lt as f64
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
    let gtop: usize = ok.iter().map(|r| r.g_top).sum();
    println!("globals: {gt} total | Num {gn} ({:.1}%) | ArrNum {gan} | Arr {ga} | Fun/Builtin {go} | Unknown {gtop}",
        100.0*gn as f64/gt as f64);
    let gut: usize = ok.iter().map(|r| r.gu_tot).sum();
    let gun: usize = ok.iter().map(|r| r.gu_num).sum();
    let gutop: usize = ok.iter().map(|r| r.gu_top).sum();
    println!("  user globals only (predefined excluded): {gut} | Num {gun} ({:.1}%) | ArrNum {gan} | Arr {ga} | Fun/Builtin {go} | Unknown {gutop}",
        100.0*gun as f64/gut as f64);
    println!(
        "  patterns with the array pool poisoned: {}",
        cnt(&|r| r.poison)
    );
    println!(
        "fully typed render path: {} / {n}",
        cnt(&|r| r.fully_typed && !r.entries.is_empty())
    );
    println!(
        "  (HOF-precise variant:  {} / {n})",
        cnt(&|r| r.p_fully_typed && !r.entries.is_empty())
    );
    println!(
        "  (optimistic-zero-fill variant: {} / {n}; ArrNum globals {} vs {gan})",
        cnt(&|r| r.z_fully_typed && !r.entries.is_empty()),
        ok.iter().map(|r| r.z_arrnum).sum::<usize>()
    );
    println!(
        "  (ArrNum globals under HOF-precise: {})",
        ok.iter().map(|r| r.p_arrnum).sum::<usize>()
    );
    println!(
        "any Unknown slot on render path: {}",
        cnt(&|r| !r.fully_typed)
    );
    let mut rh: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for r in &ok {
        for why in &r.reasons {
            let e = rh.entry(why.clone()).or_default();
            e.0 += 1;
            if e.1.len() < 3 {
                e.1.push(r.file.rsplit('/').next().unwrap().to_string());
            }
        }
    }
    let mut rv: Vec<_> = rh.into_iter().collect();
    rv.sort_by_key(|(_, (c, _))| std::cmp::Reverse(*c));
    println!("\n== top Unknown reasons on the render path ==");
    for (why, (c, ex)) in rv.iter().take(12) {
        println!("{c:4} patterns  {why}  e.g. {}", ex.join(", "));
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
        "-- same, counted per UNKNOWN SLOT (n={}) --",
        sv.iter().map(|(_, c)| c).sum::<usize>()
    );
    for (why, c) in sv.iter().take(12) {
        println!("{c:4} slots  {why}");
    }

    println!("\n== first-JIT exclusion (bail on CallValue or HOF-with-fn-callback) ==");
    let excl = |r: &&Rec| r.uses_call_value || r.hof_fun_cb;
    let e = cnt(&excl);
    println!("excluded: {e} / {n} ({:.1}%)", 100.0 * e as f64 / n as f64);
    let rf = cnt(&|r| r.renderframe);
    let rfe = cnt(&|r| r.renderframe && excl(r));
    let nrf = n - rf;
    let nrfe = e - rfe;
    println!(
        "  renderFrame patterns:  {rfe}/{rf} excluded ({:.1}%)",
        100.0 * rfe as f64 / rf.max(1) as f64
    );
    println!(
        "  non-renderFrame:       {nrfe}/{nrf} excluded ({:.1}%)",
        100.0 * nrfe as f64 / nrf.max(1) as f64
    );
    let e2 = cnt(&|r| r.uses_call_value || !r.hofs.is_empty());
    println!(
        "if ANY HOF call bails (even a builtin callback): {e2} / {n} ({:.1}%)",
        100.0 * e2 as f64 / n as f64
    );
    // fully typed AND not excluded
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
        "\nUNKNOWN-on-render-path patterns: {:?}",
        ok.iter()
            .filter(|r| !r.fully_typed)
            .map(|r| r.file.rsplit('/').next().unwrap())
            .collect::<Vec<_>>()
    );
    println!(
        "\nrenderFrame patterns that ARE excluded: {:?}",
        ok.iter()
            .filter(|r| r.renderframe && excl(r))
            .map(|r| r.file.rsplit('/').next().unwrap())
            .collect::<Vec<_>>()
    );
    // self-checks
    let it: usize = ok.iter().map(|r| r.insns_total).sum();
    let is_: usize = ok.iter().map(|r| r.insns_seen).sum();
    println!("\nself-check: abstract walk reached {is_}/{it} instructions ({:.2}%); truncated/depth-mismatch patterns: {}",
        100.0*is_ as f64/it as f64, cnt(&|r| r.truncated));
    let mut ur: BTreeMap<u8, usize> = BTreeMap::new();
    for r in &ok {
        for (o, n) in &r.unreached {
            *ur.entry(*o).or_default() += n;
        }
    }
    println!(
        "  unreached opcodes (hex): {:?}",
        ur.iter()
            .map(|(o, n)| (format!("{o:#04x}"), *n))
            .collect::<Vec<_>>()
    );
}

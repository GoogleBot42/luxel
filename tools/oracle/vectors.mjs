// Differential test battery. Each vector becomes
//   <setup with $ → _name>
//   export var o_<name> = <code>
// run on both the real Pixel Blaze and the Luxel engine, comparing exact
// 16.16 raws. Time- and random-dependent things don't belong here (except
// the seeded prng, which goes in SPECIALS to capture the sequence).

export const VECTORS = [
  // --- division / modulo by zero (runtime, via variable to defeat folding)
  { name: "div0", setup: "z$ = 0", code: "5 / z$" },
  { name: "div0n", setup: "z$ = 0", code: "-5 / z$" },
  { name: "div00", setup: "z$ = 0", code: "z$ / z$" },
  { name: "rem0", setup: "z$ = 0", code: "5 % z$" },
  { name: "mod0", setup: "z$ = 0", code: "mod(5, z$)" },
  // --- overflow wrap
  { name: "sq181", code: "181 * 181" },
  { name: "sq182", code: "182 * 182" },
  { name: "addmax", code: "32767 + 1" },
  { name: "submin", setup: "m$ = 0 - 32768", code: "m$ - 1" },
  { name: "mul200", code: "200 * 200" },
  { name: "divwrap", setup: "m$ = 0 - 32768\no$ = -1", code: "m$ / o$" },
  // --- bitwise on the full word
  { name: "or0", code: "1.5 | 0" },
  { name: "shl1", code: "1.25 << 1" },
  { name: "shr1", code: "2.5 >> 1" },
  { name: "shrneg", setup: "n$ = -2.5", code: "n$ >> 1" },
  { name: "shrneg31", setup: "n$ = -1", code: "n$ >> 31" },
  { name: "not15", code: "~1.5" },
  { name: "not0", code: "~0" },
  { name: "and53", code: "5 & 3" },
  { name: "xor53", code: "5 ^ 3" },
  { name: "andfrac", code: "1.5 & 1.25" },
  // --- shift counts: ≥32, negative, fractional (all via variables)
  { name: "shl32", setup: "c$ = 32", code: "1 << c$" },
  { name: "shl33", setup: "c$ = 33", code: "1 << c$" },
  { name: "shlneg", setup: "c$ = -1", code: "1 << c$" },
  { name: "shlfrac", setup: "c$ = 1.5", code: "1 << c$" },
  { name: "shr32", setup: "c$ = 32", code: "65 >> c$" },
  // --- rounding family
  { name: "floorneg", code: "floor(-5.1)" },
  { name: "ceilneg", code: "ceil(-5.9)" },
  { name: "fracneg", code: "frac(-5.5)" },
  { name: "truncneg", code: "trunc(-5.9)" },
  { name: "round25", code: "round(2.5)" },
  { name: "roundn25", code: "round(-2.5)" },
  { name: "round05", code: "round(0.5)" },
  { name: "roundn05", code: "round(-0.5)" },
  // --- % vs mod()
  { name: "remneg", code: "-3.5 % 3" },
  { name: "modneg", code: "mod(-3.5, 3)" },
  { name: "remposneg", code: "3.5 % -3" },
  { name: "modposneg", code: "mod(3.5, -3)" },
  // --- logic values
  { name: "or42", code: "0 || 42" },
  { name: "and7", code: "3 && 7" },
  { name: "or2", code: "2 || 7" },
  { name: "not5", code: "!5" },
  { name: "truesum", code: "true + true" },
  { name: "cmppack", code: "(1 < 2) + (2 <= 1) * 10 + (3 == 3) * 100" },
  { name: "tern", code: "0 ? 10 : 20" },
  // --- precision / literals
  { name: "tinymul", setup: "t$ = 0.001", code: "t$ * t$" },
  // 31-bit literal probe: raw 1 has the LSB set; if the compiler drops it
  // (16.15 literals) this is 0, if kept it's 1.0
  { name: "lit_epsilon", setup: "e$ = 0.0000152587890625", code: "e$ << 16" },
  { name: "multrunc", setup: "a$ = 0.7\nb$ = 0.00005", code: "(a$ * b$) << 16" },
  // --- transcendentals (exact raw comparison — expect small diffs, record them)
  { name: "sin1", code: "sin(1)" },
  { name: "sinpi", code: "sin(PI)" },
  { name: "sinhalfpi", code: "sin(PI / 2)" },
  { name: "sin100", code: "sin(100)" },
  { name: "sinneg", setup: "n$ = -1", code: "sin(n$)" },
  { name: "cos1", code: "cos(1)" },
  { name: "tan1", code: "tan(1)" },
  { name: "sqrt2", code: "sqrt(2)" },
  { name: "sqrthalf", code: "sqrt(0.5)" },
  { name: "sqrtbig", code: "sqrt(30000)" },
  { name: "sqrtneg", setup: "n$ = -4", code: "sqrt(n$)" },
  { name: "exp1", code: "exp(1)" },
  { name: "exp10", code: "exp(10)" },
  { name: "log2_8", code: "log2(8)" },
  { name: "loghalf", code: "log(0.5)" },
  { name: "log0", setup: "z$ = 0", code: "log(z$)" },
  { name: "logneg", setup: "n$ = -1", code: "log(n$)" },
  { name: "pow_2_10", code: "pow(2, 10)" },
  { name: "pow_9_half", code: "pow(9, 0.5)" },
  { name: "pow_neg2_2", setup: "n$ = -2", code: "pow(n$, 2)" },
  { name: "pow_neg2_3", setup: "n$ = -2", code: "pow(n$, 3)" },
  { name: "pow_x_0", code: "pow(5, 0)" },
  { name: "pow_0_0", setup: "z$ = 0", code: "pow(z$, z$)" },
  { name: "atan1", code: "atan(1)" },
  { name: "atan100", code: "atan(100)" },
  { name: "atan2_11", code: "atan2(1, 1)" },
  { name: "atan2_1n1", setup: "n$ = -1", code: "atan2(1, n$)" },
  { name: "atan2_n1n1", setup: "n$ = -1", code: "atan2(n$, n$)" },
  { name: "atan2_00", setup: "z$ = 0", code: "atan2(z$, z$)" },
  { name: "asin_half", code: "asin(0.5)" },
  { name: "acos_half", code: "acos(0.5)" },
  { name: "hypot34", code: "hypot(3, 4)" },
  { name: "hypotbig", code: "hypot(200, 200)" },
  { name: "hypot3_122", code: "hypot3(1, 2, 2)" },
  // --- waveforms
  { name: "wave25", code: "wave(0.25)" },
  { name: "wave125", code: "wave(0.125)" },
  { name: "wave75", code: "wave(0.75)" },
  { name: "waveneg", setup: "n$ = -0.25", code: "wave(n$)" },
  { name: "tri25", code: "triangle(0.25)" },
  { name: "tri6", code: "triangle(0.6)" },
  { name: "trineg", setup: "n$ = -0.25", code: "triangle(n$)" },
  { name: "square2", code: "square(0.2, 0.5)" },
  { name: "square5", code: "square(0.5, 0.5)" },
  { name: "square_1arg", code: "square(0.2)" },
  { name: "mix1020", code: "mix(10, 20, 0.5)" },
  { name: "smooth5", code: "smoothstep(0, 1, 0.5)" },
  { name: "smooth25", code: "smoothstep(0, 1, 0.25)" },
  { name: "bezq", code: "bezierQuadratic(0.5, 0, 1, 0)" },
  { name: "bezc", code: "bezierCubic(0.5, 0, 1, 1, 0)" },
  { name: "clamp5", code: "clamp(5, 0, 1)" },
  { name: "minmax", code: "min(3, 7) + max(3, 7) * 100" },
  // --- constants
  { name: "cPI", code: "PI" },
  { name: "cPI2", code: "PI2" },
  { name: "cE", code: "E" },
  { name: "cPI3_4", code: "PI3_4" },
  { name: "cPISQ", code: "PISQ" },
  { name: "cSQRT2", code: "SQRT2" },
  { name: "cSQRT1_2", code: "SQRT1_2" },
  { name: "cLN2", code: "LN2" },
  { name: "cLOG2E", code: "LOG2E" },
  // --- arrays
  { name: "oob_read", setup: "a$ = [1, 2, 3]", code: "a$[5]" },
  { name: "oob_write", setup: "a$ = [1, 2, 3]\na$[5] = 9", code: "a$[0] + a$.length" },
  { name: "neg_index", setup: "a$ = [1, 2, 3]", code: "a$[-1]" },
  { name: "frac_index", setup: "a$ = [10, 20, 30]", code: "a$[1.5]" },
  { name: "frac_index_n", setup: "a$ = [10, 20, 30]\ni$ = -0.5", code: "a$[i$]" },
  { name: "arr_sum", setup: "a$ = [1, 2, 3]", code: "a$.sum()" },
  { name: "arr_len", setup: "a$ = array(4)", code: "a$.length" },
  { name: "arr_sort", setup: "a$ = [3, 1, 2]\na$.sort()", code: "a$[0] * 100 + a$[1] * 10 + a$[2]" },
  // --- abort-or-continue sentinels: does the odd op halt execution?
  { name: "sent_fracread", setup: "a$ = [10, 20, 30]\nx$ = a$[1.5]\ns$ = 77", code: "s$" },
  { name: "sent_fracrval", setup: "a$ = [10, 20, 30]\nx$ = a$[1.5]", code: "x$" },
  { name: "sent_negfrac", setup: "a$ = [10, 20, 30]\ni$ = -0.5\nx$ = a$[i$]\ns$ = 82", code: "s$ * 1000 + x$" },
  { name: "sent_oobfrac", setup: "a$ = [10, 20, 30]\nx$ = a$[5.5]\ns$ = 83", code: "s$ * 1000 + x$" },
  { name: "sent_oobint", setup: "a$ = [10, 20, 30]\nx$ = a$[5]\ns$ = 21", code: "s$ * 1000 + x$" },
  { name: "sent_lastidx", setup: "a$ = [10, 20, 30]\nx$ = a$[2]\ns$ = 22", code: "s$ * 1000 + x$" },
  { name: "sent_negread", setup: "a$ = [10, 20, 30]\nx$ = a$[-1]\ns$ = 78", code: "s$" },
  { name: "sent_fracwrite", setup: "a$ = [10, 20, 30]\na$[1.5] = 9\ns$ = 79", code: "s$" },
  { name: "sent_fracwval", setup: "a$ = [10, 20, 30]\na$[1.5] = 9", code: "a$[1]" },
  { name: "sent_oobwrite", setup: "a$ = [1]\na$[5] = 9\ns$ = 80", code: "s$" },
  { name: "sent_negwrite", setup: "a$ = [1]\na$[-1] = 9\ns$ = 81", code: "s$" },
  { name: "sent_fwvar", setup: "a$ = [10, 20, 30]\ni$ = 1.5\na$[i$] = 9\ns$ = 85", code: "s$ * 100 + a$[1]" },
  { name: "sent_fwcomp", setup: "a$ = [10, 20, 30]\ni$ = 1.5\na$[i$] += 9\ns$ = 86", code: "s$ * 100 + a$[1]" },
  { name: "sent_owvar", setup: "a$ = [10, 20, 30]\ni$ = 5\na$[i$] = 9\ns$ = 87", code: "s$ * 100 + a$[0]" },
  { name: "sent_owvarc", setup: "a$ = [10, 20, 30]\ni$ = 5\na$[i$] += 9\ns$ = 88", code: "s$ * 100 + a$[0]" },
  { name: "sent_nwvar", setup: "a$ = [10, 20, 30]\ni$ = -1\na$[i$] = 9\ns$ = 89", code: "s$ * 100 + a$[0]" },
  // --- JS-isms accepted by the PB compiler
  { name: "jnull", code: "null == 0" },
  { name: "jstrict", code: "(1 === 1) + (1 !== 2) * 10" },
  // --- array() size semantics with fractional lengths
  { name: "alen74", setup: "a$ = array(7.4)", code: "a$.length" },
  { name: "alen05", setup: "a$ = array(0.5)", code: "a$.length" },
  { name: "alen76", setup: "a$ = array(7.6)", code: "a$.length" },
  { name: "alen70", setup: "a$ = array(7)", code: "a$.length" },
  { name: "alenrd", setup: "a$ = array(7.4)\nx$ = a$[7]\ns$ = 90", code: "s$" },
  // --- GPIO constants
  { name: "gLOW", code: "LOW" },
  { name: "gHIGH", code: "HIGH" },
  { name: "gINPUT", code: "INPUT" },
  { name: "gOUTPUT", code: "OUTPUT" },
  { name: "gINPUT_PULLUP", code: "INPUT_PULLUP" },
  { name: "gINPUT_PULLDOWN", code: "INPUT_PULLDOWN" },
  { name: "gOUTPUT_OPEN_DRAIN", code: "OUTPUT_OPEN_DRAIN" },
  { name: "gANALOG", code: "ANALOG" },
  // --- inc/dec value semantics
  { name: "postinc", setup: "i$ = 5\nr$ = i$++ * 10 + i$", code: "r$" },
  { name: "preinc", setup: "i$ = 5\nr$ = ++i$ * 10 + i$", code: "r$" },
  { name: "postdec", setup: "i$ = 5\nr$ = i$-- * 10 + i$", code: "r$" },
];

// Whole-program probes whose exported vars are compared key-by-key.
export const SPECIALS = [
  {
    title: "prng-sequence",
    source: `prngSeed(42)
export var o_prng1 = prng(100)
export var o_prng2 = prng(100)
export var o_prng3 = prng(100)
export var o_seedret = prngSeed(7)
export var o_prng4 = prng(100)
export function render(index) { hsv(0, 0, 0) }
`,
    keys: ["o_prng1", "o_prng2", "o_prng3", "o_seedret", "o_prng4"],
  },
  {
    title: "hsv-pixel-write", // engine-level, needs previewFrame — placeholder for later
    skip: true,
  },
];

// Interactive review UI — the page logic. Served by tools/verify/review.mjs.
//
// Every pair runs BOTH sides live on the engine wasm under the sweep's exact
// conventions (same rig, seed 1, pinned wall clock, beat120 sensor feed where
// a side binds sensors), so what you watch here is what the judge judged.
//
// Budget model: engines are expensive, 293 pairs × 2 sides is not survivable,
// so an IntersectionObserver keeps only the visible cards live (capped), a
// single rAF loop round-robins a fixed step budget across them, and scrolled-
// away cards free their engines. Same shape as web/src/components/Gallery.svelte.
//
// SPDX-License-Identifier: Apache-2.0

import { loadEngineHost, cubeLattice, sensorSlots } from "/engine.js";
import { synthSensorFrame } from "/sensormodel.mjs";
import { parseControlHints } from "/hints.mjs";

const WALL_CLOCK = 1756000000; // the sweep's pin (snap.mjs / report.mjs)
const SEED = 1;
const MAX_LIVE_CARDS = 20; // ×2 sides = ~40 live engines
const STEP_BUDGET = 18; // engine steps per animation frame
const GAP_RGB = [16, 16, 20]; // cloud slice separator

// "non-visual" is not a judgement — it is a manifest exclusion (fixups.json
// `nonVisual`, Gitea #123): the ORIGINAL is not a visual pattern, so the pair
// carries no score. It gets a filter chip so those pairs can be found (or
// filtered away) rather than silently vanishing from every chip.
const VERDICTS = ["match", "close", "divergent", "broken", "orig-unrenderable", "non-visual"];
const DECISIONS = ["delete", "good", "fork", "needs-work"];
// Deliberately NOT emoji: the nix chromium in this repo's dev shell ships no
// emoji font, so 🗑/✅/🔀/🔧 render as tofu boxes. These glyphs are in the
// base system-ui coverage everywhere the tool is used.
const DECISION_LABEL = {
  delete: "✕ delete",
  good: "✓ good",
  fork: "⋔ fork",
  "needs-work": "⚙ needs work",
};
// A fix pass stamps `addressedAt` on the decision entries it acted on; the
// stamp survives until the slug is re-decided (the server rewrites the whole
// entry then), so "addressed" = fixed and awaiting Jeremy's re-review. It is
// a virtual chip, not a decision kind — a pair can be both needs-work and
// addressed.
const ADDRESSED = "addressed";
const ADDRESSED_LABEL = "↻ addressed";

// Polled readouts, not inputs — they have no widget position to get wrong.
const READONLY_KINDS = new Set(["showNumber", "gauge"]);

const $ = (id) => document.getElementById(id);
const el = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};

const state = {
  host: null,
  pairs: [],
  cards: [],
  fps: 20,
  paused: false,
  query: "",
  verdictFilter: new Set(),
  decisionFilter: new Set(),
};

// ---- pixel layouts (the report.mjs rigs, as canvas geometry) ---------------

function makePainter(rig) {
  if (rig.kind === "grid") {
    const w = rig.gridW;
    const h = Math.max(1, Math.ceil(rig.pixels / rig.gridW));
    return {
      w,
      h,
      cls: "grid",
      paint(frame, data) {
        for (let p = 0, s = 0, d = 0; p < rig.pixels; p++, s += 3, d += 4) {
          data[d] = frame[s];
          data[d + 1] = frame[s + 1];
          data[d + 2] = frame[s + 2];
          data[d + 3] = 255;
        }
      },
    };
  }
  if (rig.kind === "cloud") {
    const s = rig.cloudSide;
    const w = s * s + (s - 1); // z-slices side by side, 1 px gap between
    const h = s;
    const idx = new Int32Array(rig.pixels);
    for (let p = 0; p < rig.pixels; p++) {
      const x = p % s;
      const y = Math.floor(p / s) % s;
      const z = Math.floor(p / (s * s));
      idx[p] = ((y * w + (z * (s + 1) + x)) * 4) | 0;
    }
    return {
      w,
      h,
      cls: "cloud",
      base(data) {
        for (let i = 0; i < data.length; i += 4) {
          data[i] = GAP_RGB[0];
          data[i + 1] = GAP_RGB[1];
          data[i + 2] = GAP_RGB[2];
          data[i + 3] = 255;
        }
      },
      paint(frame, data) {
        for (let p = 0, si = 0; p < rig.pixels; p++, si += 3) {
          const d = idx[p];
          data[d] = frame[si];
          data[d + 1] = frame[si + 1];
          data[d + 2] = frame[si + 2];
          data[d + 3] = 255;
        }
      },
    };
  }
  return {
    w: rig.pixels,
    h: 1,
    cls: "strip",
    paint(frame, data) {
      for (let p = 0, s = 0, d = 0; p < rig.pixels; p++, s += 3, d += 4) {
        data[d] = frame[s];
        data[d + 1] = frame[s + 1];
        data[d + 2] = frame[s + 2];
        data[d + 3] = 255;
      }
    },
  };
}

// ---- one rendered side ------------------------------------------------------

class Side {
  /** @param opts {label, source, loadError, rig, vars, big, onReady} */
  constructor(opts) {
    this.o = opts;
    this.rig = opts.rig;
    this.painter = makePainter(opts.rig);
    this.engine = null;
    this.acc = 0;
    this.frameIndex = 0;
    this.wants = false;
    this.values = {}; // control name → values[], survives a reset
    this.el = this.build();
  }

  build() {
    const root = el("div", "side");
    const cap = el("div", "cap");
    cap.append(el("span", null, this.o.label));
    this.rigNote = el("span", "mono");
    this.rigNote.style.color = "var(--dim-2)";
    cap.append(this.rigNote);
    root.append(cap);

    const stage = el("div", "stage");
    this.canvas = el("canvas", this.painter.cls);
    this.canvas.width = this.painter.w;
    this.canvas.height = this.painter.h;
    if (this.painter.cls !== "strip")
      this.canvas.style.aspectRatio = `${this.painter.w} / ${this.painter.h}`;
    this.ctx = this.canvas.getContext("2d", { alpha: false });
    this.img = this.ctx.createImageData(this.painter.w, this.painter.h);
    this.painter.base?.(this.img.data);
    stage.append(this.canvas);
    this.overlay = el("div", "overlay hidden");
    stage.append(this.overlay);
    root.append(stage);

    this.rterr = el("div", "rterr hidden");
    root.append(this.rterr);
    return root;
  }

  fail(msg) {
    this.overlay.textContent = msg;
    this.overlay.classList.remove("hidden");
  }

  /** Compile and arm. Idempotent-safe: always stop() first. */
  start() {
    this.stop();
    this.overlay.classList.add("hidden");
    this.rterr.classList.add("hidden");
    this.frameIndex = 0;
    this.acc = 0;
    this.painter.base?.(this.img.data);
    if (typeof this.o.source !== "string") {
      this.fail(this.o.loadError ?? "source unavailable");
      this.o.onReady?.(this);
      return;
    }
    // Before compile: top-level init runs inside compile(), and clock-driven
    // patterns may read time-of-day there (Gitea #104). Same order as
    // snap.mjs's renderSide.
    state.host.setDefaultWallClock(WALL_CLOCK);
    const res = state.host.compile(this.o.source, this.rig.pixels, SEED);
    if (res.compileError) {
      this.fail(`compile: ${res.compileError}`);
      this.o.onReady?.(this);
      return;
    }
    this.engine = res;
    if (this.rig.kind === "grid") this.engine.setMapGrid(this.rig.gridW, this.rig.gridH);
    else if (this.rig.kind === "cloud") this.engine.setMap3D(cubeLattice(this.rig.cloudSide));
    this.engine.setWallClock(WALL_CLOCK);
    this.wants = this.engine.wantsSensors();
    this.rigNote.textContent = `${this.rig.pixels}px${this.wants ? " · beat120" : ""}`;
    for (const [name, vals] of Object.entries(this.values)) this.engine.setControl(name, vals);
    // Exported-var pins from tools/verify/fixups.json, pushed once after init
    // the way an external client would — a driven original (a mapper helper, a
    // home-automation bridge) renders nothing until something writes its var.
    // After the controls, so a control handler cannot clobber a pinned var.
    for (const [name, value] of Object.entries(this.o.vars ?? {})) this.engine.setVar(name, value);
    this.o.onReady?.(this);
  }

  step(deltaMs) {
    if (!this.engine) return;
    if (this.wants) this.engine.setSensors(sensorSlots(synthSensorFrame(this.frameIndex, state.fps)));
    const frame = this.engine.frame(deltaMs);
    this.painter.paint(frame, this.img.data);
    this.ctx.putImageData(this.img, 0, 0);
    this.frameIndex++;
    const err = this.engine.takeError();
    if (err) {
      // Runtime errors do not necessarily stop the pattern (some originals
      // throw once and keep rendering), so this annotates rather than covers.
      this.rterr.textContent = `runtime @f${this.frameIndex}: ${err.message ?? err}`;
      this.rterr.title = this.rterr.textContent;
      this.rterr.classList.remove("hidden");
    }
  }

  setControl(name, vals) {
    this.values[name] = vals;
    return this.engine?.setControl(name, vals) ?? null;
  }

  stop() {
    this.engine?.free();
    this.engine = null;
  }
}

// ---- decision bar -----------------------------------------------------------

const decisionBars = new Map(); // slug → Set<refresh fn>

function registerBar(slug, refresh) {
  if (!decisionBars.has(slug)) decisionBars.set(slug, new Set());
  decisionBars.get(slug).add(refresh);
  return () => decisionBars.get(slug)?.delete(refresh);
}

function refreshSlug(slug) {
  for (const fn of decisionBars.get(slug) ?? []) fn();
  updateProgress();
}

async function postDecision(pair, body) {
  const res = await fetch("/api/decision", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ slug: pair.slug, ...body }),
  });
  const saved = await res.json();
  if (!res.ok) throw new Error(saved.error ?? `HTTP ${res.status}`);
  pair.decision = saved.decision ? saved : null;
  refreshSlug(pair.slug);
  return pair.decision;
}

function makeDecisionBar(pair, { compact = false } = {}) {
  const root = el("div");
  const row = el("div", "decide");
  const buttons = new Map();
  for (const d of DECISIONS) {
    const b = el("button", null, DECISION_LABEL[d]);
    if (compact) b.style.fontSize = "11.5px";
    b.dataset.d = d;
    b.title = DECISION_LABEL[d];
    b.onclick = () => choose(d);
    buttons.set(d, b);
    row.append(b);
  }
  const saved = el("span", "saved hidden", "saved ✓");
  const clear = el("button", "clear hidden", "clear");
  clear.onclick = () => choose(null);
  row.append(saved, clear);
  root.append(row);

  const extra = el("div", "decide-extra hidden");
  const nameWrap = el("label", null, "");
  nameWrap.append(el("span", null, "fork name (optional)"));
  const nameInput = el("input");
  nameInput.type = "text";
  nameInput.placeholder = "new pattern name";
  nameWrap.append(nameInput);
  const fbWrap = el("label", null, "");
  fbWrap.append(el("span", null, "feedback — goes verbatim to the agent fixing the pattern"));
  const fb = el("textarea");
  fbWrap.append(fb);
  extra.append(nameWrap, fbWrap);
  root.append(extra);

  let busy = false;
  async function choose(d) {
    if (busy) return;
    busy = true;
    try {
      await postDecision(pair, {
        decision: d,
        forkName: nameInput.value,
        feedback: fb.value,
      });
      flashSaved();
    } catch (err) {
      alert(`save failed: ${err.message}`);
    } finally {
      busy = false;
    }
  }

  async function saveText() {
    const cur = pair.decision?.decision;
    if (!cur) return;
    if ((pair.decision.forkName ?? "") === nameInput.value.trim() &&
        (pair.decision.feedback ?? "") === fb.value.trim())
      return;
    await choose(cur);
  }
  nameInput.onblur = saveText;
  fb.onblur = saveText;

  let flashTimer = null;
  function flashSaved() {
    saved.classList.remove("hidden");
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => saved.classList.add("hidden"), 1800);
  }

  function refresh() {
    const cur = pair.decision?.decision ?? null;
    for (const [d, b] of buttons) b.classList.toggle("on", d === cur);
    clear.classList.toggle("hidden", !cur);
    const wantsText = cur === "fork" || cur === "needs-work";
    extra.classList.toggle("hidden", !wantsText);
    nameWrap.classList.toggle("hidden", cur !== "fork");
    if (document.activeElement !== nameInput) nameInput.value = pair.decision?.forkName ?? "";
    if (document.activeElement !== fb) fb.value = pair.decision?.feedback ?? "";
  }
  refresh();
  const unregister = registerBar(pair.slug, refresh);
  return { el: root, refresh, unregister };
}

// ---- cards ------------------------------------------------------------------

function verdictBadge(pair) {
  const v = pair.verdict?.verdict;
  if (!v) return el("span", "badge v-orig-unrenderable", "unjudged");
  // A non-visual pair is excluded, not scored — showing "0/10" would read as
  // a failing port.
  const scored = pair.verdict.score != null && v !== "non-visual";
  const b = el("span", `badge v-${v}`, scored ? `${v} ${pair.verdict.score}/10` : v);
  b.title = pair.verdict.confidence ? `confidence: ${pair.verdict.confidence}` : v;
  return b;
}

function rigLabel(rig) {
  if (rig.kind === "grid") return `grid ${rig.gridW}×${rig.gridH}`;
  if (rig.kind === "cloud") return `cloud ${rig.cloudSide}³`;
  return `strip ${rig.pixels}`;
}

function makeCard(pair) {
  const root = el("div", "card");
  root.dataset.slug = pair.slug;

  const head = el("div", "head");
  head.append(el("span", "name", pair.epeName || pair.slug));
  head.append(el("span", "spacer"));
  const rigB = el("span", "badge rig", rigLabel(pair.rig));
  if (pair.rig.overridden) rigB.title = "rig overridden by tools/verify/fixups.json";
  head.append(rigB, verdictBadge(pair));
  const decBadge = el("span", "badge hidden");
  const addrBadge = el("span", "badge hidden");
  head.append(decBadge, addrBadge);
  head.onclick = () => openModal(pair);
  root.append(head);

  const sub = el("div", "head");
  sub.append(el("span", "slug", pair.slug));
  if (pair.ambiguous) {
    const a = el("span", "badge rig", "ambiguous");
    a.title = `multiple corpus candidates: ${(pair.epeFiles ?? []).join(", ")}`;
    sub.append(el("span", "spacer"), a);
  }
  if (pair.fixups) {
    const f = el("span", "badge rig", "fixup");
    f.title = `original: stripped ${pair.fixups.removed} tripwire line(s)`;
    sub.append(f);
  }
  sub.onclick = () => openModal(pair);
  root.append(sub);

  const sides = el("div", "sides");
  const card = {
    pair,
    root,
    sides: null,
    live: false,
    visible: false,
    setDecided() {
      const d = pair.decision?.decision;
      decBadge.className = d ? `badge d-${d}` : "badge hidden";
      decBadge.textContent = d ? DECISION_LABEL[d] : "";
      const at = pair.decision?.addressedAt;
      addrBadge.className = at ? "badge d-addressed" : "badge hidden";
      addrBadge.textContent = at ? ADDRESSED_LABEL : "";
      if (at) addrBadge.title = `fix pass acted on this ${at} — re-review and re-decide`;
      root.classList.toggle("decided", !!d);
    },
  };
  root.append(sides);
  card.sidesEl = sides;

  if (pair.verdict?.summary) root.append(el("div", "summary", pair.verdict.summary));

  const bar = makeDecisionBar(pair, { compact: true });
  root.append(bar.el);
  registerBar(pair.slug, card.setDecided);
  card.setDecided();
  return card;
}

function activate(card) {
  if (card.live) return;
  const { pair } = card;
  card.sides = {
    orig: new Side({
      label: "Original",
      source: pair.origSource,
      loadError: pair.origError,
      rig: pair.rig,
      vars: pair.vars?.orig,
    }),
    port: new Side({
      label: "Port",
      source: pair.portSource,
      loadError: pair.portError,
      rig: pair.rig,
      vars: pair.vars?.port,
    }),
  };
  card.sidesEl.append(card.sides.orig.el, card.sides.port.el);
  card.sides.orig.start();
  card.sides.port.start();
  card.live = true;
  liveSides.push(card.sides.orig, card.sides.port);
}

function deactivate(card) {
  if (!card.live) return;
  for (const s of Object.values(card.sides)) {
    s.stop();
    const i = liveSides.indexOf(s);
    if (i >= 0) liveSides.splice(i, 1);
  }
  card.sidesEl.replaceChildren();
  card.sides = null;
  card.live = false;
}

// ---- scheduler --------------------------------------------------------------

const liveSides = [];
const modalSides = [];
let cursor = 0;
let lastT = 0;

function tick(t) {
  requestAnimationFrame(tick);
  const dt = lastT ? Math.min(t - lastT, 250) : 0;
  lastT = t;
  if (state.paused || dt <= 0) return;
  const stepMs = 1000 / state.fps;
  const list = modalSides.length ? modalSides.concat(liveSides) : liveSides;
  if (!list.length) return;
  for (const s of list) s.acc += dt;
  let budget = STEP_BUDGET;
  for (let n = 0; n < list.length && budget > 0; n++) {
    const s = list[(cursor + n) % list.length];
    if (s.acc >= stepMs) {
      s.step(stepMs);
      // Never let a starved side bank more than two steps of debt — it would
      // fast-forward in a burst the moment the budget frees up.
      s.acc = Math.min(s.acc - stepMs, stepMs * 2);
      budget--;
    }
  }
  cursor = (cursor + STEP_BUDGET) % list.length;
}

const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const card = cardBySlug.get(e.target.dataset.slug);
      if (card) card.visible = e.isIntersecting;
    }
    rebalance();
  },
  { rootMargin: "220px 0px" },
);

function rebalance() {
  let budget = MAX_LIVE_CARDS;
  for (const card of state.cards) {
    const want = card.visible && budget > 0 && !card.root.classList.contains("hidden");
    if (want) budget--;
    if (want) activate(card);
    else deactivate(card);
  }
}

// ---- detail modal -----------------------------------------------------------

let modalCleanup = null;

function closeModal() {
  modalCleanup?.();
  modalCleanup = null;
  $("modal").classList.add("hidden");
  $("modalBody").replaceChildren();
}

function controlPanel(side, source) {
  const wrap = el("div", "ctrls");
  const hints = parseControlHints(source ?? "");
  const readouts = [];
  const controls = side.engine ? side.engine.controls() : [];
  if (!controls.length) {
    wrap.append(el("div", "no-ctrls", side.engine ? "no controls" : "not running"));
    return { el: wrap, poll: () => {} };
  }
  for (const c of controls) {
    const row = el("div", "ctrl");
    const lbl = el("span", "lbl", c.label || c.name);
    lbl.title = `${c.name} (${c.kind})`;
    row.append(lbl);
    const h = hints.get(c.name) ?? {};
    const cur = (i, dflt) => side.values[c.name]?.[i] ?? dflt;

    // An UNTOUCHED control is running whatever the pattern's own top-level
    // code put in the variable, and the engine offers no way to read that back
    // (lx_set_control with no args INVOKES the handler, which would overwrite
    // it). Only a `//# default=` declares it. Without one, the position drawn
    // below is a GUESS, so say so: two sides both parked at a guessed 0.5 read
    // as "same value" while their engines hold different ones — exactly what
    // hid the orig-0.4 / port-0.5 Slope gap in holiday-diagonal-stripes until
    // the slider was nudged.
    const untouched = side.values[c.name] === undefined;
    const guessed =
      untouched && h.default === undefined && c.kind !== "trigger" && !READONLY_KINDS.has(c.kind);
    let flag = null;
    if (guessed) {
      row.classList.add("guess");
      flag = el("span", "guessflag", "?");
      flag.title =
        `${c.name}: no //# default declared, and a control's live value cannot be read back ` +
        `from the engine. The position shown is a PLACEHOLDER, not this side's actual value — ` +
        `the pattern's own top-level initialiser is what is rendering. Move it to take control ` +
        `(and to make both sides comparable, set the same value on each).`;
      row.append(flag);
    }
    // First user input makes the widget authoritative again.
    const settled = () => {
      row.classList.remove("guess");
      flag?.remove();
      flag = null;
    };

    if (c.kind === "slider" || c.kind === "inputNumber") {
      const min = h.min ?? 0;
      const max = h.max ?? 1;
      const step = h.step ?? (c.kind === "inputNumber" ? 1 : 0.001);
      const v0 = cur(0, h.default ?? (c.kind === "inputNumber" ? min : 0.5));
      const num = el("input", "num");
      num.type = "number";
      num.step = step;
      num.value = v0;
      const apply = (v) => {
        if (Number.isNaN(v)) return;
        side.setControl(c.name, [v]);
        num.value = v;
        settled();
      };
      if (c.kind === "slider") {
        const r = el("input");
        r.type = "range";
        r.min = min;
        r.max = max;
        r.step = step;
        r.value = v0;
        r.oninput = () => apply(Number(r.value));
        num.onchange = () => {
          r.value = num.value;
          apply(Number(num.value));
        };
        row.append(r);
      } else {
        num.onchange = () => apply(Number(num.value));
      }
      row.append(num);
    } else if (c.kind === "hsvPicker" || c.kind === "rgbPicker") {
      const stack = el("div", "ctrl-stack");
      const chans = c.kind === "hsvPicker" ? ["H", "S", "V"] : ["R", "G", "B"];
      const vals = [0, 1, 2].map((i) => cur(i, i === 0 ? 0 : 1));
      chans.forEach((ch, i) => {
        const line = el("div", "ctrl");
        line.append(el("span", "ch", ch));
        const r = el("input");
        r.type = "range";
        r.min = 0;
        r.max = 1;
        r.step = 0.001;
        r.value = vals[i];
        r.oninput = () => {
          vals[i] = Number(r.value);
          side.setControl(c.name, vals.slice());
          settled();
        };
        line.append(r);
        stack.append(line);
      });
      row.append(stack);
    } else if (c.kind === "toggle") {
      const cb = el("input");
      cb.type = "checkbox";
      cb.checked = cur(0, h.default ?? 0) > 0.5;
      // Native tri-state says "unknown" better than any badge can.
      cb.indeterminate = guessed;
      cb.onchange = () => {
        cb.indeterminate = false;
        side.setControl(c.name, [cb.checked ? 1 : 0]);
        settled();
      };
      row.append(cb);
    } else if (c.kind === "trigger") {
      const b = el("button", "btn", "fire");
      b.onclick = () => side.setControl(c.name, []);
      row.append(b);
    } else {
      // showNumber / gauge — read-only, polled below
      const out = el("span", "ro", "—");
      row.append(out);
      let meter = null;
      if (c.kind === "gauge") {
        meter = el("meter");
        meter.min = 0;
        meter.max = 1;
        meter.style.flex = "1";
        row.append(meter);
      }
      readouts.push({ name: c.name, out, meter });
    }
    wrap.append(row);
  }
  const poll = () => {
    if (!side.engine) return;
    for (const r of readouts) {
      const v = side.engine.setControl(r.name, []);
      if (v === null) continue;
      r.out.textContent = v.toFixed(4);
      if (r.meter) r.meter.value = Math.max(0, Math.min(1, v));
    }
  };
  return { el: wrap, poll };
}

function verdictDetail(v) {
  const wrap = el("div", "verdict-detail");
  if (!v) {
    wrap.append(el("p", "no-ctrls", "no verdict on file for this pair"));
    return wrap;
  }
  const list = (title, items) => {
    if (!items?.length) return;
    wrap.append(el("h3", null, title));
    const ul = el("ul");
    for (const x of items) ul.append(el("li", null, x));
    wrap.append(ul);
  };
  wrap.append(el("h3", null, "sweep summary"));
  wrap.append(el("p", null, v.summary || "—"));
  list("observations", v.observations);
  if (v.dials?.length) {
    wrap.append(el("h3", null, "dials"));
    const t = el("table", "dials");
    const hr = el("tr");
    for (const h of ["dial", "match", "original", "port"]) hr.append(el("th", null, h));
    t.append(hr);
    for (const d of v.dials) {
      const tr = el("tr");
      tr.append(el("td", null, d.name ?? ""));
      tr.append(el("td", d.matches ? "ok" : "no", d.matches ? "✓" : "✗"));
      tr.append(el("td", null, d.origEffect ?? ""));
      tr.append(el("td", null, d.portEffect ?? ""));
      t.append(tr);
    }
    wrap.append(t);
  }
  list("feedback for the fix pass", v.feedback);
  if (v.experiments?.length) list("experiments", [v.experiments.join(", ")]);
  return wrap;
}

function openModal(pair) {
  closeModal();
  const body = $("modalBody");

  const close = el("button", "btn modal-close", "✕ close");
  close.onclick = closeModal;
  body.append(close);

  const head = el("div", "modal-head");
  head.append(el("h2", null, pair.epeName || pair.slug));
  head.append(el("span", "slug mono", pair.slug));
  head.append(el("span", "badge rig", rigLabel(pair.rig)));
  head.append(verdictBadge(pair));
  body.append(head);
  const files = el("div", "slug mono");
  files.style.color = "var(--dim-2)";
  files.textContent = `${pair.libFile}  ·  ${pair.epeFile}`;
  body.append(files);

  const grid = el("div", "big-sides");
  const panels = [];
  const sides = {};
  for (const [key, label, source, err] of [
    ["orig", "Original", pair.origSource, pair.origError],
    ["port", "Port", pair.portSource, pair.portError],
  ]) {
    const col = el("div");
    const panelHost = el("div");
    const side = new Side({
      label,
      source,
      loadError: err,
      rig: pair.rig,
      vars: pair.vars?.[key],
      big: true,
      onReady: () => {
        const built = controlPanel(side, source);
        panelHost.replaceChildren(built.el);
        panels[key === "orig" ? 0 : 1] = built;
      },
    });
    side.el.classList.add("big");
    col.append(side.el);
    const reset = el("button", "btn", "⟳ reset this side");
    reset.style.margin = "6px 0";
    reset.onclick = () => side.start();
    col.append(reset, panelHost);
    side.start();
    grid.append(col);
    sides[key] = side;
    modalSides.push(side);
  }
  body.append(grid);

  const box = el("div", "decide-box");
  box.append(el("h3", null, "decision"));
  const bar = makeDecisionBar(pair);
  box.append(bar.el);
  body.append(box);

  body.append(verdictDetail(pair.verdict));

  const pollTimer = setInterval(() => {
    for (const p of panels) p?.poll();
  }, 250);

  $("modal").classList.remove("hidden");
  modalCleanup = () => {
    clearInterval(pollTimer);
    bar.unregister();
    for (const s of Object.values(sides)) {
      s.stop();
      const i = modalSides.indexOf(s);
      if (i >= 0) modalSides.splice(i, 1);
    }
  };
}

// ---- filters / chrome -------------------------------------------------------

const cardBySlug = new Map();

function updateProgress() {
  const decided = state.pairs.filter((p) => p.decision).length;
  $("progress").textContent = `decided ${decided} / ${state.pairs.length}`;
  for (const c of state.cards) c.setDecided();
}

function matches(pair) {
  if (state.query) {
    const q = state.query;
    if (!pair.slug.toLowerCase().includes(q) && !(pair.epeName ?? "").toLowerCase().includes(q))
      return false;
  }
  if (state.verdictFilter.size && !state.verdictFilter.has(pair.verdict?.verdict ?? "")) return false;
  if (state.decisionFilter.size) {
    const d = pair.decision?.decision ?? "undecided";
    const isAddressed = state.decisionFilter.has(ADDRESSED) && !!pair.decision?.addressedAt;
    if (!state.decisionFilter.has(d) && !isAddressed) return false;
  }
  return true;
}

function applyFilters() {
  let shown = 0;
  for (const card of state.cards) {
    const ok = matches(card.pair);
    card.root.classList.toggle("hidden", !ok);
    if (ok) shown++;
  }
  $("shown").textContent = `${shown} shown`;
  rebalance();
}

function chipRow(container, values, set, labels = {}) {
  for (const v of values) {
    const c = el("span", "chip", labels[v] ?? v);
    c.onclick = () => {
      if (set.has(v)) set.delete(v);
      else set.add(v);
      c.classList.toggle("on", set.has(v));
      applyFilters();
    };
    container.append(c);
  }
}

function wireChrome() {
  const fps = $("fps");
  fps.oninput = () => {
    state.fps = Number(fps.value);
    $("fpsOut").textContent = state.fps;
  };
  const pp = $("playPause");
  pp.onclick = () => {
    state.paused = !state.paused;
    pp.textContent = state.paused ? "▶ play" : "‖ pause";
    lastT = 0;
  };
  $("resetAll").onclick = () => {
    for (const s of liveSides.concat(modalSides)) s.start();
  };
  chipRow($("verdictFilters"), VERDICTS, state.verdictFilter);
  chipRow($("decisionFilters"), ["undecided", ...DECISIONS, ADDRESSED], state.decisionFilter, {
    ...DECISION_LABEL,
    [ADDRESSED]: ADDRESSED_LABEL,
  });
  const q = $("q");
  q.oninput = () => {
    state.query = q.value.trim().toLowerCase();
    applyFilters();
  };
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeModal();
  });
  $("modalBack").onclick = closeModal;
}

// ---- boot -------------------------------------------------------------------

(async function main() {
  wireChrome();
  const [host, data] = await Promise.all([
    loadEngineHost(),
    fetch("/api/data").then((r) => r.json()),
  ]);
  state.host = host;
  state.pairs = data.pairs;

  const list = $("list");
  $("loading").remove();
  const frag = document.createDocumentFragment();
  for (const pair of state.pairs) {
    const card = makeCard(pair);
    state.cards.push(card);
    cardBySlug.set(pair.slug, card);
    frag.append(card.root);
  }
  list.append(frag);
  for (const card of state.cards) observer.observe(card.root);

  updateProgress();
  applyFilters();
  requestAnimationFrame(tick);
})().catch((err) => {
  document.body.prepend(el("p", "loading", `failed to start: ${err.message}`));
  console.error(err);
});

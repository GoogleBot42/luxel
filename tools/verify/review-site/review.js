// Luxel library review page: renders the embedded item list, keeps a
// keep/fix/drop verdict + note per item in localStorage, exports/imports JSON.
// Keys are the library path stem: "fire" (library/fire.js),
// "scenes/fire-clock", "sprites/heart" — a few slugs exist in more than one kind.
"use strict";
(function () {
  const DATA = JSON.parse(document.getElementById("data").textContent);
  const PREFIX = "luxel-review:";
  const $ = (s, el = document) => el.querySelector(s);
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const fmt = (v) => (v == null ? "–" : v);

  function load(key) {
    try { return JSON.parse(localStorage.getItem(PREFIX + key)) || {}; } catch { return {}; }
  }
  function save(key, st) {
    if (!st.verdict && !(st.note || "").trim()) localStorage.removeItem(PREFIX + key);
    else localStorage.setItem(PREFIX + key, JSON.stringify({ verdict: st.verdict || "", note: st.note || "" }));
  }
  function allNotes() {
    const out = {};
    for (const it of items) {
      const st = load(it.key);
      if (st.verdict || (st.note || "").trim()) out[it.key] = { verdict: st.verdict || "", note: st.note || "" };
    }
    return out;
  }

  const items = [];
  const img = (src, alt, cls = "") =>
    src ? `<div class="img ${cls}"><a href="${esc(src)}" target="_blank" rel="noopener"><img loading="lazy" src="${esc(src)}" alt="${esc(alt)}"></a></div>` : "";
  const errs = (list) => (list.length ? `<div class="err">${list.map(esc).join("\n")}</div>` : "");
  const verdictUi = (key) =>
    `<div class="verdict">${["keep", "fix", "drop"]
      .map((v) => `<label><input type="radio" name="v-${esc(key)}" value="${v}"> ${v}</label>`)
      .join("")}<button class="clear" type="button">clear</button></div>
     <textarea placeholder="Note"></textarea>`;

  function patternCard(p) {
    return `<div class="head"><b>${esc(p.name)}</b><span class="slug">${esc(p.slug)}.js</span>
        ${p.flagged ? '<span class="badge">flagged</span>' : ""}</div>
      <div class="meta">${p.controls} controls · ${p.bytes} B source · host ms/frame ${fmt(p.ms32)} @32, ${fmt(p.ms64)} @64 ·
        lit ${fmt(p.lit32)} @32, ${fmt(p.lit64)} @64${p.strip ? ` · strip lit ${fmt(p.stripLit)}` : ""}</div>
      ${errs(p.errors)}
      <div class="lbl">32x32</div>${img(p.img32, p.slug + " 32x32")}
      <div class="lbl">64x64</div>${img(p.img64, p.slug + " 64x64")}
      ${p.strip ? `<div class="lbl">144 px strip, 8 s top to bottom${p.stripBulk ? " (renderBulk-only, strip Layout installed)" : ""}</div>${img(p.strip, p.slug + " strip", "strip")}` : ""}`;
  }
  function sceneCard(s) {
    const layers = s.layers
      .map((l) => `<li>${esc(l.kind)} <code>${esc(l.ref)}</code> <span>${esc(l.name || "")} · ${esc(l.blend)}${l.clip ? " · clip" : ""}${l.visible ? "" : " · HIDDEN"}</span></li>`)
      .join("");
    return `<div class="head"><b>${esc(s.name)}</b><span class="slug">scenes/${esc(s.slug)}.scene</span>
        ${s.flagged ? '<span class="badge">flagged</span>' : ""}</div>
      <div class="meta">${s.layers.length} layers · ${s.periodMs ? `loop ${s.periodMs / 1000} s` : "no keyframe loop (10 s shown)"} · wire ${fmt(s.wireBytes)} B ·
        host ms/frame ${fmt(s.ms64)} @64, ${fmt(s.ms128)} @128</div>
      <ol class="layers">${layers}</ol>
      ${errs(s.errors)}
      <div class="lbl">64x64</div>${img(s.img64, s.slug + " 64x64")}
      <div class="lbl">128x128 (2x2 chain)</div>${img(s.img128, s.slug + " 128x128")}`;
  }
  function spriteCard(s) {
    return `<div class="head"><b>${esc(s.name)}</b><span class="slug">sprites/${esc(s.slug)}.sprite</span>
        ${s.flagged ? '<span class="badge">flagged</span>' : ""}</div>
      <div class="meta">${fmt(s.w)}×${fmt(s.h)} · ${fmt(s.frames)} frame${s.frames === 1 ? "" : "s"} · ${s.fps ? s.fps + " fps" : "still"} · ${fmt(s.bytes)} B</div>
      ${errs(s.errors)}
      ${img(s.img, s.slug, "spr")}`;
  }

  function build(kind, list, keyOf, render) {
    const grid = $(`#${kind} .grid`);
    const frag = document.createDocumentFragment();
    for (const d of list) {
      const key = keyOf(d);
      const el = document.createElement("article");
      el.className = "card";
      el.id = "c-" + key.replace("/", "-");
      el.innerHTML = render(d) + verdictUi(key);
      const it = { kind, key, d, el, hay: (d.name + " " + d.slug + " " + key).toLowerCase() };
      items.push(it);
      wire(it);
      frag.appendChild(el);
    }
    grid.appendChild(frag);
  }

  function paint(it) {
    const st = load(it.key);
    it.el.querySelectorAll("input[type=radio]").forEach((r) => (r.checked = r.value === st.verdict));
    const ta = $("textarea", it.el);
    if (document.activeElement !== ta) ta.value = st.note || "";
    it.el.classList.remove("v-keep", "v-fix", "v-drop");
    if (st.verdict) it.el.classList.add("v-" + st.verdict);
  }
  function wire(it) {
    paint(it);
    it.el.addEventListener("change", (ev) => {
      if (ev.target.type !== "radio") return;
      const st = load(it.key);
      st.verdict = ev.target.value;
      save(it.key, st);
      paint(it);
      counts();
    });
    $("textarea", it.el).addEventListener("input", (ev) => {
      const st = load(it.key);
      st.note = ev.target.value;
      save(it.key, st);
      counts();
    });
    $(".clear", it.el).addEventListener("click", () => {
      save(it.key, {});
      paint(it);
      counts();
      apply();
    });
  }

  let filter = "all";
  function apply() {
    const q = $("#q").value.trim().toLowerCase();
    const shown = { patterns: 0, scenes: 0, sprites: 0 };
    for (const it of items) {
      const st = load(it.key);
      let ok = !q || it.hay.includes(q);
      if (filter === "patterns" || filter === "scenes" || filter === "sprites") ok = ok && it.kind === filter;
      if (filter === "flagged") ok = ok && it.d.flagged;
      if (filter === "annotated") ok = ok && !!(st.verdict || (st.note || "").trim());
      it.el.hidden = !ok;
      if (ok) shown[it.kind]++;
    }
    for (const k of Object.keys(shown)) {
      const total = items.filter((i) => i.kind === k).length;
      $(`#${k} .n`).textContent = shown[k] === total ? `(${total})` : `(${shown[k]} of ${total})`;
      $(`#${k}`).hidden = shown[k] === 0;
    }
  }
  function counts() {
    const n = { keep: 0, fix: 0, drop: 0, noted: 0 };
    let annotated = 0;
    for (const it of items) {
      const st = load(it.key);
      if (st.verdict) n[st.verdict]++;
      if ((st.note || "").trim()) n.noted++;
      if (st.verdict || (st.note || "").trim()) annotated++;
    }
    const by = (k) => items.filter((i) => i.kind === k).length;
    const flagged = items.filter((i) => i.d.flagged).length;
    $("#counts").textContent =
      `${by("patterns")} patterns · ${by("scenes")} scenes · ${by("sprites")} sprites · ${flagged} flagged — ` +
      `reviewed ${annotated}/${items.length}: keep ${n.keep} · fix ${n.fix} · drop ${n.drop} · ${n.noted} notes`;
  }
  function toast(msg) {
    const t = $("#toast");
    t.textContent = msg;
    t.hidden = false;
    clearTimeout(toast.h);
    toast.h = setTimeout(() => (t.hidden = true), 2500);
  }

  build("patterns", DATA.patterns, (d) => d.slug, patternCard);
  build("scenes", DATA.scenes, (d) => "scenes/" + d.slug, sceneCard);
  build("sprites", DATA.sprites, (d) => "sprites/" + d.slug, spriteCard);
  $("#commit").textContent = DATA.commit;

  $("#chips").addEventListener("click", (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    filter = b.dataset.f;
    document.querySelectorAll("#chips button").forEach((x) => x.classList.toggle("on", x === b));
    apply();
  });
  $("#q").addEventListener("input", apply);

  $("#export").addEventListener("click", async () => {
    const notes = allNotes();
    const text = JSON.stringify(notes, null, 2) + "\n";
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([text], { type: "application/json" }));
    a.download = "review-notes.json";
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    let copied = false;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      /* insecure context or no permission: the download still happened */
    }
    toast(`Exported ${Object.keys(notes).length} items${copied ? " (also copied to clipboard)" : ""}`);
  });
  $("#import").addEventListener("change", async (ev) => {
    const f = ev.target.files && ev.target.files[0];
    if (!f) return;
    try {
      const notes = JSON.parse(await f.text());
      let n = 0;
      for (const it of items) {
        const st = notes[it.key];
        if (!st) continue;
        save(it.key, { verdict: ["keep", "fix", "drop"].includes(st.verdict) ? st.verdict : "", note: st.note || "" });
        n++;
      }
      items.forEach(paint);
      counts();
      apply();
      toast(`Imported ${n} items`);
    } catch (e) {
      toast("Import failed: " + e.message);
    }
    ev.target.value = "";
  });

  counts();
  apply();
})();

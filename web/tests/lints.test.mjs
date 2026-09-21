// The kind report (`lx_kinds`, Gitea #627) → what the editor renders.
//
// Defends the mapping, not the inference: a boxed variable becomes exactly
// one warning-severity lint at its anchor, a JIT refusal becomes both a lint
// and the banner line, and a clean pattern produces NOTHING — the editor
// shows no strip at all rather than an empty one.
//
// Run: npm test from web/
import test from "node:test";
import assert from "node:assert/strict";

import { editorLints, jitWarning, lintSummary, plain } from "../src/lib/lints.ts";

const CLEAN = {
  jit: { eligible: true },
  dyn: [],
  stats: { typed_slots: 9, total_slots: 9 },
};

const BOXED = {
  jit: { eligible: true },
  dyn: [
    {
      name: "heat",
      scope: "global",
      fn: "",
      line: 3,
      col: 3,
      cause: "assign-merge",
      message: "`heat` is assigned two different kinds of value, so it runs boxed",
    },
  ],
  stats: { typed_slots: 8, total_slots: 9 },
};

const REFUSED = {
  jit: {
    eligible: false,
    reason: {
      kind: "callbacks",
      name: "arrayMutate",
      line: 4,
      col: 3,
      message: "`arrayMutate` takes a callback, which the JIT does not compile yet",
    },
  },
  dyn: [],
  stats: { typed_slots: 9, total_slots: 9 },
};

test("a clean pattern produces no lints, no banner and no status line", () => {
  assert.deepEqual(editorLints(CLEAN), []);
  assert.equal(jitWarning(CLEAN), null);
  assert.equal(lintSummary(editorLints(CLEAN)), "");
});

test("no report at all (a wasm build without lx_kinds) is the same as clean", () => {
  assert.deepEqual(editorLints(null), []);
  assert.equal(jitWarning(null), null);
});

test("a boxed variable becomes one lint at its anchor", () => {
  const lints = editorLints(BOXED);
  assert.equal(lints.length, 1);
  assert.deepEqual(lints[0], {
    line: 3,
    col: 3,
    message: BOXED.dyn[0].message,
    role: "boxed",
  });
  assert.equal(jitWarning(BOXED), null, "boxed is not a refusal");
});

test("the status line counts the boxed variables and quotes the first reason", () => {
  assert.equal(
    lintSummary(editorLints(BOXED)),
    "1 boxed variable · line 3 · heat is assigned two different kinds of value, so it runs boxed",
  );
  const two = {
    ...BOXED,
    dyn: [
      BOXED.dyn[0],
      { ...BOXED.dyn[0], name: "glow", line: 5, message: "`glow` holds both, so it runs boxed" },
    ],
  };
  assert.match(lintSummary(editorLints(two)), /^2 boxed variables · line 3 · heat /);
});

test("a refusal becomes a banner line AND a lint at the call site", () => {
  const w = jitWarning(REFUSED);
  assert.deepEqual(w, {
    line: 4,
    col: 3,
    text: "Runs in the interpreter on JIT boards: arrayMutate takes a callback, which the JIT does not compile yet",
  });
  const lints = editorLints(REFUSED);
  assert.equal(lints.length, 1);
  assert.equal(lints[0].role, "jit");
  assert.equal(lints[0].line, 4);
  // the squiggle's tooltip keeps the code quoting the banner drops
  assert.match(lints[0].message, /`arrayMutate`/);
  // the banner is its own surface: the boxed status line stays empty
  assert.equal(lintSummary(lints), "");
});

test("`eligible: false` with no reason shows nothing — never an empty banner", () => {
  assert.equal(jitWarning({ ...REFUSED, jit: { eligible: false } }), null);
  assert.deepEqual(editorLints({ ...REFUSED, jit: { eligible: false } }), []);
});

test("lints come back in source order, deduped, with unanchored slots dropped", () => {
  const report = {
    jit: REFUSED.jit,
    dyn: [
      { ...BOXED.dyn[0], name: "late", line: 9, col: 1, message: "`late` … boxed" },
      { ...BOXED.dyn[0], name: "early", line: 2, col: 7, message: "`early` … boxed" },
      { ...BOXED.dyn[0], name: "dup", line: 2, col: 7, message: "`early` … boxed" },
      { ...BOXED.dyn[0], name: "nodebug", line: 0, col: 0, message: "`nodebug` … boxed" },
    ],
    stats: { typed_slots: 1, total_slots: 4 },
  };
  const lints = editorLints(report);
  assert.deepEqual(
    lints.map((l) => [l.line, l.role]),
    [
      [2, "boxed"],
      [4, "jit"],
      [9, "boxed"],
    ],
  );
});

test("plain() strips the code quoting for the monospace strips", () => {
  assert.equal(plain("`heat` holds `two` kinds"), "heat holds two kinds");
});

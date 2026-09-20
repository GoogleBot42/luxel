// Unit tests for the boot resume decision (src/lib/resume.ts) — what a console
// does with the working copy the browser was holding when it loaded (#585).
//
// Run: `npm test` from web/ (node's built-in runner + type stripping, so the
// .ts module is imported directly — no build step, no test dependency).
//
// Why this is worth pinning: the decision is made once, behind the boot cover,
// against state assembled by a handshake — so the only place it is legible is
// here. The rule it enforces is #563's push rule, and the regression it guards
// against is the one #585 was filed for: a page LOAD replacing the program on
// the LEDs and stopping a playing playlist before anything was clicked.
import test from "node:test";
import assert from "node:assert/strict";
import { bootResume } from "../src/lib/resume.ts";

test("a clean working copy always defers to the device", () => {
  assert.equal(bootResume({ dirty: false, wipPatternId: "", runningId: "" }), "adopt-running");
  assert.equal(bootResume({ dirty: false, wipPatternId: "a", runningId: "b" }), "adopt-running");
  // even when it IS the running one: there is nothing unsaved to keep, and the
  // pulled source is the device's own answer about what it is playing
  assert.equal(bootResume({ dirty: false, wipPatternId: "a", runningId: "a" }), "adopt-running");
});

test("a dirty edit OF the running pattern resumes live push", () => {
  assert.equal(bootResume({ dirty: true, wipPatternId: "a", runningId: "a" }), "resume-live");
});

test("a dirty edit of some OTHER pattern is preview only", () => {
  assert.equal(bootResume({ dirty: true, wipPatternId: "a", runningId: "b" }), "resume-preview");
});

test("a dirty edit of a stored pattern the device is not running is preview only", () => {
  // the device is on an ad-hoc program (or a playlist item we could not name)
  assert.equal(bootResume({ dirty: true, wipPatternId: "a", runningId: "" }), "resume-preview");
});

test("a dirty library pick / import / share link is preview only", () => {
  assert.equal(bootResume({ dirty: true, wipPatternId: "", runningId: "b" }), "resume-preview");
});

test("two empty ids are NOT a match — the device's program wins", () => {
  // The regression #585 is about. An ad-hoc program on the device and an
  // unsaved copy from an earlier session both carry "", so treating equal ids
  // as a match would make the commonest resume of all take the LEDs over.
  assert.equal(bootResume({ dirty: true, wipPatternId: "", runningId: "" }), "resume-preview");
});

/**
 * Editor lints from the compiler's kind report (Gitea #627,
 * docs/jit-design.md §2 and §4a).
 *
 * `lx_kinds` hands back two advisory things about a pattern that compiled
 * fine: every variable the compiler had to keep BOXED (a `Dyn` slot — the
 * JIT cannot hold it in a register), and whether the whole program will be
 * refused by the JIT and run interpreted on boards that have one. Neither
 * is an error: the pattern previews and pushes exactly as before. This
 * module turns that JSON into the two things the editor renders — a list of
 * warning-severity lints for the code pane, and one line of text for the
 * interpreter banner — and nothing here touches the DOM, so it is unit
 * tested (`web/tests/lints.test.mjs`).
 */

/** Why the JIT will refuse the program; `kind` matches `/api/status`'s `jit.reason`. */
export interface JitReason {
  kind: string;
  name: string;
  line: number;
  col: number;
  message: string;
}

export interface JitReport {
  eligible: boolean;
  reason?: JitReason;
}

/** One boxed slot. `fn` is the function it lives in — empty for a global. */
export interface DynEntry {
  name: string;
  scope: "global" | "local" | "ret";
  fn: string;
  line: number;
  col: number;
  cause: string;
  message: string;
}

export interface KindsStats {
  typed_slots: number;
  total_slots: number;
}

export interface KindsReport {
  jit: JitReport;
  dyn: DynEntry[];
  stats: KindsStats;
}

/** A lint as the code pane wants it: a place, a message, and which of the
 *  two sources it came from (the banner shows only the `jit` one). */
export interface EditorLint {
  line: number;
  col: number;
  message: string;
  role: "boxed" | "jit";
}

/** Backticks are the Rust side's code quoting; the editor's strips are
 *  already monospace, so they only add noise there. */
export function plain(message: string): string {
  return message.replace(/`/g, "");
}

/**
 * Every lint to show in the code pane, in source order.
 *
 * A slot with no anchor (line 0 — a program compiled without debug info,
 * which the browser never produces) is dropped rather than pinned to line 1,
 * and two lints on the same spot with the same text collapse into one.
 */
export function editorLints(report: KindsReport | null): EditorLint[] {
  if (!report) return [];
  const out: EditorLint[] = [];
  for (const d of report.dyn) {
    if (d.line > 0) out.push({ line: d.line, col: d.col, message: d.message, role: "boxed" });
  }
  const reason = report.jit.eligible === false ? report.jit.reason : undefined;
  // the squiggle's tooltip keeps the Rust side's code quoting; the banner
  // (`jitWarning`) is a plain strip and drops it
  if (reason) {
    out.push({
      line: reason.line,
      col: reason.col,
      message: JIT_PREFIX + reason.message,
      role: "jit",
    });
  }
  const seen = new Set<string>();
  return out
    .filter((l) => {
      const key = `${l.line}:${l.col}:${l.message}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort((a, b) => a.line - b.line || a.col - b.col);
}

/** The interpreter banner's text and where it points, or null when the JIT
 *  can compile this pattern (which is the normal case). */
export function jitWarning(
  report: KindsReport | null,
): { line: number; col: number; text: string } | null {
  const reason = report?.jit.eligible === false ? report.jit.reason : undefined;
  if (!reason) return null;
  return { line: reason.line, col: reason.col, text: JIT_PREFIX + plain(reason.message) };
}

/** What a refusal costs, in the words docs/jit-design.md §4a asks for. */
const JIT_PREFIX = "Runs in the interpreter on JIT boards: ";

/** The device-side half of the same vocabulary (Gitea #658).
 *
 *  `jitWarning` above is a PREDICTION the browser makes from the kinds
 *  section; this is what the device actually did, out of `/api/status`'s
 *  `jit` object. The `reason` ids are shared by construction — the emitter's
 *  `Refusal::id`, `jitlint::JitRefusal::id` and `firmware/src/jit.rs`'s
 *  `REASONS` are one list — so the wording lives here, once, for both.
 *
 *  Returns null for the cases with nothing to say: no device, firmware
 *  older than #658, a board with no backend (`off` — most of the fleet),
 *  and the happy path (`native`, which the frame-rate marker reports
 *  instead). */
export function deviceJitReason(
  jit: { state: string; reason: string | null } | null | undefined,
): string | null {
  if (!jit || jit.state !== "interp") return null;
  const why = DEVICE_JIT_REASONS[jit.reason ?? ""];
  return "This device is running it in the interpreter: " + (why ?? jit.reason ?? "unknown reason");
}

/** One phrase per reason id. Deliberately plain: the person reading it
 *  wants to know whether they can do anything about it. */
const DEVICE_JIT_REASONS: Record<string, string> = {
  // the emitter's, shared with the compile-time lint
  unsupported: "the compiler emitted an instruction this device's backend does not have yet",
  "too-large": "the compiled pattern is bigger than the device's code buffer",
  "l32r-reach": "the compiled pattern's literal pool is out of reach (a compiler limit)",
  "frame-size": "a function needs a bigger stack frame than the backend can express",
  kinds: "the bytecode's type annotations did not verify — please report this",
  "param-overflow": "a function takes more arguments than the backend can hand over",
  "offset-reach": "a function addresses further from its frame than the backend can reach",
  scratch: "the backend ran out of registers on this pattern — please report this",
  "address-region": "the device's code buffer and its helpers are too far apart",
  "jump-reach": "a jump in this pattern is further than the backend can reach",
  untyped: "this pattern was compiled by an older browser build — re-save it",
  // device-only: nothing a compile-time lint could predict
  debug: "the debugger is attached, and debugging steps the interpreter",
  "init-error": "the pattern's setup raised an error, so its type annotations cannot be trusted",
  "no-buffer": "both code buffers are busy — a crossfade is still finishing",
  disabled: "the JIT is switched off on this device",
};

/** The code pane's one-line status: how many variables are boxed, and the
 *  first reason. Empty when there is nothing to say — the caller renders
 *  no strip at all rather than an empty one. */
export function lintSummary(lints: EditorLint[]): string {
  const boxed = lints.filter((l) => l.role === "boxed");
  const first = boxed[0];
  if (first === undefined) return "";
  const count =
    boxed.length === 1 ? "1 boxed variable" : `${boxed.length} boxed variables`;
  return `${count} · line ${first.line} · ${plain(first.message)}`;
}

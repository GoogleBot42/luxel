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

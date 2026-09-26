// Is the page's OWN origin a device? — the acceptance test behind
// `detectDeviceBase()` (stores/device.ts), pulled out here so it can be
// driven without a browser.
//
// WHY IT IS A LOOP and not one fetch (the 2026-09-26 64x64 panel): that board
// answers `/api/status` with an EMPTY 200 body, a 503, or a TCP reset for
// seconds at a time whenever it is short of internal heap — for ~20 s around
// a scene switch, and during its own cold-load burst. One probe that landed
// in that window used to throw inside `r.json()`, and a bare catch turned the
// whole console into a PLAYGROUND for the rest of the page's life
// (`deviceBase` is written once, at boot, and never re-evaluated). Jeremy:
// "sometimes the webpage reverts to being a playground; the device is still
// working."
//
// So a bad body is "busy, ask again", never "no device". Only an ANSWER that
// says this origin serves something else ends it early, which is what keeps a
// genuine playground instant:
//
//   * a 404 / 4xx           — a static host (the hosted UI on Pages)
//   * a 200 that is HTML    — a dev server's SPA fallback
//
// Everything else — a refused/reset/timed-out connection, a 5xx, an empty or
// unparsable body, a JSON body without the device's shape (the firmware's
// low-memory `{"ok":false,…}`) — is transient, and the caller decides what to
// do when the budget runs out.

/** What the origin looks like after the whole retry budget:
 *  - `device`: it answered with a genuine `/api/status` body.
 *  - `absent`: it answered, and it is not a device.
 *  - `busy`:   nothing conclusive in the whole budget — a device under
 *              memory pressure looks exactly like this. */
export type ProbeVerdict = "device" | "absent" | "busy";

export interface ProbeDeps {
  /** Normally `gatedFetch` — the gate's own ladder rides underneath. */
  fetch: (url: string, init?: RequestInit) => Promise<Response>;
  /** Injected so tests do not wait out the backoff. */
  sleep?: (ms: number) => Promise<void>;
  now?: () => number;
  /** Total patience. Sized so it outlasts one scene switch on the panel
   *  (~20 s of refusals observed, 2026-09-26) rather than the 8 s the single
   *  probe used to allow. */
  budgetMs?: number;
  /** Per-attempt deadline. `gatedFetch` has its own ~30 s one, which is far
   *  longer than a probe should hold the boot cover up for. */
  attemptMs?: number;
  /** `/api/status` by default; relative, so it is always same-origin. */
  path?: string;
  /** Called before each retry, with the number of attempts made so far. The
   *  boot cover says "the device is busy" rather than sitting on "loading…"
   *  for the length of the budget. */
  onRetry?: (attempts: number) => void;
}

/** The backoff ladder, in ms between attempts. It sums to ~20 s over seven
 *  attempts, front-loaded: a device that is merely mid-burst answers on the
 *  second or third try, and only a genuinely wedged one reaches the tail. */
const BACKOFF_MS = [250, 500, 1000, 2000, 4000, 6000, 8000];

export const PROBE_BUDGET_MS = 25000;
const ATTEMPT_MS = 8000;

/** One attempt's reading. Exported for the unit tests, and because naming
 *  the three outcomes is the whole point of the module. */
export async function classifyProbe(r: Response): Promise<ProbeVerdict> {
  // An HTTP error status is the ORIGIN answering. A 5xx is a device saying
  // "not now" (the firmware answers 503 whenever the heap cannot spare a
  // response segment — Gitea #753); a 4xx is a host that has no such route.
  if (!r.ok) return r.status >= 500 ? "busy" : "absent";
  const type = r.headers.get("content-type") ?? "";
  // A dev server's SPA fallback serves index.html for everything, and that is
  // a settled "not a device" — no point spending the budget on it.
  if (/text\/html/i.test(type)) return "absent";
  let body: unknown;
  try {
    const text = await r.text();
    if (text.trim() === "") return "busy"; // the panel's empty 200
    body = JSON.parse(text);
  } catch {
    return "busy"; // truncated mid-body: the device dropped the socket
  }
  const st = body as { pixels?: unknown } | null;
  if (st && typeof st === "object" && typeof st.pixels === "number") return "device";
  // Valid JSON, wrong shape: either the firmware's low-memory
  // `{"ok":false,"error":"…"}` or something else living at /api/status.
  // Transient is the safe reading — a wrong "absent" costs the console its
  // device for the whole session, a wrong "busy" costs a few seconds.
  return "busy";
}

/**
 * Probe this origin until it is conclusive or the budget is spent.
 *
 * Returns the verdict and how many attempts it took (the tests assert on the
 * second number; nothing in the app reads it).
 */
export async function probeDeviceOrigin(
  deps: ProbeDeps,
): Promise<{ verdict: ProbeVerdict; attempts: number }> {
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const now = deps.now ?? (() => Date.now());
  const budget = deps.budgetMs ?? PROBE_BUDGET_MS;
  const attemptMs = deps.attemptMs ?? ATTEMPT_MS;
  const path = deps.path ?? "/api/status";
  const started = now();
  let attempts = 0;
  for (;;) {
    attempts++;
    let verdict: ProbeVerdict = "busy";
    // Per-attempt deadline: a device slot that wedges mid-body would
    // otherwise hold the whole budget in one attempt and never retry.
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), attemptMs);
    try {
      verdict = await classifyProbe(await deps.fetch(path, { signal: ctl.signal }));
    } catch {
      verdict = "busy"; // refused, reset, or our own deadline — ask again
    } finally {
      clearTimeout(t);
    }
    if (verdict !== "busy") return { verdict, attempts };
    const wait = BACKOFF_MS[Math.min(attempts - 1, BACKOFF_MS.length - 1)] ?? 0;
    if (now() - started + wait >= budget) return { verdict: "busy", attempts };
    deps.onRetry?.(attempts);
    await sleep(wait);
  }
}

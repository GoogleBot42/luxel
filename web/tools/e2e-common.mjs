// Shared bits for the browser e2e harnesses: the port plan, and the driver
// for the app's in-app dialogs.
//
// ── ports (Gitea #496) ────────────────────────────────────────────────────
// Every port ANY harness binds derives from one env knob, `E2E_PORT`, which
// is the base of a 100-port block owned by a single run. Before this, only
// the `vite preview` server honoured `E2E_PORT` and every mirror port was a
// literal (`const DEV_PORT = 8723`), so two sessions running e2e at once
// either collided loudly or — worse — pointed one session's browser at the
// other session's mirror and "passed".
//
// Two concurrent sessions therefore only need `E2E_PORT` values 100 apart:
// use a multiple of 100 (4200, 4300, … 9000). The default block is
// 4179–4279, which keeps the old defaults of the three harnesses that had
// one (e2e 4179, device-e2e 4181, flash-e2e 4183).
//
// Offsets are FIXED and documented here + in docs/tools.md; nothing else may
// invent one.

/** Base of this run's 100-port block. */
export const E2E_BASE = Number(process.env.E2E_PORT ?? 4179);

const p = (offset) => E2E_BASE + offset;

/** Every port the harnesses bind, as `E2E_PORT + <fixed offset>`. */
export const PORT = {
  /** `vite preview` servers (one per harness that drives a browser). */
  web: {
    e2e: p(0),
    device: p(2),
    flash: p(4),
    maxpixels: p(6),
    debug: p(8), // tools/debug.mjs, the manual poke-at-it helper
    mockdiff: p(10), // tools/mockdiff.mjs, the mock-fidelity instrument
    bootRetry: p(12), // tools/bootretry-check.mjs, the #592 refused-bundle retry
  },
  /** `luxel serve` mirrors standing in for a device. */
  mirror: {
    device: p(20), // device-e2e: the main mirror
    tight: p(21), //             low-heap capacity mirror
    loaded: p(22), //            loaded-heap capacity mirror
    loadedPanel: p(23), //       panel mirror for the capacity section
    panel: p(24), //             panel mirror for the geometry section
    map: p(25), //               4096-px mirror for the map section
    slow: p(26), //              slow-fps mirror
    outputs: p(27), //           two-output mirror for the Settings Outputs table
    hub75: p(28), //             --board panel mirror for the Settings panel fields
    wipStrip: p(29), //          300 px strip mirror for the #573 working-copy section
    maxpixels: p(30), // maxpixels-e2e
    mdPanel: p(31), //   mockdiff: 64x64 HUB75 console (the S1/S2/S3/S4 frames)
    mdStrip: p(32), //   mockdiff: 300 px strip console (S1b, S3b, S3f)
    mdOutputs: p(33), // mockdiff: two-output strip console (S3j/S3k)
    mdLattice: p(34), // mockdiff: 3D lattice console (S3g)
    wipPanel: p(35), //  device-e2e: panel mirror for the #573 reverse case
    wipResume: p(36), // device-e2e: strip mirror for the #585 boot-resume section
    doomed: p(37), //    device-e2e: the mirror the liveness section KILLS (#538 r2)
    bcOld: p(38), //     device-e2e: --bc-format above the bundle's (#643 banner 1)
    bcNew: p(39), //     device-e2e: --bc-format below the bundle's (#643 banner 2)
    syncA: p(40), // sync-e2e leader
    syncB: p(41), // sync-e2e follower
    otaAccept: p(45), // device-e2e: --accept-ota mirror for the Update… flow (#643)
    staleStore: p(46), // device-e2e: --stale-store mirror for the self-heal (#643)
    jitNative: p(47), // device-e2e: --jit native, the #658 marker
    jitInterp: p(48), // device-e2e: --jit interp:REASON, the #658 refusal strip
  },
  /** sync-e2e's UDP beacon group port. */
  sync: p(42),
  /** device-e2e's network-input listeners (`luxel serve --ddp-port/--e131-port`).
   *  The standard 4048/5568 are global, so a concurrent session would own them. */
  netin: { ddp: p(43), e131: p(44) },
  /** flash-e2e's fake WLED device (tools/fake-wled.mjs). */
  fakeWled: p(50),
  /** lna-e2e's own https/http origins (LNA_HTTPS_PORT / LNA_HTTP_PORT still win). */
  lna: { https: p(60), http: p(61) },
  /** tools/serve-e2e.mjs's mirror; it also binds `serve + 1` for the
   *  `--board panel` impersonation at the end. */
  serve: p(70),
};

/**
 * `luxel serve` args for a mirror that is NOT the one under network-input
 * test. Port 0 binds an ephemeral port, so the mirror neither fights a
 * concurrent session for the global DDP/sACN ports nor logs a bind failure.
 */
export const NO_NETIN = ["--ddp-port", "0", "--e131-port", "0"];

// ── in-app dialogs (Gitea #472) ───────────────────────────────────────────
// Naming and confirmations are `components/Dialog.svelte`, not
// `window.prompt`/`confirm`, so the harnesses drive them through data-roles
// instead of a `page.on("dialog")` handler that matched on message text.

const DIALOG = '[data-role="dialog"]';

/** Wait for the modal to be on screen. */
export async function waitDialog(page, timeout = 5000) {
  await page.waitForSelector(DIALOG, { timeout });
}

/** Wait for it to be gone again (every accept/cancel settles the promise). */
export async function waitDialogGone(page, timeout = 5000) {
  await page.waitForFunction(
    (sel) => document.querySelector(sel) === null,
    { timeout },
    DIALOG,
  );
}

/** The dialog's title text, or "" when none is open. */
export async function dialogTitle(page) {
  return page.$eval('[data-role="dialog-title"]', (el) => el.textContent.trim()).catch(() => "");
}

/**
 * Accept the open dialog. `text` (optional) is typed into the prompt field
 * first — pass it for a naming dialog, omit it for a confirmation.
 */
export async function acceptDialog(page, text) {
  await waitDialog(page);
  if (text !== undefined) {
    await page.$eval(
      '[data-role="dialog-input"]',
      (el, v) => {
        el.value = v;
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      text,
    );
  }
  await page.click('[data-role="dialog-confirm"]');
  await waitDialogGone(page);
}

/** Dismiss the open dialog (the caller's action must then not happen). */
export async function cancelDialog(page) {
  await waitDialog(page);
  await page.click('[data-role="dialog-cancel"]');
  await waitDialogGone(page);
}

// ── the editor header (Gitea #468) ────────────────────────────────────────
// A7 moved the document's verbs into the editor's own header: the name edits
// INLINE (there is no naming dialog any more) and add-to-playlist, duplicate,
// export/import, share and delete live behind the ⋯ menu. Nothing in that
// menu exists in the DOM until it is open, so every harness reaches them
// through these helpers rather than clicking a role that may not be there.

const nap = (ms) => new Promise((r) => setTimeout(r, ms));

/** Open the ⋯ menu and click one of its items. */
export async function menuClick(page, role) {
  await page.click('[data-role="overflow"]');
  await nap(150);
  await page.click(`[data-role="${role}"]`);
  await nap(200);
}

/** Is `role` an item of the ⋯ menu? Leaves the menu closed. */
export async function menuHas(page, role) {
  await page.click('[data-role="overflow"]');
  await nap(150);
  const el = await page.$(`[data-role="${role}"]`);
  await page.click('[data-role="overflow"]'); // toggle it shut again
  await nap(100);
  return el !== null;
}

/** Commit an inline rename in the editor header. */
export async function renameTo(page, name) {
  await page.click('[data-role="pattern-name"]');
  await page.waitForSelector('[data-role="name-input"]', { timeout: 2000 });
  await page.$eval(
    '[data-role="name-input"]',
    (el, v) => {
      el.value = v;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    },
    name,
  );
  await page.focus('[data-role="name-input"]');
  await page.keyboard.press("Enter");
  await nap(200);
}

/**
 * The editor header's save-state line. Playground / live push:
 * `unsaved` · `saved · on device` · `saved · in browser` · `not saved yet`.
 * A console in LOCAL PREVIEW (the editor is not driving the device, #563):
 * `unsaved · preview only` · `saved · on device · preview only` ·
 * `preview only · not on device`.
 */
export function saveState(page) {
  return page.$eval('[data-role="save-state"]', (el) => (el.textContent ?? "").trim());
}

/**
 * The §5.7 sweep (Gitea #529): "a control is ABSENT unless the thing it acts
 * on exists — never disabled." The one deliberate exception is a control
 * disabled because the user must learn a BUDGET, and such a control has to
 * carry `data-reason` saying which budget (and show the same words on
 * screen).
 *
 * So the invariant a harness asserts is: in the whole live DOM, every
 * `[disabled]` / `[aria-disabled="true"]` element carries `data-reason`.
 * Anything else is a dimmed dead control and a bug.
 *
 * Returns the offenders as `[{role, tag, text}]` — empty means the page is
 * clean. Call it with every disclosure expanded and every state mounted;
 * a collapsed Advanced body is unmounted and hides its own controls.
 */
export function disabledSweep(page) {
  return page.$$eval('[disabled], [aria-disabled="true"]', (els) =>
    els
      .filter((el) => !el.hasAttribute("data-reason"))
      .map((el) => ({
        role: el.getAttribute("data-role") ?? "",
        tag: el.tagName.toLowerCase(),
        text: (el.textContent ?? "").trim().slice(0, 40),
      })),
  );
}

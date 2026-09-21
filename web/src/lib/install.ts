// Installing a release onto the device the console is talking to (Gitea
// #643): firmware AND the web assets that belong with it, from one file, as
// one action.
//
// The hole this closes: `POST /api/ota` and `POST /api/assets` are separate,
// and nothing made anyone do the second. A device that took a firmware OTA
// across an LXBC format bump kept serving the console that shipped with the
// OLD firmware — a console whose compiler cannot produce anything the new
// engine will run, on a device whose whole store the new engine cannot read.
// The Athom went dark exactly that way on 2026-09-20 and needed a checkout's
// worth of off-device tooling to come back.
//
// So: one `.luxr` package (lib/luxr.ts), one flow, and the assets land
// whether or not anyone remembers them. A bare `.bin` is still accepted —
// it is what every existing release artifact and every `nix build` output
// is — but the caller is told the assets are its responsibility.
//
// The orchestration lives here rather than in the Svelte card so the
// sequence, the board check and the reboot-settled rule are unit-testable
// (tests/install.test.mjs) and stated once.

import { isLuxr, parseLuxr, type LuxrPackage } from "./luxr.ts";

/** How long to give a device to write the image, reboot and answer again.
 *  The firmware's own boot window is ~60 s worst case (WiFi + the boot-loop
 *  guard's settle); `tools/ota-push.sh` waits the same 60 s. */
export const BOOT_WINDOW_MS = 60_000;
/** The device is writing flash for the first seconds after it replies — do
 *  not start polling into that. Mirrors ota-push.sh's `sleep 4`. */
const REBOOT_GRACE_MS = 4_000;
const POLL_MS = 2_000;

export type InstallStep = "firmware" | "rebooting" | "assets" | "done" | "failed";

export interface InstallProgress {
  step: InstallStep;
  /** One line for the user, already written for them. */
  text: string;
  /** 0..1 through the whole flow, for the progress element. */
  pct: number;
}

/** What a picked file turned out to be. */
export type Upload =
  | { kind: "package"; pkg: LuxrPackage }
  | { kind: "app"; app: Uint8Array };

/** Classify a picked file. A `.luxr` is parsed and hash-checked here (so a
 *  bad download fails before anything is streamed at the device); anything
 *  else is taken as a bare app image. */
export async function readUpload(bytes: Uint8Array): Promise<Upload> {
  if (isLuxr(bytes)) return { kind: "package", pkg: await parseLuxr(bytes) };
  return { kind: "app", app: bytes };
}

/** Why this package must not be installed on this device, or null.
 *
 *  This is #389's lesson moved one step earlier: the three classic-ESP32
 *  boards share an ELF path, so a wrong-board image flashes cleanly, boots
 *  fine and differs only in which pins it reserves — a failure that shows up
 *  days later as "the strip stopped working". `tools/ota-push.sh` catches it
 *  by grepping the image for `board::NAME`; a package names the board
 *  outright and `/api/status` reports the device's, so the console can
 *  simply compare them.
 *
 *  A device that does not report `board` (firmware older than the field)
 *  cannot be checked — the install proceeds, because refusing every older
 *  device would make the flow useless exactly where it is most needed.
 *
 *  CONTAINMENT, not equality, and deliberately so: `board_name` in
 *  firmware/board-target.sh is documented as a substring of `board::NAME`
 *  where the name varies with a feature (`board-s3-devkit` becomes
 *  "ESP32-S3 devkit + HUB75 panel" under `hub75`), and `ota-push.sh` greps
 *  the image for exactly that substring. This is the same test one step
 *  later, so the two cannot disagree about what counts as the right board. */
export function boardMismatch(pkgBoard: string, deviceBoard: string | undefined): string | null {
  if (!deviceBoard || !pkgBoard) return null;
  if (deviceBoard.includes(pkgBoard) || pkgBoard.includes(deviceBoard)) return null;
  return (
    `This package is built for ${pkgBoard}; this device reports ${deviceBoard}. ` +
    `Installing it would write another board's pin map into the OTA slot — ` +
    `download the package for ${deviceBoard}.`
  );
}

/** Has the device come back as something OTHER than what it was?
 *
 *  A real device always lands in the other OTA slot, so `slot` alone settles
 *  it; the version changes too whenever the release does. The mirror has one
 *  "slot" and reports a `+otaN` version instead. Requiring BOTH would hang
 *  on a re-install of the same version; requiring EITHER covers every host. */
export function rebootSettled(
  before: { version?: string; slot?: string },
  now: { version?: string; slot?: string },
): boolean {
  if (now.slot && before.slot && now.slot !== before.slot) return true;
  return !!now.version && !!before.version && now.version !== before.version;
}

/** The device-side calls the flow needs — narrowed to what it uses so a test
 *  can drive it with a few lines instead of an HTTP server. */
export interface InstallTarget {
  otaUpload(image: ArrayBuffer): Promise<{ ok: boolean; bytes?: number; error?: string }>;
  assetsUpload(archive: ArrayBuffer): Promise<{ ok: boolean; bytes?: number; error?: string }>;
  status(): Promise<{ version?: string; slot?: string; board?: string }>;
}

export interface InstallResult {
  ok: boolean;
  /** Set when the flow stopped; already a sentence. */
  error?: string;
  /** True when web assets were installed — the caller reloads the page, so
   *  the console the user is looking at becomes the one that just landed. */
  assetsInstalled: boolean;
  /** Version the device reports now. */
  version?: string;
}

function bufOf(a: Uint8Array): ArrayBuffer {
  return a.slice().buffer;
}

/**
 * Stream a release onto a device: app image → wait out the reboot → assets.
 *
 * Order matters and is not negotiable. The assets partition is served by the
 * running firmware, so pushing assets first would leave the NEW console in
 * front of the OLD engine for the length of the OTA — the exact skew this
 * whole ticket is about, just narrower. Firmware first means the worst
 * intermediate state is the one we already have today.
 *
 * `sleep` is injected so a test does not wait out a 60 s boot window.
 */
export async function installRelease(
  dev: InstallTarget,
  upload: Upload,
  before: { version?: string; slot?: string },
  onProgress: (p: InstallProgress) => void,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
  now: () => number = () => Date.now(),
): Promise<InstallResult> {
  const app = upload.kind === "package" ? upload.pkg.app : upload.app;
  const assets = upload.kind === "package" ? upload.pkg.assets : new Uint8Array(0);

  onProgress({
    step: "firmware",
    text: `sending the firmware image (${Math.round(app.length / 1024)} KB)…`,
    pct: 0.05,
  });
  let r: { ok: boolean; error?: string };
  try {
    r = await dev.otaUpload(bufOf(app));
  } catch (e) {
    return fail(`the device stopped answering while the image was uploading (${String(e)})`);
  }
  if (!r.ok)
    return fail(`the device refused the firmware image: ${r.error ?? "no reason given"}`);

  onProgress({
    step: "rebooting",
    text: "the device is rebooting into the new image…",
    pct: 0.5,
  });
  await sleep(REBOOT_GRACE_MS);
  const deadline = now() + BOOT_WINDOW_MS;
  let after: { version?: string; slot?: string } | null = null;
  while (now() < deadline) {
    try {
      const s = await dev.status();
      if (rebootSettled(before, s)) {
        after = s;
        break;
      }
    } catch {
      // still down — that is the expected answer for most of this window
    }
    await sleep(POLL_MS);
  }
  if (!after)
    return fail(
      "the device did not come back within 60 seconds. It may still be booting, or it " +
        "may have rolled back to the previous image — reload this page to check before " +
        "trying again.",
    );

  if (assets.length === 0) {
    onProgress({ step: "done", text: `running v${after.version ?? "?"}`, pct: 1 });
    return { ok: true, assetsInstalled: false, version: after.version };
  }

  onProgress({
    step: "assets",
    text: `sending the web app (${Math.round(assets.length / 1024)} KB)…`,
    pct: 0.8,
  });
  let ar: { ok: boolean; error?: string };
  try {
    ar = await dev.assetsUpload(bufOf(assets));
  } catch (e) {
    return fail(
      `the firmware installed, but the web app did not (${String(e)}). The device is ` +
        `running v${after.version ?? "?"} with its old console — install the web assets ` +
        `before saving any pattern.`,
      { version: after.version },
    );
  }
  if (!ar.ok)
    return fail(
      `the firmware installed, but the device refused the web app: ${ar.error ?? "no reason given"}. ` +
        `A hosted-UI build serves no on-device console — that refusal is expected there.`,
      { version: after.version },
    );

  onProgress({ step: "done", text: `running v${after.version ?? "?"} — reloading…`, pct: 1 });
  return { ok: true, assetsInstalled: true, version: after.version };

  function fail(error: string, extra: Partial<InstallResult> = {}): InstallResult {
    onProgress({ step: "failed", text: error, pct: 1 });
    return { ok: false, error, assetsInstalled: false, ...extra };
  }
}


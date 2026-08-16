// Talking to the device across origins: probing what's at an address,
// uploading the takeover image through WLED's /update, waiting for Luxel
// to come up, and pushing the web-asset bundle.
//
// Browser reality this code is built around (see docs/wled-migration.md):
//  - WLED 0.13 sends no CORS headers, so /json/info is often unreadable
//    cross-origin (0.14+ added `Access-Control-Allow-Origin: *`). A
//    `no-cors` probe still distinguishes "something answered" from
//    "nothing there".
//  - The /update POST is a multipart form — CORS-safelisted, so it can be
//    *sent* without preflight even when the response stays opaque.
//  - Luxel's own API serves CORS headers (the playground depends on it),
//    so once the takeover lands, everything is readable again.
//  - HTTPS-hosted pages can't touch http:// LAN devices (mixed content)
//    except in Chromium's Local Network Access scheme; we pass
//    `targetAddressSpace` (ignored where unsupported) and surface manual
//    fallback steps when the fetch is blocked outright.

/** Extra RequestInit member from Chromium's Local Network Access spec;
 * unknown dictionary members are ignored by other browsers. */
type LnaInit = RequestInit & { targetAddressSpace?: "private" | "local" };

/** The LNA hint only helps (and is only safe to send) from an https page
 * reaching down to a plain-http local-network device — the mixed-content
 * case. Per Chrome's Local Network Access spec the value is "local"
 * (the PNA-era "private" was renamed; "local" parses on old builds too),
 * and Chrome auto-detects private-IP literals and .local hosts anyway —
 * the explicit hint is what buys the mixed-content exemption. Chromium
 * fails any request whose actual address space doesn't match the hint
 * (measured against a loopback target from the live site), so send it
 * only for hosts that are genuinely local-space, and never for loopback
 * (already mixed-content-exempt). */
function lnaHint(origin: string): LnaInit {
  if (window.location.protocol !== "https:") return {};
  const host = new URL(origin).hostname;
  if (host === "localhost" || host === "127.0.0.1" || host === "[::1]") return {};
  const oct = /^(\d+)\.(\d+)\.\d+\.\d+$/.exec(host);
  const privateIp =
    oct !== null &&
    (oct[1] === "10" ||
      (oct[1] === "192" && oct[2] === "168") ||
      (oct[1] === "172" && Number(oct[2]) >= 16 && Number(oct[2]) <= 31));
  if (privateIp || host.endsWith(".local")) return { targetAddressSpace: "local" };
  return {};
}

function lna(origin: string, init: RequestInit = {}, timeoutMs = 4000): LnaInit {
  return { signal: AbortSignal.timeout(timeoutMs), ...lnaHint(origin), ...init };
}

/** Accepts "192.168.1.50", "192.168.1.50:8080", "luxel.local", or a full
 * http URL; returns a normalized http origin, or null if unparseable. */
export function normalizeAddress(input: string): string | null {
  let s = input.trim();
  if (s === "") return null;
  if (!/^[a-z]+:\/\//i.test(s)) s = `http://${s}`;
  try {
    const u = new URL(s);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return u.origin;
  } catch {
    return null;
  }
}

export type Probe =
  | { kind: "luxel"; version: string; slot: string }
  | { kind: "wled"; arch: string; version: string; name: string }
  | { kind: "reachable" } // something answered but we can't read it (CORS)
  | { kind: "unreachable"; blocked: boolean }; // blocked = fetch refused by the browser itself

/** True when a fetch failure smells like the *browser* refusing to try
 * (mixed content / private-network blocking) rather than a network miss.
 * Heuristic: an https page asking for an http LAN origin. */
function browserBlocked(origin: string): boolean {
  return window.location.protocol === "https:" && origin.startsWith("http:");
}

export async function probeDevice(origin: string): Promise<Probe> {
  // Luxel first: its status shape (version + slot) doubles as the success
  // signature after the takeover, and its CORS always works.
  try {
    const res = await fetch(`${origin}/api/status`, lna(origin));
    if (res.ok) {
      const s = (await res.json()) as { version?: string; slot?: string };
      if (typeof s.version === "string" && typeof s.slot === "string")
        return { kind: "luxel", version: s.version, slot: s.slot };
    }
  } catch {
    /* fall through */
  }
  try {
    const res = await fetch(`${origin}/json/info`, lna(origin));
    if (res.ok) {
      const i = (await res.json()) as { arch?: string; ver?: string; name?: string };
      if (typeof i.arch === "string")
        return { kind: "wled", arch: i.arch, version: i.ver ?? "?", name: i.name ?? "WLED" };
    }
  } catch {
    /* fall through */
  }
  try {
    await fetch(`${origin}/json/info`, lna(origin, { mode: "no-cors" }));
    return { kind: "reachable" }; // opaque response — WLED 0.13's CORS-less API
  } catch {
    return { kind: "unreachable", blocked: browserBlocked(origin) };
  }
}

/** Upload the app image through WLED's own OTA page. The response is
 * opaque cross-origin — success is judged by waitForLuxel() afterwards.
 * Returns false when the browser refused to send at all (mixed content /
 * private-network policy) — the caller then shows the manual steps. */
export async function uploadToWled(origin: string, image: Blob, filename: string): Promise<boolean> {
  const form = new FormData();
  form.append("update", new File([image], filename, { type: "application/octet-stream" }));
  try {
    await fetch(`${origin}/update`, lna(origin, { method: "POST", body: form, mode: "no-cors" }, 120_000));
    return true;
  } catch {
    return false;
  }
}

export interface WaitProgress {
  elapsedMs: number;
  budgetMs: number;
}

/** Poll until Luxel answers (WLED flash-write ≈30 s, takeover copy ≈15 s,
 * two reboots, WiFi join — see docs/wled-migration.md). */
export async function waitForLuxel(
  origin: string,
  onTick: (p: WaitProgress) => void,
  budgetMs = 180_000,
  intervalMs = 2000,
): Promise<{ version: string; slot: string } | null> {
  const t0 = Date.now();
  for (;;) {
    const elapsedMs = Date.now() - t0;
    if (elapsedMs > budgetMs) return null;
    onTick({ elapsedMs, budgetMs });
    const probe = await probeDevice(origin);
    if (probe.kind === "luxel") return probe;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

/** Push the LUXA web-asset bundle. A BufferSource body carries no
 * Content-Type, so this stays a simple request; Luxel replies with CORS
 * headers, so the result is readable. Hot-reloads, no reboot. */
export async function pushAssets(origin: string, luxa: ArrayBuffer): Promise<boolean> {
  try {
    const res = await fetch(`${origin}/api/assets`, lna(origin, { method: "POST", body: luxa }, 120_000));
    return res.ok;
  } catch {
    return false;
  }
}

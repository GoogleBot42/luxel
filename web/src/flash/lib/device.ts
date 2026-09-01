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

// The `targetAddressSpace` hint and the browser-blocked heuristic live in
// src/lib/lna.ts: the playground's fetch gate needs exactly the same rules
// (Gitea #162), and since a hint that disagrees with the target's real
// address space hard-fails the request, there is only ever one classifier.
import { browserBlocked, lnaHint, type LnaInit } from "../../lib/lna";

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

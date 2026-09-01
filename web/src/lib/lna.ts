// Chromium's Local Network Access (LNA), in one place: everything the app
// needs to reach a plain-http device on the user's LAN from an https-hosted
// copy of itself (the GitHub Pages build, linked from the firmware's fallback
// page as `?device=http://<host>`).
//
// The rules this encodes, measured against the live site (docs/wled-migration.md,
// UPDATES.md 2026-08-15/2026-08-30):
//  - An https page reaching an http LAN address is BOTH mixed content and a
//    local-network request. Chrome exempts recognized-local targets from mixed
//    content and gates them behind a Local Network Access permission prompt.
//  - `targetAddressSpace` is the request's declaration of which space the
//    target lives in. It is what buys the mixed-content exemption — but
//    Chromium HARD-FAILS any request whose hint doesn't match the target's
//    real address space, so loopback and public hosts must get NO hint.
//  - The current spec value is "local" (the PNA-era "private" was renamed).
//    Browsers that don't implement it ignore the unknown dictionary member.
//  - Headless chromium denies the permission outright (no prompt, and a CDP
//    `localNetworkAccess` grant did not help in Chromium 150), so the blocked
//    path is the one that's testable without a human; the granted path needs
//    a headful browser (Gitea #162).
//
// Both the installer page (src/flash/lib/device.ts) and the playground's fetch
// gate (src/lib/fetchgate.ts) run through here, so the two can't drift.

/** Extra RequestInit member from Chromium's Local Network Access spec;
 *  unknown dictionary members are ignored by other browsers. */
export type LnaInit = RequestInit & { targetAddressSpace?: "private" | "local" };

/** Just enough of `window.location` to classify a page — taking it as a
 *  parameter keeps every function here pure and unit-testable without a DOM. */
export interface PageLocation {
  readonly protocol: string;
  readonly href: string;
}

/** Chrome's three address spaces, as far as we need to tell them apart. */
export type AddressSpace = "loopback" | "local" | "public";

/** Where a hostname lives, by literal inspection only — no DNS. A name we
 *  can't place resolves wherever it resolves, and guessing wrong is worse
 *  than not guessing (a mismatched hint hard-fails the request), so unknown
 *  names are "public": the space that gets no hint. */
export function classifyHost(hostname: string): AddressSpace {
  // URL.hostname keeps IPv6 literals in brackets ("[::1]"); strip them.
  const host = hostname.toLowerCase().replace(/^\[|\]$/g, "");
  if (host === "localhost" || host.endsWith(".localhost")) return "loopback";

  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(host);
  if (v4 !== null) {
    const [a, b] = [Number(v4[1]), Number(v4[2])];
    if (a > 255 || b > 255 || Number(v4[3]) > 255 || Number(v4[4]) > 255) return "public";
    if (a === 127) return "loopback";
    if (a === 10) return "local";
    if (a === 192 && b === 168) return "local";
    if (a === 172 && b >= 16 && b <= 31) return "local";
    if (a === 169 && b === 254) return "local"; // link-local
    return "public";
  }

  if (host.includes(":")) {
    // IPv6 literal: ::1 loopback, fc00::/7 unique-local, fe80::/10 link-local.
    if (host === "::1" || host === "0:0:0:0:0:0:0:1") return "loopback";
    if (/^f[cd][0-9a-f]{0,2}:/.test(host)) return "local";
    if (/^fe[89ab][0-9a-f]:/.test(host)) return "local";
    return "public";
  }

  // mDNS names always resolve on the LAN.
  if (host.endsWith(".local")) return "local";
  return "public";
}

/** The page we're running in. Absent outside a browser (unit tests), where
 *  "not a page" is the honest answer: no hint, nothing blocked. */
function currentPage(): PageLocation {
  const loc = (globalThis as { location?: PageLocation }).location;
  return loc ?? { protocol: "file:", href: "file:///" };
}

/** Resolve a request target (absolute URL, or a path relative to the page)
 *  to an absolute URL; null when it can't be parsed. */
function resolveTarget(target: string, page: PageLocation): URL | null {
  try {
    return new URL(target, page.href);
  } catch {
    return null;
  }
}

/**
 * The `targetAddressSpace` hint for a request, or `{}` when sending one would
 * be wrong. Only an https page needs it (an http page has no mixed-content
 * problem and no LNA gate to open), and only a genuinely local-space target
 * may carry it — a hint that doesn't match the target's real space hard-fails
 * the fetch, so loopback and public targets deliberately get nothing.
 *
 * Spread into a RequestInit; harmless everywhere it isn't implemented.
 */
export function lnaHint(target: string, page: PageLocation = currentPage()): LnaInit {
  if (page.protocol !== "https:") return {};
  const url = resolveTarget(target, page);
  if (url === null) return {};
  if (classifyHost(url.hostname) !== "local") return {};
  return { targetAddressSpace: "local" };
}

/**
 * True when a failed fetch smells like the *browser* refusing to try — mixed
 * content or a denied Local Network Access permission — rather than a device
 * that isn't answering. Heuristic by construction: an https page asking for
 * an http target is the only shape that gets refused this way, and the fetch
 * API deliberately reports it as an indistinguishable TypeError.
 */
export function browserBlocked(target: string, page: PageLocation = currentPage()): boolean {
  if (page.protocol !== "https:") return false;
  const url = resolveTarget(target, page);
  return url !== null && url.protocol === "http:";
}

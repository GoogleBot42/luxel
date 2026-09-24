// Playlist transport reconciliation (Gitea #431).
//
// `POST /api/playlist/play|stop` does not return the new state, and neither
// the native mirror nor the firmware necessarily has APPLIED it by the time
// the UI's follow-up `GET /api/playlist` is answered — the mirror flips
// `playing` in its render loop, and both devices advance `index` there. A
// read-back that beats the device therefore reports the state the user just
// changed away from.
//
// That read used to latch: the playlist poll only ran while the UI already
// believed it was playing, so one early `playing: false` froze the transport
// on "play" while the device was happily advancing. The fix is two-sided —
// App.svelte polls the playlist for as long as the tab is open, and every
// read passes through `reconcileTransport`, which keeps showing what the user
// asked for until the device agrees or the settle window runs out.
import type { Playlist, PlaylistItem } from "./device";

/** How long a transport request outranks the device's read-back. Long enough
 *  to cover a slow render loop plus a WiFi round trip, short enough that a
 *  request the device REFUSED (an empty playlist, say) corrects itself in
 *  about the time it takes to notice. */
export const TRANSPORT_SETTLE_MS = 3000;

/** A play/stop the user asked for that the device has not confirmed yet. */
export interface TransportIntent {
  /** What was asked for: `true` after ▶ play, `false` after ■ stop. */
  playing: boolean;
  /** Wall-clock ms after which the device's own state wins regardless. */
  until: number;
}

export const transportIntent = (playing: boolean, now: number): TransportIntent => ({
  playing,
  until: now + TRANSPORT_SETTLE_MS,
});

/**
 * Fold a fresh `GET /api/playlist` into any pending transport request.
 *
 * The device wins as soon as it agrees with the request (the normal case,
 * usually the very next read) or once the settle window has passed (the
 * request was dropped or refused). Until then the pending `playing` is
 * overlaid on the read, so the transport shows the state being entered
 * rather than flipping back to the old one for a frame or two.
 */
export function reconcileTransport(
  read: Playlist,
  intent: TransportIntent | undefined,
  now: number,
): { playlist: Playlist; intent: TransportIntent | undefined } {
  if (intent === undefined) return { playlist: read, intent: undefined };
  if (read.playing === intent.playing || now >= intent.until) {
    return { playlist: read, intent: undefined };
  }
  return { playlist: { ...read, playing: intent.playing }, intent };
}

// ---- the wire (POST /api/playlist) and the read-back (GET) ----
//
// The line format is the firmware's and the mirror's (docs/api.md "Playlist"):
//
//     D <defaultSec>
//     X <crossfadeMs>
//     I <patternId> <sec>      a pattern item     (-1 = inherit the default)
//     C <name> <raw…>            its control overrides, raw 16.16
//     P <mode>                   its projection override
//     I S<sceneId> <sec>       a SCENE item (Gitea #478)
//
// A scene item names a RECORD, not a blob: its layers own their values, so it
// never carries `C` or `P` (both hosts ignore them under one). Kept here
// rather than inline in `DeviceSession` so it is unit-testable without a
// device — `web/tests/playlist.test.mjs`.

/** 16.16 fixed point, the device's control scale. */
const RAW = 65536;

/** Serialize a playlist to the POST body. */
export function playlistWire(pl: Playlist): string {
  const lines: string[] = [
    `D ${Math.max(0, Math.round(pl.defaultSec))}`,
    `X ${Math.max(0, Math.round(pl.crossfadeMs))}`,
  ];
  for (const it of pl.items) {
    const sec = it.sec === null ? -1 : Math.max(0, Math.round(it.sec));
    if (it.kind === "scene") {
      lines.push(`I S${it.id} ${sec}`);
      continue; // a scene's values live in its layers — never C/P here
    }
    lines.push(`I ${it.id} ${sec}`);
    for (const [name, vals] of Object.entries(it.controls ?? {})) {
      lines.push(`C ${name} ${vals.map((v) => Math.round(v * RAW)).join(" ")}`);
    }
    if (it.proj !== undefined) lines.push(`P ${it.proj}`);
  }
  return lines.join("\n");
}

/**
 * Normalize a `GET /api/playlist` body. Scene items come back without
 * `controls` at all (they have none) and a pre-#478 device sends no `kind`,
 * so every consumer downstream can read `item.controls` and `item.kind`
 * without a guard.
 */
export function normalizePlaylist(read: Playlist): Playlist {
  return {
    ...read,
    items: (read.items ?? []).map((it): PlaylistItem => ({
      ...it,
      kind: it.kind === "scene" ? "scene" : "pattern",
      controls: it.controls ?? {},
    })),
  };
}

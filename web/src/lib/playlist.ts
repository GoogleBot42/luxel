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
import type { Playlist } from "./device";

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

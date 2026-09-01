// One global gate for every fetch the app itself fires — API calls AND
// startup assets (luxel.wasm, gallery.json, …). The device's HTTP server
// keeps a deliberately tiny connection pool (2 sockets — RAM budget), and
// when the UI is served from the device they all share it: a browser
// firing parallel fetches can starve *itself*, with excess connections
// refused at the TCP level surfacing as "cannot reach device" while the
// device is perfectly healthy. Two defenses, both here so no call site
// has to care: at most MAX_INFLIGHT requests in flight (matching the
// device pool), and connection-level failures retry with backoff. HTTP
// error statuses are never retried — those responses came from the
// device. An abort (caller's AbortSignal) is never retried either.
// RETRIES sizes the patience window (~10 s of exponential backoff): during
// a cold load the browser's own html/css/js connections pin the device's
// two sockets for seconds at a time, and 3 retries (~1 s) burned out
// before a slot freed (observed on the Athom, 2-slot build).
import { lnaHint, type LnaInit } from "./lna";

const MAX_INFLIGHT = 2;
const RETRIES = 6;
// Overall per-attempt deadline (connect + headers + full body). A device
// slot that wedges mid-body would otherwise hang the fetch forever — a
// hung fetch is not a failed fetch, so without this it pins a gate slot
// and stalls every queued request behind it. Sized generously above the
// largest gated asset on device WiFi (~7 s observed for a 300 KB body).
const ATTEMPT_MS = 30000;

let inflight = 0;
const waiters: (() => void)[] = [];

export async function gatedFetch(url: string, init?: RequestInit): Promise<Response> {
  // Local Network Access: from an https-hosted copy of the app (the Pages
  // build, reached via `?device=http://…`), every device request is also a
  // mixed-content / local-network request. The hint is what buys the
  // exemption — and it must match the target's real address space, so lnaHint
  // returns nothing for same-origin assets, loopback and public hosts. See
  // lna.ts; the installer page shares the same classifier.
  const hint = lnaHint(url);
  while (inflight >= MAX_INFLIGHT) {
    await new Promise<void>((wake) => waiters.push(wake));
  }
  inflight++;
  try {
    for (let attempt = 0; ; attempt++) {
      try {
        // The slot must be held until the BODY is fully received, not just
        // the headers — fetch() resolves at headers, and a 300 KB gallery
        // body keeps the device socket busy for seconds afterwards (this
        // exact hole let 3+ connections pile onto the 2-socket device and
        // starve the boot handshake). Buffer inside the gate and hand back
        // a detached Response; a mid-body connection drop retries here too.
        const deadline = AbortSignal.timeout(ATTEMPT_MS);
        const signal = init?.signal ? AbortSignal.any([init.signal, deadline]) : deadline;
        const opts: LnaInit = { ...init, ...hint, signal };
        const res = await fetch(url, opts);
        const body = await res.arrayBuffer();
        const bodyless = res.status === 204 || res.status === 205 || res.status === 304;
        return new Response(bodyless ? null : body, {
          status: res.status,
          statusText: res.statusText,
          headers: res.headers,
        });
      } catch (err) {
        // Only the CALLER's abort ends the attempt loop — a deadline abort
        // (TimeoutError from AbortSignal.timeout) is a stall and retries.
        if (init?.signal?.aborted || attempt >= RETRIES) throw err;
        const backoff = 150 * 2 ** attempt + Math.random() * 100;
        await new Promise((r) => setTimeout(r, backoff));
      }
    }
  } finally {
    inflight--;
    waiters.shift()?.();
  }
}

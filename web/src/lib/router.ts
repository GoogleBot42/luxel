// The app's URL. One route per screen so a reload reopens what was open —
// Jeremy's explicit ask for Settings (2026-09-19).
//
// WHY THE HASH and not a real path: the device serves this console from
// flash and its GET router is a flat match over asset paths
// (`firmware/src/server.rs`) — an unknown path falls through to 404, there
// is no SPA fallback. `http://luxel-f6b0a8/settings` would therefore be a
// dead link on the hardware, which is the only place most people ever load
// this app. A fragment never reaches the server, so `#/settings` behaves
// identically on the device, on the native mirror and on the hosted copy.
// (A real path would need a firmware fallback route first — see the issue
// filed alongside this module.)
//
// The share link uses the fragment too (`#p=…`, `stores/pattern.ts`), so the
// two are kept apart by the leading slash: a route ALWAYS starts `#/`.

export type Page = "patterns" | "scenes" | "playlist" | "settings" | "editor" | "map";

export interface Route {
  page: Page;
  /**
   * The scene the SCENE editor holds — `#/scenes/5eed1c92` (Gitea #480).
   *
   * The pattern editor deliberately has no such argument (see below), and the
   * reason does not apply here: opening a scene does not run it. Nothing on
   * the fixture changes when this route is restored, so the id can live in
   * the URL, which is what lets a playlist row link straight at a scene.
   */
  id?: string;
}

// WHY THERE IS NO `?pattern=<id>`: a route names a SCREEN. Which DOCUMENT the
// editor holds is the working copy's business — it is restored from the
// autosave, or pulled from the device's running pattern, on every boot.
// Putting the id in the URL means a refresh RE-ACTIVATES that pattern on the
// hardware, overwriting whatever the device was actually running. A page
// refresh must never change what the LEDs are doing.

const PAGES: Page[] = ["patterns", "scenes", "playlist", "settings", "editor", "map"];

/** Parse a `location.hash`. Returns null when it is not a route — an empty
 *  hash, or a share fragment, which must not be mistaken for one. */
export function parseRoute(hash: string): Route | null {
  const raw = hash.replace(/^#/, "");
  if (!raw.startsWith("/")) return null;
  const q = raw.indexOf("?");
  const path = q === -1 ? raw : raw.slice(0, q); // a query is tolerated, never read
  const seg = path.replace(/^\/+/, "").replace(/\/+$/, "");
  if (seg === "") return { page: "patterns" };
  const [head, arg] = seg.split("/");
  const page = PAGES.find((p) => p === head);
  if (!page) return null;
  // `#/scenes/<id>` is the only route with an argument, and only an 8-hex id
  // is one — anything else is a link to the Scenes page, not a 404.
  if (page === "scenes" && arg !== undefined && /^[0-9a-f]{8}$/.test(arg)) {
    return { page, id: arg };
  }
  return { page };
}

/** The fragment for a route, `#` included. Patterns is the root (`#/`). */
export function routeHash(r: Route): string {
  if (r.page === "patterns") return "#/";
  if (r.page === "scenes" && r.id) return `#/scenes/${r.id}`;
  return `#/${r.page}`;
}

/** Push a route onto the history stack, unless it is already the current
 *  URL (so re-clicking the open tab doesn't stack duplicate entries). */
export function pushRoute(r: Route): void {
  const h = routeHash(r);
  if (location.hash === h) return;
  history.pushState(null, "", h);
}

/** Replace the current entry — for the route the app SETTLES on at boot,
 *  which is not a navigation the back button should have to undo. */
export function replaceRoute(r: Route): void {
  const h = routeHash(r);
  if (location.hash === h) return;
  history.replaceState(null, "", h);
}

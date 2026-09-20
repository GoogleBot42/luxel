// Build-time constants, substituted by `define` in vite.config.ts.

/** `web/public/pixelblaze-library.json` existed when this bundle was built.
 *  The scraped-corpus gallery is a LOCAL dev artefact — `gen-gallery.mjs`
 *  writes it only when `corpus/` is populated, and the device asset bundle
 *  never carries it — so the app must not REQUEST the file to find out
 *  whether it is there (Gitea #564: two 404s against the device's 3-socket
 *  pool on every cold load). */
declare const __HAS_PIXELBLAZE_LIBRARY__: boolean;

// Where the installer gets Luxel firmware from.
//
// Two modes, tried in order:
//
//  - "bundled": a `firmware/manifest.json` next to this page (the release
//    workflow publishes the installer together with the per-board OTA
//    images and the LUXA web-asset bundle). Everything is same-origin, so
//    the page can fetch the binaries itself and drive the whole flash.
//  - "github": no bundle (e.g. a self-hosted web-dist). Release *metadata*
//    is CORS-open on api.github.com, but the binary downloads are not
//    (verified 2026-08-15: no Access-Control-Allow-Origin on the asset
//    hosts, either hop), so the user downloads the file via a normal link
//    and hands it back through a file picker.

export const GITHUB_REPO = "GoogleBot42/luxel";

/** Chip architectures this installer can flash. */
export type Chip = "esp32" | "esp32c3";

/** Chips the WLED takeover supports. Releases build images for more chips
 * than these (docs/boards.md, docs/releases.md): those are untested on real
 * hardware, so they ship as artifact downloads only and are not offered here.
 * Adding one is a deliberate call — see Gitea #57 / #56. */
const FLASHABLE_CHIPS: Record<Chip, string> = {
  esp32: "ESP32",
  esp32c3: "ESP32-C3",
};

/** User-facing name list, e.g. "ESP32 and ESP32-C3". */
export const FLASHABLE_CHIP_NAMES = Object.values(FLASHABLE_CHIPS).join(" and ");

export function isFlashableChip(arch: string): arch is Chip {
  return Object.hasOwn(FLASHABLE_CHIPS, arch);
}

export interface BoardInfo {
  id: string;
  name: string;
  chip: Chip;
}

/** Boards offered by the takeover, in UI order — a subset of what the release
 * pipeline builds (.github/workflows/release.yml / flake.nix), limited to
 * FLASHABLE_CHIPS. Manifest entries with an id missing here are skipped. */
export const BOARDS: BoardInfo[] = [
  { id: "athom-music", name: "Athom music controller (LS8P family)", chip: "esp32" },
  { id: "esp32-generic", name: "Generic ESP32 board", chip: "esp32" },
  { id: "c3-devkit", name: "ESP32-C3 devkit", chip: "esp32c3" },
  { id: "pixelblaze-v3", name: "Pixelblaze v3", chip: "esp32" },
];

export interface BoardImage extends BoardInfo {
  /** File name of the app-only OTA image (what WLED's /update accepts). */
  file: string;
  /** URL the image can be downloaded from (same-origin in bundled mode). */
  url: string;
  size?: number;
}

export interface FirmwareSource {
  mode: "bundled" | "github";
  version: string;
  boards: BoardImage[];
  /** The packed web app (POST /api/assets after the takeover). */
  luxa: { file: string; url: string; size?: number };
  /** True when this page can fetch the binaries itself (same-origin). */
  canFetchBinaries: boolean;
  /** Human-facing page for this release. */
  releaseUrl: string;
}

interface Manifest {
  version: string;
  boards: { id: string; file: string; size?: number }[];
  luxa: { file: string; size?: number };
}

/** Normalize WLED's /json/info `.arch` spellings ("esp32-c3", "esp32c3"…). */
export function normalizeArch(arch: string): string {
  return arch.toLowerCase().replaceAll("-", "");
}

function boardInfo(id: string): BoardInfo | undefined {
  return BOARDS.find((b) => b.id === id);
}

async function fromManifest(): Promise<FirmwareSource | null> {
  let res: Response;
  try {
    res = await fetch("firmware/manifest.json", { cache: "no-cache" });
  } catch {
    return null;
  }
  if (!res.ok) return null;
  const m = (await res.json()) as Manifest;
  const boards: BoardImage[] = [];
  for (const b of m.boards) {
    const info = boardInfo(b.id);
    if (!info) continue; // manifest from a future release with unknown boards
    boards.push({ ...info, file: b.file, url: `firmware/${b.file}`, size: b.size });
  }
  if (boards.length === 0) return null;
  return {
    mode: "bundled",
    version: m.version,
    boards,
    luxa: { file: m.luxa.file, url: `firmware/${m.luxa.file}`, size: m.luxa.size },
    canFetchBinaries: true,
    releaseUrl: `https://github.com/${GITHUB_REPO}/releases/tag/v${m.version}`,
  };
}

interface GhAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

async function fromGithub(): Promise<FirmwareSource | null> {
  let res: Response;
  try {
    res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
  } catch {
    return null;
  }
  if (!res.ok) return null;
  const rel = (await res.json()) as { tag_name: string; html_url: string; assets: GhAsset[] };
  const version = rel.tag_name.replace(/^v/, "");
  const boards: BoardImage[] = [];
  let luxa: FirmwareSource["luxa"] | null = null;
  for (const a of rel.assets) {
    const ota = /^luxel-(.+)-[0-9][^-]*-ota\.bin$/.exec(a.name)?.[1];
    if (ota !== undefined) {
      const info = boardInfo(ota);
      if (info)
        boards.push({ ...info, file: a.name, url: a.browser_download_url, size: a.size });
      continue;
    }
    if (/^luxel-web-assets-.*\.luxa$/.test(a.name))
      luxa = { file: a.name, url: a.browser_download_url, size: a.size };
  }
  if (boards.length === 0 || !luxa) return null;
  return {
    mode: "github",
    version,
    boards,
    luxa,
    canFetchBinaries: false,
    releaseUrl: rel.html_url,
  };
}

export async function loadFirmwareSource(): Promise<FirmwareSource | null> {
  return (await fromManifest()) ?? (await fromGithub());
}

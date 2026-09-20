<script lang="ts">
  // The pattern editor (proposal §5.2, mockups S2/S2b/S2c/S2d).
  //
  // Three owners, and nothing crosses between them:
  //   · the HEADER owns the document — back, the inline-editable name, the
  //     save state, the one primary action (Save) and the ⋯ menu of document
  //     verbs (add to playlist, duplicate, export/import, delete, share).
  //   · the CODE PANE owns its errors — a gutter dot and a wavy underline on
  //     the line, and one status strip pinned to the bottom of the pane. No
  //     compile-error banner in the rail: the cause and the report used to be
  //     ~1000 px apart (research/ui-audit.md §5).
  //   · the PREVIEW HEADER owns the transport — play/pause, target fps, the
  //     mic (only when the pattern binds sensors) and the debugger, next to
  //     the thing they control.
  //
  // It owns the local WASM engine, the render loop and the device push — the
  // local-preview-plus-push model: the preview always runs on the local
  // engine, and the device is a sink we send code + controls to. On a console
  // the preview additionally runs the DEVICE OUTPUT CHAIN over each frame
  // (#466), so what is drawn here is what the wire would carry.
  //
  // THE PUSH RULE (Gitea #563, docs/web-architecture.md): the editor writes to
  // the device only while its document IS the device's running program
  // (`livePush`). Opening a pattern is browsing — a Library tile, or `Edit` on
  // a stored pattern that is not the one playing, opens in LOCAL PREVIEW:
  // nothing is sent, a running playlist keeps playing, and the header says so.
  // `Save` stores without activating; `▶ Play on device` is the one explicit
  // verb that hands the pattern to the LEDs, and live push resumes from there.
  // Before it, one click on a browsing page could stop a playlist and leave
  // the device on an unsaved ad-hoc program with nothing in the UI saying so.
  // The PLAYGROUND has no device and is unaffected.
  //
  // BOOT obeys the same rule (Gitea #585): resuming the browser's autosaved
  // working copy opens it in LOCAL PREVIEW unless it is an unsaved edit of the
  // program the device is already running — see `bootDevice` / `lib/resume.ts`.
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import Controls from "../components/Controls.svelte";
  import Debugger from "../components/Debugger.svelte";
  import DeviceChip from "../components/DeviceChip.svelte";
  import CodeEditor from "../components/Editor.svelte";
  import PinPanel from "../components/PinPanel.svelte";
  import Popover from "../components/Popover.svelte";
  import PreviewAsChip from "../components/PreviewAsChip.svelte";
  import Preview from "../components/Preview.svelte";
  import ProjectionRow from "../components/ProjectionRow.svelte";
  import VarWatcher from "../components/VarWatcher.svelte";
  import "../components/editor-frame.css";
  import { MicSource, toSensorBoardFrame } from "../lib/audio";
  import { lxpEnvelope } from "../lib/device";
  import type { ProjectionMode } from "../lib/geometry";
  import {
    Engine,
    type ColorOrder,
    type Control,
    type DebugSnapshot,
    type DeviceModel,
    type Diagnostic,
    type OutpipeSettings,
    type StepKind,
  } from "../lib/luxel";
  import { bootResume } from "../lib/resume";
  import {
    activateDevicePattern,
    addToPlaylist as addPatternToPlaylist,
    brightness,
    device,
    deviceCaps,
    deviceEngineHeap,
    deviceError,
    deviceFps,
    deviceHeapFree,
    deviceOutFps,
    devicePatterns,
    devicePixels,
    deviceRescanHz,
    deviceRunningId,
    deviceVmerr,
    isPlayground,
    outputStatus,
    paletteAmount,
    paletteFlat,
    paletteSupported,
    refreshDevicePatterns,
    refreshStatus,
  } from "../stores/device";
  import { confirm, promptText } from "../stores/dialog";
  import {
    configureEngine,
    layout,
    layoutName,
    layoutSignature,
    patternDims,
    pixelCount,
  } from "../stores/geometry";
  import { banners, clearNote, note, notes, setBanner } from "../stores/notify";
  import {
    compileToBytecode,
    controlValues,
    deleteFromLocalLibrary,
    devicePatternId,
    dirty,
    encodeShare,
    exampleName,
    exportEpe,
    findSaved,
    hints,
    livePush,
    luxel,
    newPatternSource,
    parseEpe,
    patternName,
    previewFps,
    projectionOverride,
    runtimeError,
    saved,
    saveToLocalLibrary,
    source,
  } from "../stores/pattern";

  /** The editor is the visible surface (drives keyboard shortcuts). */
  export let active = false;
  /** What the back button returns to — the shell knows, the editor doesn't. */
  export let backLabel = "Patterns";

  const dispatch = createEventDispatcher<{ open: void; back: void; openmap: void }>();

  let editor: CodeEditor;
  let preview: Preview;
  let fileInput: HTMLInputElement;
  let nameInput: HTMLInputElement;

  let engine: Engine | undefined;
  let compileError: Diagnostic | null = null;
  let controls: Control[] = [];
  let readouts = new Map<string, number>();
  let vars: Record<string, number | number[]> = {};
  /** The compiled pattern binds sensor-board variables (`frequencyData`, …),
   *  which is the ONLY reason the mic button exists (proposal §5.7 — S2 used
   *  to show it always). Read off the engine, never the source text. */
  let wantsSensors = false;
  /** Digital pins the running pattern touches — the engine can't be asked
   *  statically (pin numbers are runtime values), so this is polled off
   *  `lx_pins_used` and the pin panel is shown only when it's non-empty
   *  (Gitea #205). `pinLatched` is owned here, not by the panel, because a
   *  recompile builds a fresh VM with nothing driven. */
  let pins: number[] = [];
  let pinLevels: Record<number, boolean> = {};
  let pinIdleHigh: Record<number, boolean> = {};
  let pinLatched: Record<number, boolean> = {};
  /** Analog pins the pattern samples (`analogRead`/`touchRead`) and the value
   *  each slider is parked at, 0..1 (Gitea #206). Polled the same way, and
   *  cleared on recompile for the same reason: a fresh VM reads 0 everywhere. */
  let analogPins: number[] = [];
  let analogValues: Record<number, number> = {};
  let targetFps = 60;
  let running = true;
  let debugMode = false;
  let breakpoints: number[] = [];
  let dbg: DebugSnapshot = { paused: false };
  /** Fetching/activating a pattern for the editor — cover the editor with a
   *  loading screen so a stale last-opened script never flashes first. */
  let patternLoading = false;
  /** The header's ⋯ menu (the document verbs). */
  let menuOpen = false;
  /** The ⋯ button the menu hangs off (components/Popover.svelte). */
  let moreBtn: HTMLElement;
  let importError = "";
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let pushDebounce: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let lastT = 0;
  let lastPoll = 0;

  /** The Layout moved — the header's "Preview as" chip, the console's shape
   *  chip, a device whose geometry changed — so the preview engine has to
   *  be rebuilt at the new pixel count and re-given its map (#463). Compared
   *  by `layoutKey`, not by identity: the 1 Hz status poll re-derives the
   *  Layout every second without changing it. A Layout change NEVER pushes to
   *  the device; it only rearranges what this browser draws. */
  let builtRig = "";
  $: if ($layoutSignature !== builtRig && $luxel) recompile();

  /** The device's vmerr, but only when it is a capacity rejection — other
   *  runtime errors are the local engine's business and are reported by the
   *  code pane's own status strip.
   *
   *  Two shapes reach here. The load-time refusal ("pattern too large for this
   *  device") comes from the floor check, and so does the array-arena BYTE
   *  budget. The array ELEMENT ledger is the third: `array(pixelCount)` that
   *  fits the editor's preview layout and not the device's real pixel count
   *  fails during the pattern's init, and the device then renders black with
   *  nothing but this vmerr to say why (Gitea #420). Mirrors
   *  `luxel_core::vm::is_array_budget_error`. */
  $: deviceRejectedForSize =
    $deviceVmerr && /too large for this device|array element budget exceeded/.test($deviceVmerr)
      ? $deviceVmerr
      : "";

  // ---- the document: name, save state (proposal §5.2) ----
  //
  // These are FUNCTIONS called from the markup with every dependency passed in,
  // not `$:` derivations. `matchRunningToLibrary()` below writes `patternName`
  // and `devicePatternId` from inside a reactive statement, and a `$:` whose
  // input is assigned that way renders one cycle stale — Svelte captures the
  // dirty bits for the fragment patch, but does not re-run earlier reactive
  // statements (.claude/rules/web.md; it cost this ticket an afternoon when
  // the header read "untitled pattern" for a pattern it had just adopted a
  // name for). A markup expression, given the same dependencies, is patched
  // with those bits and is always current.

  /** What the header shows when the name is not being edited. */
  function nameOf(name: string, example: string): string {
    return name || example || "untitled pattern";
  }

  /** `1D`/`2D`/`3D` — `preferredDims()`'s 0 means "no preference", i.e. 1D
   *  (mockups S2c `Sunset Fade · 1D`, S2d `Aurora 2D · 2D`). */
  function dimsLabel(d: number): string {
    return `${d === 0 ? 1 : d}D`;
  }

  /**
   * The one line that answers "is what I am looking at stored anywhere — and
   * is it what the LEDs are doing?". Its exact strings are a documented
   * contract (`data-role="save-state"`, docs/web-architecture.md):
   *
   *   playground / live push   `unsaved` · `saved · on device`
   *                            `saved · in browser` · `not saved yet`
   *   console, local preview   `unsaved · preview only`
   *                            `saved · on device · preview only`
   *                            `preview only · not on device`
   *
   * The `preview only` half is the whole of the #563 disclosure: the editor is
   * NOT driving the device, so nothing typed here is on the LEDs.
   */
  function saveStateOf(
    drt: boolean,
    dev: unknown,
    dpid: string,
    name: string,
    lib: { name: string }[],
    live: boolean,
  ): string {
    if (dev && !live) {
      if (drt) return "unsaved · preview only";
      return dpid ? "saved · on device · preview only" : "preview only · not on device";
    }
    if (drt) return "unsaved";
    if (dev && dpid) return "saved · on device";
    if (!dev && name !== "" && lib.some((s) => s.name === name)) return "saved · in browser";
    return "not saved yet";
  }

  /** Delete targets a stored pattern: the device's copy on a console, this
   *  browser's library entry in the playground. Absent when there is neither. */
  function canDeleteNow(
    dev: unknown,
    dpid: string,
    example: string,
    name: string,
    lib: { name: string }[],
  ): boolean {
    if (dev) return dpid !== "";
    return example === "" && lib.some((s) => s.name === name);
  }

  let editingName = false;
  let nameDraft = "";
  let nameError = "";

  /** Click-to-edit. `reason` seeds the inline rejection when the rename is
   *  forced by something else (Save on an unnamed pattern). */
  function startRename(reason = ""): void {
    nameDraft = $patternName || $exampleName;
    nameError = reason;
    editingName = true;
    void tick().then(() => {
      nameInput?.focus();
      nameInput?.select();
    });
  }

  /** Enter or blur commits; an empty name is refused IN PLACE (nothing is
   *  disabled — proposal §5.7), so the field stays open with the reason. */
  function commitName(): void {
    if (!editingName) return;
    const next = nameDraft.trim();
    if (next === "") {
      nameError = "a name is required";
      void tick().then(() => nameInput?.focus());
      return;
    }
    editingName = false;
    nameError = "";
    if (next === ($patternName || $exampleName)) return;
    patternName.set(next);
    exampleName.set("");
    // The stored copy still carries the old name, so the document no longer
    // matches it: Save (re-)stores it under the new one.
    dirty.set(true);
  }

  function cancelRename(): void {
    editingName = false;
    nameError = "";
  }

  function onNameKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      commitName();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelRename();
    }
  }

  // ---- capacity warning (Gitea #15) ----
  // Different ESP32s have different heap. A pattern that runs happily in the
  // playground can be rejected by the device it is pushed to, and the device's
  // rejection is ASYNCHRONOUS — /api/code returns 200 and the render task then
  // records a "pattern too large" vmerr — so without this the editor would
  // just silently keep showing the old pattern on the strip.
  //
  // Prediction: the wasm build models the firmware's own load sequence under a
  // counting allocator (`Luxel.deviceModel`), against the device's reported
  // free heap. Confirmation: after each push we read /api/status back, so the
  // device's own verdict replaces our guess the moment it exists.
  //
  // The model predicts the LIVE push (POST /api/code) — what this editor does
  // on every recompile, and the more expensive of the firmware's two load
  // paths: the upload envelope stays resident across a COPYING decode, where
  // a pattern activated from the device's own library is decoded straight off
  // the flash mapping with its code borrowed (Gitea #276/#300). When the live
  // push is the only thing that doesn't fit, the banner says so, because
  // "save it to the device" is then a real fix (Gitea #287).

  /** The local prediction for the source currently in the editor. */
  let capacity: { level: "over" | "tight"; text: string; detail: string } | null = null;

  // Re-model on every successful compile (the engine is rebuilt) and on every
  // fresh heap reading from the 1 Hz status poll. Dependencies are named in
  // the block itself — a `$:` only tracks what appears in its own syntax
  // (.claude/rules/web.md).
  $: {
    engine;
    compileError;
    $device;
    $deviceHeapFree;
    $deviceEngineHeap;
    $devicePixels;
    if (!compileError) checkCapacity();
  }

  const kb = (bytes: number): string => `${Math.round(bytes / 1024)} KB`;

  /** Model the pattern currently in the preview engine against the connected
   *  device and set (or clear) `capacity`. Silent in the playground, and
   *  silent on any device that can't report its free heap — an unknown budget
   *  is not a small one, and a guess here would cry wolf on every pattern. */
  function checkCapacity(): void {
    const lx = $luxel;
    if (!lx || !engine || !$device || $deviceHeapFree <= 0 || $devicePixels <= 0) {
      capacity = null;
      return;
    }
    const bytecode = engine.bytecode();
    // The device holds the WHOLE upload (name + source + bytecode) while it
    // decodes, so the envelope length — not the blob length — is what its heap
    // actually sees. Model against the device's own pixel count, never the
    // preview layout's: the strip length is hardware truth.
    const envelopeLen = lxpEnvelope("", $source, bytecode).length;
    const m: DeviceModel | null = lx.deviceModel(
      bytecode,
      envelopeLen,
      $devicePixels,
      $deviceHeapFree,
      $deviceEngineHeap,
    );
    if (!m) {
      capacity = null;
      return;
    }
    // "Saving it would fit" is only worth saying when it is actually true and
    // actually different — the stored path skips the envelope and borrows the
    // program's code from flash instead of copying it.
    const savingHelps = m.fit === "over" && m.storedFit !== "over";
    const detail =
      `a live push models ${kb(m.resident)} resident (peaking at ${kb(m.peak)}) ` +
      `at ${$devicePixels} px; saved to the device's library it would be ` +
      `${kb(m.storedResident)} (peak ${kb(m.storedPeak)}). The device reports ` +
      `${kb($deviceHeapFree)} free with its current pattern's ${kb($deviceEngineHeap)} ` +
      `still loaded, frees that first, and keeps ${kb(m.floor)} for itself — ` +
      `leaving ${kb(m.headroom)} for this pattern.` +
      (m.vmerr ? ` Device verdict: ${m.vmerr}` : "");
    const savingHint = savingHelps
      ? ` — saving it to the device's library would fit (${kb(m.storedResident)})`
      : "";
    if (m.vmerr) {
      // An array budget ran out before the floor check could even run — a
      // different failure from "the whole load doesn't fit", and worth saying
      // so, because the fix is smaller arrays rather than a smaller pattern.
      // Which budget matters: the byte arena is a size the user can read off
      // in KB, while the PB-compat ELEMENT ledger is a count that depends on
      // the device's pixel count, and blowing it makes the pattern load and
      // then render BLACK rather than be refused (Gitea #420). The exact
      // figures are in `detail` ("Device verdict: …").
      const elementLedger = /array element budget exceeded/.test(m.vmerr);
      capacity = {
        level: "over",
        text: elementLedger
          ? `this pattern needs more array elements than this device allows at ${$devicePixels} px — it would load and render black`
          : `this pattern's arrays exceed this device's array memory budget (${kb(m.budget)} available)`,
        detail,
      };
    } else if (m.fit === "over" && m.peak > m.base) {
      // The RESIDENT engine would have fitted; the upload itself doesn't.
      // Naming the upload is the difference between "rewrite your pattern"
      // and "save it to the device instead".
      capacity = {
        level: "over",
        text: `this pattern's upload is likely too large for this device (${kb(m.peak)} needed to receive it, ${kb(m.base)} free)${savingHint}`,
        detail,
      };
    } else if (m.fit === "over") {
      capacity = {
        level: "over",
        text: `this pattern is likely too large for this device (${kb(m.resident)} needed, ${kb(m.headroom)} free)${savingHint}`,
        detail,
      };
    } else if (m.fit === "tight") {
      capacity = {
        level: "tight",
        text: `close to this device's limit (${kb(m.resident)} of ${kb(m.headroom)} usable)`,
        detail,
      };
    } else {
      capacity = null;
    }
  }

  // ---- compile + push ----

  /** Rebuild the local preview engine from `source`. Device-independent: the
   *  preview always runs on the local WASM engine, even on a device. */
  export function recompile(): void {
    const lx = $luxel;
    if (!lx) return;
    const result = lx.compile($source, pixelCount());
    if (result instanceof Engine) {
      // What the COMPILED pattern wants — the only honest source of it, and
      // what playground Auto follows (D7). Telling the store can move the
      // Layout, and a moved Layout is a different pixel count, so the engine
      // is rebuilt at it. Recurses exactly once: the second pass agrees.
      const dims = result.preferredDims();
      if (dims !== $patternDims) {
        patternDims.set(dims);
        if (pixelCount() !== result.pixelCount) {
          result.free();
          recompile();
          return;
        }
      }
      engine?.free();
      engine = result;
      compileError = null;
      runtimeError.set(null);
      configureEngine(engine); // map + projection, from the ONE Layout
      applyProjection(); // …then this pattern's own override, if any (§5.4d)
      applyOutpipe(); // the device output chain the console previews through
      builtRig = $layoutSignature; // this engine matches the current Layout
      engine.setWallClock(Date.now() / 1000);
      controls = engine.controls();
      vars = engine.vars(); // VARS is absent for a pattern that exports none
      wantsSensors = engine.wantsSensors();
      if (!wantsSensors && micOn) toggleMic(); // nothing consumes the audio
      if (debugMode) {
        engine.debugEnable(true);
        applyBreakpoints();
      }
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      // seed //# defaults, then reapply saved control values (PB persists
      // control state per pattern)
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge" || c.kind === "trigger") continue;
        const d = $hints.get(c.name)?.default;
        if (!(c.name in $controlValues) && d !== undefined) {
          controlValues.set({ ...$controlValues, [c.name]: [d] });
        }
        const savedValues = $controlValues[c.name];
        if (savedValues) engine.setControl(c.name, savedValues);
      }
      // a fresh VM drives nothing, so drop any latched pins and parked
      // analog values with it
      pinLatched = {};
      analogValues = {};
      refreshPins();
      preview?.clear(); // fresh program → fresh history
      // Re-model against the device on every successful compile, not just on
      // push: the warning is then already up while the 500 ms push debounce is
      // still counting down, and it appears for a pattern merely *opened* from
      // the library too — before the user has invested any editing in it.
      checkCapacity();
    } else {
      compileError = result; // keep the old engine running while typing
    }
  }

  /** Send the current source + its compiled LXBC bytecode to the device so
   *  its real LEDs follow the local preview. The device has no compiler —
   *  the local engine's bytecode IS what it executes; a rejection (or
   *  network error) surfaces as a device error. */
  export async function devicePush(): Promise<void> {
    const d = $device;
    if (!d) return;
    if (!$livePush) return; // local preview — the device is not ours to write (#563)
    if (compileError || !engine) return; // never push a pattern the local compile rejected
    const bc = engine.bytecode();
    try {
      const r = await d.run($source, bc);
      if (!r.ok) deviceError.set(`device rejected the pattern: ${r.error}`);
      else deviceError.set("");
    } catch (e) {
      deviceError.set(`push failed: ${String(e)}`);
      return;
    }
    // A push rebuilds the device's engine from its Layout defaults, so the
    // working copy's projection override has to be re-stated (Gitea #598) —
    // and before the read-back, so `/api/status` describes what is running.
    await devicePushProjection();
    // Read the device back: a capacity rejection happens on the render task
    // AFTER /api/code has already answered 200, so the vmerr is the only place
    // it surfaces. This also refreshes the headroom the next prediction uses.
    await refreshStatus();
  }

  /** Local recompile (for the preview) plus a device push when the document
   *  is the running program — used when a whole new pattern is opened/created
   *  (typing debounces separately). `devicePush` is the one gate (#563). */
  function applyEdit(): void {
    recompile();
    if ($device) void devicePush();
  }

  function onSourceChange(e: CustomEvent<string>): void {
    source.set(e.detail);
    dirty.set(true); // the user edited away from the loaded/saved pattern
    // local preview recompiles fast; the device push (over WiFi) is throttled
    clearTimeout(debounce);
    debounce = setTimeout(recompile, 150);
    if ($device && $livePush) {
      clearTimeout(pushDebounce);
      pushDebounce = setTimeout(() => void devicePush(), 500);
    }
  }

  // The device is unreachable / rejected a push — a condition, not an event,
  // so it goes in the banner list and clears itself when it clears.
  $: setBanner("device-error", $deviceError ? { level: "error", text: $deviceError } : null);

  // ---- projection (proposal §5.4d) ----

  /** Point the preview engine at the projection actually in force: the
   *  Layout's default, with this pattern's override substituted for its own
   *  dimensionality. Called right after `configureEngine`, which installs the
   *  defaults — and before the first frame, because `pixelCount` under an
   *  along-axis projection becomes the strip length. */
  function applyProjection(): void {
    if (!engine) return;
    const mode = $projectionOverride;
    if (mode === null) return; // configureEngine already installed the default
    const key = $patternDims === 3 ? "proj3d" : $patternDims === 2 ? "proj2d" : "proj1d";
    engine.setProjection({ ...$layout.projection, [key]: mode });
  }

  /**
   * Send the working copy's projection override to the device (Gitea #598).
   * The device's engine is built from its Layout DEFAULTS, so the override
   * has to be stated — on a pick, and again after every code push, because a
   * push rebuilds that engine. `null` posts "no override", which puts the
   * device back on its own defaults.
   *
   * Silent on failure: firmware older than #598 rejects the `proj` line as
   * an unknown one, and an unreachable device is already reported by
   * `devicePush`.
   */
  async function devicePushProjection(): Promise<void> {
    const d = $device;
    if (!d || !$livePush) return;
    try {
      await d.setProjection($projectionOverride);
    } catch {
      /* the device-error banner already covers an unreachable device */
    }
  }

  /** A pick from the quiet Projection row. The engine reads `pixelCount` at
   *  init time under an along-axis projection, so this rebuilds rather than
   *  patching a running VM — locally, and on the device (#598), which is not
   *  a rebuild there: its `pixelCount` follows `set_projection`. */
  function onProjectionSet(e: CustomEvent<ProjectionMode | null>): void {
    projectionOverride.set(e.detail);
    recompile();
    void devicePushProjection();
  }

  // ---- the device output chain (Gitea #466) ----

  const COLOR_ORDERS: readonly string[] = ["rgb", "rbg", "grb", "gbr", "brg", "bgr"];

  /**
   * Configure the engine's copy of the DEVICE output chain — palette remap,
   * blur, glow, colour order, gamma, power cap — from what this device
   * reports. Without it the console preview differs from the strip by the
   * whole Settings page (research/engine-constraints.md §8v). In the
   * playground there is no device, so the chain is off and `outpipe()` would
   * equal `frame()`; we draw the raw frame there instead of paying for it.
   *
   * Cheap and idempotent: called on compile and whenever the device's output
   * settings change, never per frame.
   */
  function applyOutpipe(): void {
    if (!engine) return;
    if (!$device) {
      engine.setOutpipe({});
      return;
    }
    const o = $outputStatus;
    const order = o && COLOR_ORDERS.includes(o.order) ? (o.order as ColorOrder) : "rgb";
    const s: OutpipeSettings = {
      order,
      gamma: o?.gamma ?? 0,
      capMa: o?.capMa ?? 0,
      brightCurve: o?.brightCurve ?? 0,
      blur: o?.blur ?? 0,
      glow: o?.glow ?? 0,
      palette: $paletteSupported ? [...$paletteFlat] : [],
      paletteAmount: $paletteSupported ? $paletteAmount : 0,
      brightness: $brightness,
      // Which per-pixel current model the power cap uses is a capability, not
      // a board name (#464): a panel time-multiplexes its rows, a strip
      // conducts every pixel at once. Unknown caps ⇒ the conservative strip
      // model, which is also what every non-panel board wants.
      powerModel: $deviceCaps?.panel ? "hub75" : "strip",
      panelScan: Math.max(1, Math.round(($layout.h || 64) / 2)),
    };
    engine.setOutpipe(s);
  }

  // Every input of the chain, named in the block itself so Svelte tracks them.
  $: {
    engine;
    $device;
    $outputStatus;
    $paletteSupported;
    $paletteFlat;
    $paletteAmount;
    $brightness;
    $deviceCaps;
    $layout;
    applyOutpipe();
  }

  // ---- boot ----

  /** The source the device reported at connect, kept so the running pattern
   *  can be named once the stored sources have streamed in. */
  let runningSource = "";

  /**
   * Device mode: a local engine first so the boot cover lifts onto a live
   * preview, then the handshake, then the pulled/resumed source.
   *
   * The handshake ALWAYS pulls the running program now (#585). It used to be
   * skipped when the browser arrived holding a dirty working copy, because
   * that copy was about to be pushed over the top anyway — which meant a page
   * load replaced the user's installation and stopped a playing playlist
   * before anything had been clicked. What the device is running is instead
   * the INPUT to the decision: `lib/resume.ts`.
   */
  export async function bootDevice(
    connect: () => Promise<{ ok: boolean; source: string | null }>,
    wip: { dirty: boolean; devicePatternId: string },
  ): Promise<void> {
    recompile();
    startLoop();
    livePush.set(false); // nothing is ours to write until the decision below
    deviceRunningId.set(""); // ad-hoc until the handshake names what is running
    // Nothing to reset for geometry: the console's Layout IS the device's, and
    // the handshake's `/api/status` + `/api/map` reads are what the reconciler
    // is watching (#463).
    const r = await connect();
    if (r.ok) compileError = null;
    runningSource = r.source ?? "";
    nameRunningPattern($devicePatterns, runningSource, $device, $deviceRunningId);
    if (wip.dirty && wip.devicePatternId && !$deviceRunningId) {
      // The one case the library match above cannot settle on its own: the
      // pulled source is an ad-hoc-looking blob until the stored sources have
      // streamed in, and the boot cannot wait for them. Ask the one pattern
      // the answer depends on.
      await confirmRunning(wip.devicePatternId, runningSource);
    }
    const how = bootResume({
      dirty: wip.dirty,
      wipPatternId: wip.devicePatternId,
      runningId: $deviceRunningId,
    });
    if (how === "adopt-running") {
      if (r.source !== null) {
        source.set(r.source); // show what's running on the device
        dirty.set(false); // editor now matches the running pattern
        patternName.set("");
        exampleName.set("");
        devicePatternId.set("");
        projectionOverride.set(null);
      }
      livePush.set(true); // the document IS the running program (#563)
    } else {
      // The resumed copy is the document either way; only `resume-live` —
      // an unsaved edit OF the running program — may drive the LEDs.
      devicePatternId.set(wip.devicePatternId);
      livePush.set(how === "resume-live");
    }
    recompile(); // rebuild the preview from the pulled/resumed source
    if (how === "resume-live" && $device) await devicePush();
  }

  /** Name the RUNNING pattern from the device library (the device streams
   *  source, never which row it came from). Never overwrites an id we already
   *  have: `refreshPlaylist()` names it authoritatively while a playlist is
   *  playing, and every activation names it as it happens.
   *
   *  This is the running PROGRAM's identity, distinct from
   *  `matchRunningToLibrary` below, which names the editor's DOCUMENT — the
   *  two are the same string only while the editor is in live push (#563).
   *
   *  Deps as args, the house style below: the stored sources stream in one at
   *  a time after the handshake, so this re-runs until one of them matches. */
  $: nameRunningPattern($devicePatterns, runningSource, $device, $deviceRunningId);
  function nameRunningPattern(
    pats: { id: string; source?: string }[],
    src: string,
    dev: unknown,
    rid: string,
  ): void {
    if (!dev || rid || src.trim() === "") return;
    const m = pats.find((p) => p.source && p.source.trim() === src.trim());
    if (m) deviceRunningId.set(m.id);
  }

  /** Is the device running the document the resumed working copy is an edit
   *  of? Used only by the boot decision, when the library sources have not
   *  streamed in yet and it cannot wait for them (#585). Sets
   *  `deviceRunningId` when the answer is yes.
   *
   *  Two ways it can be yes: the device still holds the STORED version (no
   *  edit has been pushed yet), or it is running this very working copy —
   *  the session that dirtied it was in live push and had already sent the
   *  edit before the reload, so the id it claims is still the honest one. */
  async function confirmRunning(id: string, running: string): Promise<void> {
    const d = $device;
    if (!d || running.trim() === "") return;
    if (running.trim() === $source.trim()) {
      deviceRunningId.set(id);
      return; // no read needed — the device is running what we are holding
    }
    try {
      const p = await d.patternSource(id);
      if (p.source.trim() === running.trim()) deviceRunningId.set(id);
    } catch {
      /* the pattern is gone from the device — it is not what's running */
    }
  }

  /** Playground mode. A pre-#463 share link's map program is run by the shell
   *  (`runMapProgram`), not here — the map is not the pattern's (#471). */
  export function bootPlayground(): void {
    recompile();
    startLoop();
  }

  function startLoop(): void {
    if (raf === 0) raf = requestAnimationFrame(loop);
  }

  // ---- opening patterns ----

  /** Everything a fresh document resets. A projection override is a VALUE of
   *  the working copy, so it is dropped exactly where the slider values are.
   *
   *  It also drops LIVE PUSH: a document that has just been replaced is not
   *  what the device is running, so the editor goes back to local preview
   *  until something explicitly hands the new one to the LEDs (#563). The
   *  callers that DO run it (`playDevicePattern`, `playOnDevice`) set it
   *  after. In the playground the flag is inert — there is no device. */
  function resetDocumentState(): void {
    importError = "";
    controlValues.set({});
    projectionOverride.set(null);
    editingName = false;
    nameError = "";
    livePush.set(false);
  }

  export function newPattern(): void {
    // The template follows the Layout: a matrix console starts you in
    // `render2D`, not on a 1D ramp it will show row-major (#463).
    source.set(newPatternSource($layout.dims));
    patternName.set("");
    exampleName.set("");
    devicePatternId.set("");
    resetDocumentState();
    dirty.set(false); // a fresh template — not yet edited
    preview?.clear();
    void tick().then(applyEdit);
  }

  export function loadSaved(name: string): void {
    const p = findSaved(name);
    if (!p) return;
    preview?.clear();
    patternName.set(p.name);
    exampleName.set("");
    source.set(p.source);
    resetDocumentState();
    dirty.set(false); // freshly loaded from the library
    void tick().then(applyEdit);
  }

  /** Open a pattern the gallery was showing. No geometry travels with it: the
   *  tile and the editor reconcile the same Layout from the same inputs, so
   *  the preview already matches the tile it was clicked on (#463). */
  export function loadGalleryPick(p: { name: string; source: string }): void {
    preview?.clear(); // picking a pattern opens it in the editor
    patternName.set(p.name);
    exampleName.set("");
    devicePatternId.set("");
    source.set(p.source);
    resetDocumentState();
    dirty.set(false); // freshly picked from the gallery
    void tick().then(applyEdit);
  }

  /**
   * `Edit` on an On-device tile: open the stored pattern in the editor. NO
   * device write — the LEDs keep doing whatever they were doing, a playlist
   * keeps playing, and the editor is in local preview unless this pattern
   * happens to be the one already running (#563).
   */
  export async function openDevicePattern(id: string): Promise<void> {
    preview?.clear();
    patternLoading = true; // cover the editor until the source is fetched
    try {
      const running = $deviceRunningId === id;
      await loadDevicePattern(id);
      livePush.set(running); // editing the running pattern IS the live case
      recompile(); // build the local preview (never a push: see devicePush)
    } finally {
      patternLoading = false;
    }
  }

  /**
   * `▶ Play on device` / a tile's `Play`: hand a stored pattern to the LEDs
   * and adopt it as the editor's document, so the running marker, the editor
   * and the device agree — and live push resumes from here.
   */
  export async function playDevicePattern(id: string): Promise<void> {
    preview?.clear();
    patternLoading = true;
    try {
      await loadDevicePattern(id);
      recompile(); // local preview of the pattern we are about to run
      await activateStored(id); // a refusal leaves the editor in local preview
    } finally {
      patternLoading = false;
    }
  }

  /** Activate a stored pattern, healing a stale-bytecode rejection. Returns
   *  whether the device is now running it (and `livePush` set to match). */
  async function activateStored(id: string): Promise<boolean> {
    const d = $device;
    if (!d) return false;
    let r = await activateDevicePattern(id);
    if (!r.ok && r.code === "bc-version") {
      // the stored bytecode predates a firmware format bump (the device
      // can't recompile — it has no compiler): recompile from the stored
      // source, re-save, and retry once
      const bc = compileToBytecode($source);
      if (bc) {
        await d.savePattern($patternName, $source, bc);
        r = await activateDevicePattern(id);
      }
    }
    if (!r.ok) {
      deviceError.set(`activate failed: ${r.error}`);
      livePush.set(false);
      return false;
    }
    livePush.set(true);
    return true;
  }

  /** Fetch a stored pattern into the document. Activation is the CALLER's
   *  decision (#563) — this only reads. */
  async function loadDevicePattern(id: string): Promise<void> {
    const d = $device;
    if (!d) return;
    try {
      const p = await d.patternSource(id);
      patternName.set(p.name);
      exampleName.set("");
      source.set(p.source);
      resetDocumentState(); // drops live push; the caller re-establishes it
      devicePatternId.set(id);
      dirty.set(false); // freshly loaded from the device — matches the stored copy
      compileError = null;
      // controls come from the local recompile the caller runs next
    } catch (e) {
      deviceError.set(`cannot load pattern: ${String(e)}`);
    }
  }

  /**
   * The editor header's `▶ Play on device` (§5.2, #563): the one explicit
   * "put this on the LEDs" verb. A pattern the device already stores is
   * activated by id; anything else (a Library pick, an unsaved edit) is SAVED
   * first, because the device can only run what it holds — which is also what
   * gives it a row in `On device` to come back to.
   */
  async function playOnDevice(): Promise<void> {
    if (!$device) return;
    if ($dirty || !$devicePatternId) {
      await saveCurrent();
      if ($dirty || !$devicePatternId) return; // save refused (no name, no compile)
    }
    if (await activateStored($devicePatternId)) note("save", "playing on the device", 2500);
  }

  // The device streams only source, not which library entry it came from — so
  // a freshly-opened running pattern shows as "untitled". If its source matches
  // a saved device pattern, adopt that name/id (so the header isn't "untitled"
  // and Add-to-playlist works). Runs as device pattern sources stream in.
  // deps passed as args so Svelte tracks devicePatterns (source-fill re-runs it)
  // NOT gated on `active`: since the console opens on the Patterns page
  // (#538) rather than in the editor, this match is what lights the running
  // tile there — it has to happen whether or not the editor is on screen.
  $: matchRunningToLibrary($devicePatterns, $source, $dirty, $devicePatternId, $device, $livePush);
  function matchRunningToLibrary(
    pats: { id: string; name: string; source?: string }[],
    src: string,
    drt: boolean,
    dpid: string,
    dev: unknown,
    live: boolean,
  ): void {
    if (!dev || drt || dpid || !src) return;
    const m = pats.find((p) => p.source && p.source.trim() === src.trim());
    if (m) {
      patternName.set(m.name);
      devicePatternId.set(m.id);
      // While the editor IS the running program, naming the document also
      // names what the device is playing — that is what lights the ring on
      // the Patterns page (#563 split the two ids apart).
      if (live) deviceRunningId.set(m.id);
    }
  }

  // ---- .epe import / export ----

  export async function importEpeFile(file: File): Promise<void> {
    importError = "";
    try {
      const epe = await parseEpe(file);
      patternName.set(epe.name);
      exampleName.set("");
      devicePatternId.set("");
      source.set(epe.source);
      resetDocumentState();
      dirty.set(true); // an imported .epe isn't in the library/device until saved
      dispatch("open"); // a dropped/imported .epe opens straight in the editor
      preview?.clear(); // fresh waterfall for the imported pattern
      await tick();
      applyEdit();
    } catch (e) {
      importError = `.epe import failed: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function onImportPick(e: Event): void {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) void importEpeFile(f);
    input.value = ""; // allow re-importing the same file
  }

  function doExportEpe(): void {
    exportEpe($patternName || $exampleName || "luxel pattern", $source);
  }

  // ---- the ⋯ menu's document verbs ----

  /** Detach the working copy from whatever it was stored as, under a new
   *  name — the shortcut for "start from this one". Nothing is written until
   *  Save, so a duplicate that is abandoned leaves no trace. */
  function duplicate(): void {
    const base = nameOf($patternName, $exampleName);
    patternName.set(`${base} copy`);
    exampleName.set("");
    devicePatternId.set("");
    dirty.set(true);
    // the copy is a new document the device has never seen (#563)
    livePush.set(false);
    note("save", "duplicated — Save to store it", 2500);
  }

  /** Save the working copy under the name in the header. The name is the
   *  header's (A7, #468) — there is no naming prompt any more; an unnamed
   *  pattern opens the inline editor with the reason, which is the same
   *  "refuse in place, disable nothing" rule the dialog used. */
  async function saveCurrent(): Promise<void> {
    const name = ($patternName || $exampleName).trim();
    if (name === "") {
      startRename("name this pattern before saving");
      return;
    }
    if ($device) {
      const bc = compileToBytecode($source);
      if (!bc) {
        note("save", "save failed: pattern does not compile", 3000);
        return;
      }
      // A save under an existing name OVERWRITES that row, so its cached
      // source is stale and must be re-read; every other row keeps its
      // engine (#538 round 2, refreshDevicePatterns).
      const overwritten = $devicePatterns.find((p) => p.name === name)?.id;
      const r = await $device?.savePattern(name, $source, bc);
      if (r?.ok) {
        patternName.set(name);
        exampleName.set("");
        dirty.set(false); // now stored on the device
        note("save", "saved to device", 3000);
        await refreshDevicePatterns(overwritten ? [overwritten] : []);
        // The id is what makes this an ON-DEVICE document — it is what "Add
        // to playlist" and "Delete" in the menu act on, and what the save
        // state reads. An overwrite (and older firmware) answers with no
        // `id` at all, which used to leave the editor stuck in its library
        // state after a library→device save (audit E6, Jeremy 2026-09-19).
        // The freshly-read list is the fallback: names are unique on the
        // device, because saving the same name overwrites.
        const id =
          r.id && r.id !== "" ? r.id : ($devicePatterns.find((p) => p.name === name)?.id ?? "");
        devicePatternId.set(id);
        // Saving never ACTIVATES (#563) — `▶ Play on device` does. The one
        // exception is bookkeeping: while the editor already drives the
        // device, the freshly stored row IS what is playing, so the Patterns
        // page's ring follows it instead of staying on an ad-hoc blank.
        if ($livePush && id) deviceRunningId.set(id);
      } else {
        note("save", r && "error" in r ? `save failed: ${r.error}` : "save failed", 3000);
      }
      return;
    }
    saveToLocalLibrary(name, $source);
    patternName.set(name);
    exampleName.set("");
    dirty.set(false); // now stored in the library
    note("save", "saved", 2000);
  }

  async function deleteSaved(): Promise<void> {
    if ($device && $devicePatternId) {
      const id = $devicePatternId; // fix the target before we await the dialog
      const ok = await confirm({
        title: "Delete pattern from the device?",
        body: `"${$patternName}" is removed from the device's stored library. This cannot be undone.`,
        confirmLabel: "Delete",
        danger: true,
      });
      if (!ok) return;
      await $device?.deletePattern(id);
      devicePatternId.set("");
      if ($deviceRunningId === id) deviceRunningId.set(""); // no row to ring any more
      note("save", "deleted from device", 2000);
      await refreshDevicePatterns();
      return;
    }
    if (!$patternName || !$saved.some((s) => s.name === $patternName)) return;
    const ok = await confirm({
      title: "Delete pattern from the library?",
      body: `"${$patternName}" is removed from this browser's library. This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
    });
    if (!ok) return;
    deleteFromLocalLibrary($patternName);
    note("save", "deleted", 2000);
  }

  /** Append the CURRENT editor pattern (must be saved on the device) with its
   *  current control values — so the same pattern can be added repeatedly with
   *  different params. */
  function addToPlaylist(): void {
    if (!$device || !$devicePatternId) return;
    // One path for every "Add to playlist" affordance (#470). The item takes
    // a snapshot of everything that is a VALUE here — the sliders AND the
    // projection you chose for this pattern — because the playlist item is
    // where those get their durable home (stores/pattern.ts).
    addPatternToPlaylist($devicePatternId, { ...$controlValues }, $projectionOverride ?? undefined);
    note("save", "added to playlist", 2000);
  }

  // ---- share links ----

  async function sharePattern(): Promise<void> {
    // The pattern only: a map is the Layout's, not the pattern's (#463).
    const frag = await encodeShare($source);
    history.replaceState(null, "", `#${frag}`);
    const url = location.href;
    try {
      await navigator.clipboard.writeText(url);
      note("share", "link copied", 2500);
    } catch {
      // clipboard needs a secure context — a device over plain http isn't.
      // The dialog's field opens selected, so ⌘/Ctrl-C still works.
      await promptText({
        title: "Copy this link",
        body: "Your browser blocked the clipboard on this page. The link is selected below — copy it, or copy it from the address bar.",
        label: "Link",
        initial: url,
        confirmLabel: "Done",
        cancelLabel: "Close",
        validate: () => null,
      });
      note("share", "link in address bar", 2500);
    }
  }

  // ---- transport (the preview panel's own header) ----

  /** The rate the preview header states. A console runs TWO loops — this
   *  tab's preview and the device's own render — and Jeremy asked to see
   *  both, because the device's number is already in the shell header and
   *  the local one is what tells you whether the browser is the bottleneck
   *  (audit E9, 2026-09-19). The device figure is `out_fps` on a pipelined
   *  HUB75 board (frames the panel actually displayed), else `fps`. The
   *  playground has only the local loop. */
  $: localRate = `${$previewFps.toFixed(0)} fps`;
  $: previewRate = !$device
    ? { text: localRate, title: "local preview loop in this browser tab" }
    : {
        // `60/92 fps` — the RATES first and the layout after them, because
        // this line lives in a 360px rail beside the transport and the tail
        // is what an ellipsis takes. The layout is on the header chip too.
        text: `${$previewFps.toFixed(0)}/${$deviceOutFps > 0 ? $deviceOutFps : $deviceFps} fps`,
        title:
          ($deviceOutFps > 0
            ? `${$deviceOutFps} fps displayed by the panel (out_fps)` +
              ($deviceRescanHz ? `, panel rescan ${$deviceRescanHz} Hz` : "") +
              `, device render loop ${$deviceFps} fps`
            : `${$deviceFps} fps rendered by the device`) +
          ` — local preview loop ${$previewFps.toFixed(0)} fps in this browser tab, ${$layoutName}`,
      };

  function onFpsChange(e: Event): void {
    targetFps = Number((e.target as HTMLSelectElement).value);
  }

  function togglePause(): void {
    running = !running;
    lastT = 0; // don't integrate the paused time into delta
  }

  function onControlSet(e: CustomEvent<{ name: string; values: number[] }>): void {
    engine?.setControl(e.detail.name, e.detail.values); // drives the local preview
    // …and the strip, but only while the strip is running THIS pattern: a
    // slider moved in local preview would otherwise re-colour whatever the
    // device happens to be playing (#563).
    if ($device && $livePush) void $device.setControl(e.detail.name, e.detail.values);
  }

  // ---- debugger ----

  /** Install breakpoints and echo the VM-resolved lines back into the
   *  gutter — a click on a comment/blank line snaps to the next executable
   *  line, so the dot always sits where execution will actually stop. */
  function applyBreakpoints(): void {
    if (!engine) return;
    const resolved = [...new Set(engine.setBreakpoints(breakpoints))].sort((a, b) => a - b);
    const current = [...new Set(breakpoints)].sort((a, b) => a - b);
    if (JSON.stringify(resolved) !== JSON.stringify(current)) {
      breakpoints = resolved;
      editor?.setBreakpointLines(resolved);
    }
  }

  function onBreakpoints(e: CustomEvent<number[]>): void {
    breakpoints = e.detail;
    // The device has no stepping debugger — breakpoints must not arm debug
    // mode while connected (it would show a UI that can't actually break).
    if ($device) return;
    if (breakpoints.length > 0 && !debugMode) {
      toggleDebug(); // placing a breakpoint arms the debugger
    } else if (debugMode) {
      applyBreakpoints();
    }
  }

  function toggleDebug(): void {
    debugMode = !debugMode;
    if (!engine) return;
    engine.debugEnable(debugMode);
    if (debugMode) {
      applyBreakpoints();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
    }
  }

  function onPausedRefresh(): void {
    if (!engine) return;
    dbg = engine.debugState();
    editor?.setCurrentLine(dbg.line ?? null);
    preview?.draw(engine.pixels());
  }

  function step(kind: StepKind): void {
    if (!engine || !dbg.paused) return;
    const still = engine.debugStep(kind);
    if (still) {
      onPausedRefresh();
    } else {
      dbg = { paused: false };
      editor?.setCurrentLine(null);
      preview?.draw(engine.pixels());
    }
  }

  function requestBreak(): void {
    engine?.debugPause(); // takes effect at the next executed instruction
  }

  function fmtRaw(raw: number): string {
    return (raw / 65536).toFixed(4).replace(/\.?0+$/, "") || "0";
  }

  function fmtLocal(l: { raw?: number; array?: number; fn?: number }): string {
    if (l.raw !== undefined) return fmtRaw(l.raw);
    if (l.array !== undefined) return `array[${l.array}]`;
    return `fn#${l.fn}`;
  }

  /** Hover inspection: paused → locals (shadowing globals) then globals;
   *  running → live globals. */
  function hoverValue(name: string): string | null {
    if ($device) {
      const v = vars[name];
      return typeof v === "number" ? v.toFixed(4).replace(/\.?0+$/, "") || "0" : null;
    }
    if (!engine) return null;
    if (dbg.paused) {
      const local = dbg.stack?.[0]?.locals.find((l) => l.name === name);
      if (local) return fmtLocal(local);
      const g = dbg.globals?.find((g) => g.name === name);
      return g ? fmtLocal(g) : null;
    }
    const g = engine.globals().find((g) => g.name === name);
    return g ? fmtLocal(g) : null;
  }

  /** Diagnostic spans are UTF-8 byte offsets; CodeMirror wants char offsets. */
  function byteToChar(text: string, byte: number): number {
    const bytes = new TextEncoder().encode(text);
    return new TextDecoder().decode(bytes.subarray(0, Math.min(byte, bytes.length))).length;
  }

  // keep the squiggle + gutter dot in sync with the compile status
  $: if (editor) {
    if (compileError && compileError.start !== undefined && compileError.end !== undefined) {
      editor.setErrorRange({
        from: byteToChar($source, compileError.start),
        to: byteToChar($source, compileError.end),
        message: compileError.message,
      });
    } else {
      editor.setErrorRange(null);
    }
  }

  function jumpToError(): void {
    if (compileError) editor.jumpTo(compileError.line, compileError.col);
  }

  // ---- microphone → sensor patterns (frequencyData etc.) ----
  const mic = new MicSource();
  let micOn = false;
  let sensorInFlight = false;
  let lastSensorPush = 0;

  function toggleMic(): void {
    if (micOn) {
      mic.stop();
      micOn = false;
      return;
    }
    void (async () => {
      try {
        await mic.start();
        micOn = true;
        clearNote("mic");
      } catch {
        note("mic", "microphone unavailable", 4000);
      }
    })();
  }

  // ---- external event injection (preview clicks → readEvent patterns) ----

  const EV_POINTER = 1; // event type carried in [type, x, y, value]
  let evQueue: [number, number, number, number][] = [];
  let evFlushTimer: ReturnType<typeof setTimeout> | null = null;

  function onInject(e: CustomEvent<{ x: number; y: number }>): void {
    engine?.pushEvent(EV_POINTER, e.detail.x, e.detail.y, 1);
    if (!$device || !$livePush) return; // local preview writes nothing (#563)
    // batch drags into one POST per ~50 ms instead of a request per move
    evQueue.push([EV_POINTER, e.detail.x, e.detail.y, 1]);
    if (!evFlushTimer) evFlushTimer = setTimeout(flushEvents, 50);
  }

  function flushEvents(): void {
    evFlushTimer = null;
    const batch = evQueue;
    evQueue = [];
    if ($device && batch.length) void $device.sendEvents(batch).catch(() => {});
  }

  // ---- pin injection (pin panel → digitalRead / analogRead) ----

  /** Re-read which pins the pattern touches and what they currently read.
   *  Polled rather than computed once: a `pinMode` at top level shows up at
   *  compile time, but a pin only ever named in `digitalRead` inside
   *  `beforeRender` isn't known until the pattern has run a frame. */
  function refreshPins(): void {
    if (!engine) {
      pins = [];
      return;
    }
    const next = engine.pinsUsed();
    // reassign only on a real change — `pins` keys an `{#each}`
    if (next.length !== pins.length || next.some((p, i) => p !== pins[i])) pins = next;
    const levels: Record<number, boolean> = {};
    const idle: Record<number, boolean> = {};
    for (const p of pins) {
      levels[p] = engine.pinRead(p);
      idle[p] = engine.pinIdleHigh(p);
    }
    pinLevels = levels;
    pinIdleHigh = idle;

    const nextAnalog = engine.analogPinsUsed();
    if (nextAnalog.length !== analogPins.length || nextAnalog.some((p, i) => p !== analogPins[i])) {
      analogPins = nextAnalog;
    }
    // The engine is the source of truth for a slider's position too — a pin
    // the pattern only just started sampling has never been driven, so it
    // reads 0 and the slider must show 0 rather than a stale value.
    const analog: Record<number, number> = {};
    for (const p of analogPins) analog[p] = engine.analogRead(p);
    analogValues = analog;
  }

  /** A press/latch from the pin panel. `level: null` releases the pin back to
   *  its idle level (HIGH under a pull-up) — the injection ABI's driven-vs-idle
   *  model, not a plain HIGH/LOW toggle. Preview-only by design: on a device
   *  the same pins are real pads the firmware syncs every frame (Gitea #177
   *  item 4), so an injected level would be overwritten by the wire. */
  function onPinDrive(e: CustomEvent<{ pin: number; level: boolean | null }>): void {
    engine?.setPin(e.detail.pin, e.detail.level);
    refreshPins();
  }

  /** A slider move from the pin panel: park the analog pin at `value` (0..1),
   *  which is what `analogRead`/`touchRead` then report until it is moved
   *  again (Gitea #206). Preview-only for the same reason as `onPinDrive`. */
  function onAnalogDrive(e: CustomEvent<{ pin: number; value: number }>): void {
    engine?.setAnalogPin(e.detail.pin, e.detail.value);
    refreshPins();
  }

  // ---- render loop ----

  function loop(t: number): void {
    raf = requestAnimationFrame(loop);
    if (!engine || !running) return;
    if (dbg.paused) return; // suspended at a debug stop — step buttons drive
    const minInterval = targetFps > 0 ? 1000 / targetFps - 1 : 0;
    if (lastT !== 0 && t - lastT < minInterval) return;
    const dt = lastT === 0 ? 1000 / (targetFps || 60) : Math.min(t - lastT, 200);
    lastT = t;
    if (micOn) {
      const sf = mic.frame();
      engine.setSensors(sf);
      // in device mode, the mic also stands in for the physical sensor
      // board: stream frames to the strip (throttled, one in flight) — but
      // only while the device is running THIS pattern (#563)
      const d = $livePush ? $device : null;
      if (d && !sensorInFlight && t - lastSensorPush > 50) {
        lastSensorPush = t;
        sensorInFlight = true;
        d.sendSensors(toSensorBoardFrame(sf))
          .catch(() => {})
          .finally(() => (sensorInFlight = false));
      }
    }
    const px = engine.frame(dt);
    if (engine.debugPaused()) {
      onPausedRefresh();
      return;
    }
    // The console draws what the WIRE would carry: the device output chain
    // (palette, blur, glow, colour order, gamma, power cap) over the engine's
    // frame — the same `luxel_core::outpipe::DeviceChain` the firmware runs
    // (#466). The playground has no device chain, so its raw frame IS the
    // finished frame.
    preview?.draw($device ? engine.outpipe() : px);
    previewFps.update((f) => f * 0.9 + (1000 / Math.max(dt, 1)) * 0.1);
    const err = engine.takeError();
    if (err) runtimeError.set(err);
    if (t - lastPoll > 250) {
      lastPoll = t;
      vars = engine.vars();
      const r = new Map<string, number>();
      for (const c of controls) {
        if (c.kind === "showNumber" || c.kind === "gauge") {
          const v = engine.setControl(c.name, []);
          if (v !== null) r.set(c.name, v);
        }
      }
      readouts = r;
      refreshPins();
    }
  }

  /** Editor keyboard shortcuts: ⌘/Ctrl+S save, ⌘/Ctrl+Enter force run + push. */
  function onKeydown(e: KeyboardEvent): void {
    if (!active) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void saveCurrent();
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      applyEdit(); // recompile the preview + push to the device
    }
  }

  export function clearPreview(): void {
    preview?.clear();
  }

  onDestroy(() => {
    cancelAnimationFrame(raf);
    clearTimeout(debounce);
    clearTimeout(pushDebounce);
    engine?.free();
    mic.stop();
  });
</script>

<svelte:window on:keydown={onKeydown} />

<main class="editor-view editor-frame" data-role="editor-view" hidden={!active}>
  {#if patternLoading}
    <!-- cover the editor while a pattern is being fetched/activated so the
         previously-open script never flashes before the real one loads -->
    <div class="pattern-loading" data-role="pattern-loading">
      <span class="spinner"></span>
      {$device ? "loading the pattern from the device…" : "loading pattern…"}
    </div>
  {/if}

  <!-- ── the header owns the DOCUMENT (proposal §5.2, mockup S2) ──
       back · inline-editable name · save state · one primary action · the ⋯
       menu of document verbs. No geometry, no transport, no sub-tabs. -->
  <header class="editor-header" data-role="editor-header">
    <button
      data-role="editor-back"
      class="btn quiet back"
      title={`back to ${backLabel}`}
      on:click={() => dispatch("back")}
    >
      <span class="backglyph" aria-hidden="true">←</span>
      <span class="backlabel">{backLabel}</span>
    </button>

    {#if editingName}
      <input
        class="nameedit"
        data-role="name-input"
        bind:this={nameInput}
        bind:value={nameDraft}
        aria-label="pattern name"
        on:keydown={onNameKey}
        on:blur={commitName}
        on:click|stopPropagation
      />
    {:else}
      <button
        class="nameedit"
        data-role="pattern-name"
        title="click to rename"
        on:click|stopPropagation={() => startRename()}
      >
        <!-- the clamp is on an INNER span: mockup `.nameedit` neither wraps
             nor clips, and this element is measured against it -->
        <span class="nametext">{nameOf($patternName, $exampleName)}</span>
      </button>
    {/if}
    {#if nameError}<span class="name-error" data-role="name-error">{nameError}</span>{/if}

    <span class="savestate" data-role="save-state">
      {saveStateOf($dirty, $device, $devicePatternId, $patternName, $saved, $livePush)}
    </span>

    <span class="spacer"></span>

    {#if $notes.save}<span class="dim note" data-role="save-note">{$notes.save}</span>{/if}
    {#if $notes.share}<span class="dim note" data-role="share-note">{$notes.share}</span>{/if}

    <!-- The explicit "put this on the LEDs" verb, and the ONLY thing in the
         editor that changes what the device is playing while the document
         is not already the running one (#563). Absent once it IS — there is
         then nothing to play, exactly as on the playing tile (#555) — and
         absent in the playground, which has no device. -->
    {#if $device && !$livePush}
      <button
        class="btn"
        data-role="editor-play-device"
        title="save this pattern to the device and run it on the LEDs"
        on:click={() => void playOnDevice()}
      >
        ▶ Play on device
      </button>
    {/if}

    <!-- one word, in both modes: WHERE it lands is the save state's job,
         not the button's (audit E2 — "Save to device" was the wrong text) -->
    <button
      class="btn primary"
      data-role="save"
      title={$device ? "store this pattern on the device" : "store this pattern in this browser"}
      on:click={() => void saveCurrent()}
    >
      Save
    </button>

    <span class="overflow">
      <button
        class="btn icon"
        bind:this={moreBtn}
        data-role="overflow"
        title="more actions"
        aria-label="more actions"
        on:click={() => (menuOpen = !menuOpen)}
      >
        ⋯
      </button>
      <Popover
        open={menuOpen}
        anchor={moreBtn}
        dataRole="editor-menu"
        on:close={() => (menuOpen = false)}
      >
        <!-- "Add to scene ▸" belongs here (proposal §5.4b) and is absent
             until scenes exist — Phase B, Gitea #480. Not rendered rather
             than rendered-disabled: a control is absent unless the thing it
             acts on exists (§5.7). -->
        {#if $device && $devicePatternId}
          <button
            class="mi"
            data-role="add-to-playlist"
            role="menuitem"
            title="add this pattern, with its current values, to the playlist"
            on:click={addToPlaylist}
          >
            Add to playlist
          </button>
          <div class="sepr"></div>
        {/if}
        <!-- the mock's second group: what you can do to the DOCUMENT itself -->
        <button class="mi" data-role="duplicate" role="menuitem" on:click={duplicate}>Duplicate</button>
        <button class="mi" data-role="epe-export" role="menuitem" on:click={doExportEpe}>Export .epe</button>
        <button class="mi" data-role="epe-import" role="menuitem" on:click={() => fileInput.click()}>
          Import .epe…
        </button>
        {#if $isPlayground}
          <!-- the mock has no Share (it is a console frame); a share link is
               a document verb, so it joins the document group -->
          <button
            class="mi"
            data-role="share"
            role="menuitem"
            title="copy a link that carries this pattern in the URL"
            on:click={() => void sharePattern()}
          >
            Share…
          </button>
        {/if}
        {#if canDeleteNow($device, $devicePatternId, $exampleName, $patternName, $saved)}
          <div class="sepr"></div>
          <button class="mi del" data-role="delete" role="menuitem" on:click={() => void deleteSaved()}>
            Delete
          </button>
        {/if}
      </Popover>
    </span>

    <input
      class="file-input"
      type="file"
      accept=".epe,.json,application/json"
      bind:this={fileInput}
      on:change={onImportPick}
    />

    <!-- WHAT this screen renders through, over the rail it describes. The
         shell header is not rendered over an editor screen (#538), so its rig
         control travels with it — mockup S2 puts the device chip in the
         header's own rail column. A playground has no device, so it carries
         the "Preview as" chooser here instead — the chip IS the playground's
         rig, which is why only the console half is dropped on a phone. -->
    <span class="edhdr-rail" class:statusonly={!$isPlayground}>
      {#if $isPlayground}
        <PreviewAsChip on:openmap={() => dispatch("openmap")} />
      {:else}
        <DeviceChip />
      {/if}
    </span>
  </header>

  <!-- The body is the mock's `.edbody`: code left, rail right, and the two
       stacked with the rail first on a phone (S2b). -->
  <div class="edbody" data-role="editor-body">
    <!-- ── the code column holds only code, and owns its own errors ── -->
    <section class="left">
      <!-- S2b: on a phone the code column wears the same section header the
           rail sections do, and its `.rdim` says why it is read-only. -->
      <div class="code-head">
        <span class="slabel">Code</span>
        <span class="rdim code-hint">edit on a larger screen to change code</span>
      </div>
      <div class="editor-host">
        <div class="editor-slot">
          <CodeEditor
            bind:this={editor}
            value={$source}
            {hoverValue}
            on:change={onSourceChange}
            on:breakpoints={onBreakpoints}
          />
        </div>
      </div>

      <!-- The status strip pinned to the bottom of the pane: one plain sentence
           about the line the gutter dot and the squiggle already point at
           (proposal §5.2). Compile first, then the runtime error — never both,
           and never a banner across the page from the cause. -->
      {#if compileError}
        <button class="codestatus err" data-role="compile-error" on:click={jumpToError}>
          ✗ line {compileError.line} · {compileError.message}
          <span class="jump">jump to line</span>
        </button>
      {:else if $runtimeError}
        <div class="codestatus warn" data-role="runtime-error">
          ⚠ runtime · {$runtimeError.message}
          <button class="dismiss" title="dismiss" on:click={() => runtimeError.set(null)}>×</button>
        </div>
      {/if}
    </section>

    <section class="right">
      <div class="railscroll">
        <!-- Conditions that persist until whatever caused them goes away: the
             wasm failed to load, the device is unreachable. Pushed by whoever
             knows (the shell, the push path) rather than derived here. Compile and
             runtime errors are NOT here any more — they belong to the code pane. -->
        {#each $banners as b (b.id)}
          <div class="banner" class:error={b.level === "error"} class:warn={b.level === "warn"} data-role={b.role}>
            {b.text}
          </div>
        {/each}
        {#if importError}
          <div class="banner error" data-role="import-error">
            {importError}
            <button class="dismiss" on:click={() => (importError = "")}>×</button>
          </div>
        {/if}

        {#if debugMode}
          <div class="rsec">
            <Debugger snapshot={dbg} on:step={(e) => step(e.detail)} on:break={requestBreak} />
          </div>
        {/if}

        <!-- ── Preview: its header owns the transport (mockup S2) ── -->
        <div class="rsec">
          <div class="rhead">
            <span class="slabel">Preview</span>
            <span class="rdim" data-role="preview-dims" title={previewRate.title}>
              {previewRate.text} · {$layoutName}
            </span>
            <!-- transport order (audit E7/E8): pause, then Debug beside it — the
                 two things you reach for while writing a frame — then the rate.
                 The pause box is the global 26 px `.btn.sm.icon`; it used to be a
                 padding-only button around inherited-size text, which is the
                 oversized glyph Jeremy flagged. -->
            <span class="grp">
              <button
                class="btn sm icon glyph"
                data-role="pause"
                title={running ? "pause the preview" : "resume the preview"}
                aria-label={running ? "pause" : "play"}
                on:click={togglePause}
              >
                {running ? "‖" : "▶"}
              </button>
              <!-- the preview runs on the local engine (even on a device), so the
                   step-debugger works everywhere. Labelled, not icon-only: the bug
                   glyph alone did not read as "debugger" (Jeremy, 2026-09-19). -->
              <button
                class="btn sm"
                class:active={debugMode}
                data-role="debug"
                title="toggle the step debugger"
                on:click={toggleDebug}
              >
                Debug
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                  <rect x="8" y="7" width="8" height="12" rx="4" />
                  <path d="M4 10h4M16 10h4M4 16h4M16 16h4M9.5 5l1 2M14.5 5l-1 2" />
                </svg>
              </button>
              {#if wantsSensors}
                <!-- ONLY when the pattern binds sensor variables (§5.7) — a
                     pattern that reads no audio has no use for a microphone.
                     Inline SVG rather than a glyph: a headless chromium without
                     a symbol font draws ♪ and ⏿ as tofu (seen in the e2e shots). -->
                <button
                  class="btn sm icon"
                  class:active={micOn}
                  data-role="mic-toggle"
                  title="feed microphone audio to sensor patterns (frequencyData, energyAverage, maxFrequency)"
                  aria-label="microphone"
                  on:click={toggleMic}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                    <rect x="9" y="3" width="6" height="11" rx="3" />
                    <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
                  </svg>
                </button>
              {/if}
              <select
                class="sel"
                data-role="target-fps"
                value={targetFps}
                title="preview frame rate"
                aria-label="preview frame rate"
                on:change={onFpsChange}
              >
                <option value={0}>max fps</option>
                <option value={60}>60 fps</option>
                <option value={30}>30 fps</option>
                <option value={15}>15 fps</option>
                <option value={5}>5 fps</option>
              </select>
            </span>
          </div>
          <div class="preview-wrap">
            <Preview bind:this={preview} layout={$layout} on:inject={onInject} />
          </div>
          {#if $notes.mic}<p class="note-error" data-role="mic-error">{$notes.mic}</p>{/if}

          <!-- Capacity (Gitea #15), the existing idiom in its new place: a strip
               under the preview it is about. Severity follows CERTAINTY, not size:
               the device's own rejection is a fact and reads as an error; our local
               model is advice and reads as a warning. Both are non-blocking — the
               pattern keeps previewing locally either way. -->
          {#if deviceRejectedForSize}
            <div class="capstrip err" data-role="capacity-rejected">
              the device rejected this pattern: {deviceRejectedForSize}
            </div>
          {:else if capacity}
            <div
              class="capstrip"
              class:capacity-over={capacity.level === "over"}
              data-role="capacity-warning"
              data-level={capacity.level}
              title={capacity.detail}
            >
              {capacity.level === "over" ? "⚠" : "△"}
              {capacity.text}
            </div>
          {/if}
        </div>

        <!-- ── Controls, then the quiet Projection row (S2c/S2d) ── -->
        <div class="rsec">
          <div class="rhead">
            <span class="slabel">Controls</span>
            <!-- S2c/S2d: the section says WHOSE values these are and what
                 dimensionality they were written for — the one fact the
                 projection row below is about. -->
            <span class="rdim" data-role="controls-dims">
              {nameOf($patternName, $exampleName)} · {dimsLabel($patternDims)}
            </span>
          </div>
          <Controls {controls} bind:values={$controlValues} {readouts} hints={$hints} on:set={onControlSet} />
          {#if controls.length === 0}
            <p class="dim hint">
              export <code>function sliderName(v)</code> to add controls — bound them with
              <code>//# min=0 max=5 step=0.5 default=2</code>
            </p>
          {/if}
          <ProjectionRow
            patternDims={$patternDims}
            layout={$layout}
            override={$projectionOverride}
            on:set={onProjectionSet}
          />
        </div>

        <!-- VARS is absent entirely for a pattern that exports none (§5.7). -->
        {#if Object.keys(vars).length > 0}
          <div class="rsec" data-role="vars-section">
            <div class="rhead"><span class="slabel">Vars ({Object.keys(vars).length})</span></div>
            <VarWatcher {vars} />
          </div>
        {/if}

        {#if pins.length > 0 || analogPins.length > 0}
          <div class="rsec">
            <div class="rhead"><span class="slabel">Pins</span></div>
            <PinPanel
              {pins}
              {analogPins}
              levels={pinLevels}
              idleHigh={pinIdleHigh}
              bind:latched={pinLatched}
              bind:analogValues
              on:drive={onPinDrive}
              on:analog={onAnalogDrive}
            />
            <p class="dim hint">
              {#if pins.length > 0}<code>press</code> drives the pin while held; <code>hold</code> keeps
                it driven after you let go. Releasing both returns the pin to its
                <code>pinMode</code> idle level.{/if}{#if analogPins.length > 0}{" "}
                An analog slider is what <code>analogRead</code>/<code>touchRead</code> read on that pin,
                0..1 — it stays where you leave it.{/if}{#if $device}{" "}
                Drives the local preview only: on the device these pins are real GPIO, read from and
                written to the pads every frame.{/if}
            </p>
          </div>
        {/if}

      </div>
    </section>
  </div>
</main>

<style>
  /* The frame — the grid, the header, the code column, the rail and the
     phone stacking — is components/editor-frame.css, shared with the map
     program's screen (A10, #471). Only what is this editor's own is here. */

  .pattern-loading {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-dim);
    background: var(--bg);
  }

  .dim {
    color: var(--text-dim);
  }

  /* The name field (`.nameedit`), the header split and the transport boxes
     are components/editor-frame.css — the map screen wears the same chrome. */

  .name-error {
    color: var(--error);
    font-size: 12px;
  }

  /* the capacity idiom, kept: certainty-graded and never blocking */
  .capstrip {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    padding: 7px 9px;
    border: 1px solid var(--warn);
    border-radius: 6px;
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    color: #ecd9a8;
    font-size: 12px;
  }

  /* "will not fit" vs "getting close" — same amber family (both are
     predictions, not facts; the device's own rejection is the red one),
     separated by weight rather than hue. */
  .capstrip.capacity-over {
    background: color-mix(in srgb, var(--warn) 24%, transparent);
    font-weight: 600;
  }

  .capstrip.err {
    border-color: var(--error);
    background: color-mix(in srgb, var(--error) 18%, transparent);
    color: #f2b8b8;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
  }

  .note-error {
    margin: 8px 0 0;
    color: var(--error);
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .preview-wrap {
    position: relative;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: conn-spin 0.7s linear infinite;
  }

  @keyframes conn-spin {
    to {
      transform: rotate(1turn);
    }
  }

  /* ---- phone (D9) ---- the frame does the stacking and the header row
     (components/editor-frame.css); this is the back button's share of it:
     S2b's back is icon-only, because a 390 px header has no room for a
     destination name it is about to show you anyway. */
  @media (max-width: 600px) {
    .back .backlabel {
      display: none;
    }

    .back {
      width: 32px;
      padding: 0;
    }
  }
</style>

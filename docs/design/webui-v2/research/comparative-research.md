# Comparative research: layered compositing + text for the Luxel v2 web UI

Research date 2026-09-18. Public sources only. No repo changes.

---

## A. PatternFlow — found, and why it feels complex

**Identity (high confidence).** *Patternflow*, an open-source "LED synthesizer" by SeungHun Lee (GitHub `engmung`). Repo [engmung/Patternflow](https://github.com/engmung/Patternflow) (MIT firmware/web, CC-BY-SA 4.0 hardware/docs); site [patternflow.work](https://patternflow.work/); [community](https://community.patternflow.work/community); [Hackaday](https://hackaday.io/project/206054-patternflow); [Crowd Supply](https://www.crowdsupply.com/engmung/patternflow) ($269, funded 2026-09-15); [Synthtopia](https://www.synthtopia.com/content/2026/08/30/patternflow-is-an-open-source-light-synthesizer-music-controller/). Not the TensorFlow-adjacent academic "PatternFlow". Hardware is close to Luxel's bench: **ESP32-S3-WROOM-1 N16R8** driving a **128×64 HUB75 P2.5** panel plus 4 EC11 encoders.

**IA — three separate UIs**, itself a complexity source:
1. **Marketing/tools site**: tabs *Build · Pattern · Inside*, plus `/pattern-lab`, `/community/**`, `/editions`, `/features`, `/flash`, `/roadmap` ([ARCHITECTURE.md](https://raw.githubusercontent.com/engmung/Patternflow/main/web/ARCHITECTURE.md)).
2. **Community site**: *Patterns · Decks · Workshop · Atlas*.
3. **On-device console** (`patternflow.local`): *Status · Patterns · Knobs · Wi-Fi · Update*, plus edition-gated *Audio*, *Clock*, *Sequences* ([rest-api.md](https://raw.githubusercontent.com/engmung/Patternflow/main/docs/rest-api.md)).

**Composition model.** Vocabulary: *layer / pattern / deck / show / module / ramp / cue / edition*. Pattern Lab v2 added **layers** in two flavours — **code layers** (JS `setup`/`update`/`draw`) and **pixel layers** (RGBA buffers, revision-versioned, edited in a sprite editor, convertible to code via `pixelToCode.ts`). Layers composite as a **stack**, ordering/visibility in Zustand store slices. **Each layer has its own colour ramp**; knob state is global. **No text objects, no named overlays.** Above the pattern: a **Deck** is a setlist of ≤20 patterns; a **module** is a `.pfm` binary; a **Show** is a `.pfs` cue table authored in a *Director* panel, exportable as 4 lanes of MIDI CC.

**Settings — three unrelated axes.** Pattern Lab panel settings (knob ranges, ramp blend modes, resolution, capture); device console tabs (Wi-Fi, knob edges-per-click, brightness, audio bands, NTP); and build-time **"Editions"** — Core/Audio/Performance/Clock ([EDITIONS.md](https://raw.githubusercontent.com/engmung/Patternflow/main/docs/EDITIONS.md)). MQTT/OSC/MIDI/weather are *compiled in*, not toggled.

**What makes it feel complex**
1. **Nine dockable panels** in Pattern Lab — Preview, Layers, Code, Knobs, Color Ramp, Pixel, Gallery, Graphic Export, Director — behind a bare `Panels ▾` dropdown with a persisted dock layout. An IDE shell, not a pattern editor.
2. **No progressive disclosure across the three UIs.** Getting one pattern to hardware runs Live Editor → Pattern Lab → `.h` C++ conversion → build service → deck → device console. The project's own guide concedes it "takes real patience."
3. **~13 nouns before an LED lights**: pattern, layer, ramp, knob lane, deck, module, header, show, cue, edition, composition, feature, variant.
4. **Authoring leans on AI** ("paste AI-generated code here"; the Gallery panel is a Gemini variant queue) — the primitives are never made simple.
5. **Editions-as-settings** means a capability change requires a reflash.

**Evidence is thin**: the only substantive usability complaint found is a Synthtopia comment — *"The software that I've seen in this area all have a pretty high learning curve"*. No UX-labelled GitHub issues, no Reddit usability threads. The owner's "horribly designed" is his own judgement, but the nine-panel IDE and 13-noun vocabulary corroborate it structurally. *Confidence:* identity/model — high; exact on-screen labels — medium (client-rendered SPA; [screenshots](https://github.com/engmung/Patternflow/tree/main/docs/images), walkthrough video Instagram-only).

**What this means for Luxel**
- The diagnosis is **concept count and surface count**, not feature count. Budget ~5 nouns: composite, layer, pattern, text, sprite. State the model in one sentence.
- **One UI, one URL.** The three-site split is the biggest avoidable error.
- **No build step.** Luxel's bytecode + OTA already beats the `.h` → build-service → `.pfm` → deck pipeline; don't regress toward compiled-in features.
- **No dockable-panel IDE.** Fixed layout: preview, layer list, selected-layer inspector.
- Steal **per-layer colour ramp** (makes a monochrome pattern reusable as a layer); reject global-knobs-across-layers.
- Patternflow has **no text layer at all** — compositing + a text builtin is differentiation, not catch-up.

---

## B. OBS Studio — the model users already know

Hierarchy: *Scene Collection* → **Scene** → **Source** → **Filter**.

- **Sources dock** is an **ordered list**, and the ordering rule is the whole model: *"A Source that is listed above another Source in the list will be on top and might hide what's beneath it"* ([Sources Guide](https://obsproject.com/kb/sources-guide)). Top = front. No z-index field; you drag rows.
- **Per-row, always visible:** an **eye** (visibility) and a **padlock** (lock). Two clicks, zero dialogs, for the two most frequent actions.
- **Direct manipulation:** selecting a source draws a red **bounding box** with handles; drag to move, handles to scale, **Alt+drag to crop**. Numeric fallback via right-click → Transform → **Edit Transform** (Ctrl+E): Position, Rotation, Size, Alignment, **Bounding Box Type**, Crop L/R/T/B.
- **Filters:** a per-source *ordered chain* (Crop/Pad, Chroma Key, Luma Key, Color Correction, Scroll…). Transform = where it sits; filters = what happens to its pixels.
- **Blending Mode:** right-click → Normal / Additive / Subtract / Screen / Multiply / Lighten / Darken. Added in **27.2** ([PR #5448](https://github.com/obsproject/obs-studio/pull/5448)). One menu on the row, not a panel — the default covers most users and the rest never see it.
- **Groups**, **nested scenes** (a Scene added as a Source), and **Text (GDI+/FreeType 2)** sources whose properties include Font, Text, **Read from file**, Color, Alignment, Outline, and FreeType's **Scroll**.
- **Studio Mode:** splits into **Preview** and **Program** with a **Transition** button.

**Transfers / doesn't transfer**
- Transfers: **ordered list, top = front**; **eye + lock on every row**; **transform separate from pixel effects**; **blend mode as a quiet per-row dropdown** defaulting to Normal (Additive/Screen/Multiply/Lighten are exactly right for additive LED colour); **text as a source type**, not a separate subsystem; **"read from file"** → a text layer bound to an API/MQTT slot.
- Doesn't: **rotation and free scaling** (mush at 64×64 — offer 90° steps and integer scale); the **user-editable filter chain** (the complexity trap); **Studio Mode** (the panel *is* the program — keep only "edit without disrupting", via a faithful in-browser preview); **Scene Collections** (one level of naming is enough).

**What this means for Luxel**
- Copy the IA nearly verbatim: a **Composite** is an ordered **Layers** list; top draws in front; each row has eye, lock, type icon, name, blend dropdown.
- One flat "Add layer" menu: **Pattern · Text · Sprite · Solid/Gradient**.
- Selecting a row reveals only *that layer's* inspector — the progressive disclosure PatternFlow lacks.
- Direct manipulation on the preview (drag, snap to integer px) with a numeric fallback. Not form-only.
- Ship ~5 blend modes; never expose a filter chain.

---

## C. WLED 2D

**2D Configuration page** (`settings_2D.htm`). First control `Strip or panel:` → **1D Strip / 2D Matrix**. Matrix reveals a **Matrix generator**: `Panel dimensions (WxH)`, `Horizontal panels`, `Vertical panels`, `1st panel`, `Orientation`, `Serpentine`, **Populate**. Then **Panel setup**: per-panel `1st LED`, `Orientation`, `Serpentine`, `Dimensions`, `Offset X/Y`. A live `Matrix Dimensions (W*H=LC)` readout **and a canvas wiring diagram** update as you type; a **Gap file** upload handles non-rectangular layouts. ([source](https://raw.githubusercontent.com/wled/WLED/main/wled00/data/settings_2D.htm), [docs](https://kno.wled.ge/advanced/2d-1d-Mixed-Setup/))

**Segments** are the unit of composition. In 2D the editor swaps `Start/Stop LED` for `Start X/Stop X/Start Y/Stop Y`, plus `Grouping`, `Spacing`, `Reverse`/`Mirror` per axis, `Transpose`. Each owns its effect, palette, colours and sliders ([segments](https://kno.wled.ge/features/segments/)).

**Blend modes — correcting the premise.** 0.14/0.15 shipped effect/*transition* blending styles ([PR #3877](https://github.com/wled/WLED/pull/3877)); real **segment layering** landed only in **v16.0**, after years of requests ([#3417](https://github.com/wled/WLED/issues/3417), [#4550](https://github.com/wled/WLED/issues/4550)). Overlapping segments composite live; **lower segment ID = bottom layer**. The dropdown, in order: `Top/Default, Bottom/None, Add, Subtract, Difference, Average, Multiply, Divide, Lighten, Darken, Screen, Overlay, Hard Light, Soft Light, Dodge, Burn, Stencil` (`FX.h`). Per-layer intensity is done by **lowering that segment's brightness** — there is no opacity control. ([Adafruit writeup](https://learn.adafruit.com/wled-16-what-is-new/segment-layering-and-blend-modes), which warns "additive blending can become very bright very quickly")

**Scrolling Text (FX 122).** **No text box** — the text *is the segment name*. Docs verbatim: "Edit segment name to set text (variables #DATE, #TIME, #DDMM, #MMDD, #HHMM, #HH, #MM; suffix with 0 to have leading 0s)" ([effects](https://kno.wled.ge/features/effects/)). Sliders: Speed, Y Offset, Trail, Font size, Gradient, Overlay. Five built-in fonts (6, 8, 8, 9, 12 px), four redrawn in v16.0 with variable widths; custom `.wbf` fonts upload via the File Manager, and the Font Factory accepts TTF/OTF/WOFF/BDF ([custom fonts](https://kno.wled.ge/features/custom-fonts/)).

**ledmap.** `ledmap.json` hand-written at `/edit`: `{"map":[0,1,2,3,7,6,5,4,...],"width":4,"height":3}`, `-1` = gap, multiple maps selectable per preset; docs warn "the ArduinoJSON library is extremely white-space sensitive" ([mapping](https://kno.wled.ge/advanced/mapping/)). It is an index remap *layered on top of* the 2D config. Third-party generators exist ([one](https://intrinsically-sublime.github.io/WLED-Ledmap.json-Generator/), [two](https://dosipod.github.io/WLED-Ledmap-Generator/)) — a tell that the built-in flow is insufficient.

**Documented confusion.** Text-as-segment-name: [#3002](https://github.com/wled/WLED/issues/3002) — *"the text that is displayed is tied to the segment name… I'm guessing this was a bit of a hack since there was no text field available"*; blanking the name for a clock then breaks presets ([#3526](https://github.com/Aircoookie/WLED/issues/3526)). ledmap semantics: [*"Just could not get my head around it"*](https://wled.discourse.group/t/scrolling-text-2d-matrix-setup/9757). The preview downsamples above 4096 px and users read it as a broken matrix ([thread](https://wled.discourse.group/t/seeking-help-for-led-matrix-display-issue-in-2d-mode/10528)). Font width/kerning: [#3071](https://github.com/wled/WLED/issues/3071), [#3020](https://github.com/Aircoookie/WLED/issues/3020).

**What this means for Luxel**
- **Never overload an identity field with content.** A text layer gets a real `text` field. This is WLED 2D's most-cited wart.
- **Two overlapping coordinate systems is one too many** (2D config *and* ledmap). Make the matrix case first-class with a **live wiring diagram** (WLED's is genuinely good); keep arbitrary maps as the escape hatch.
- **17 blend modes is a menu nobody reads.** Ship ~5 and carry the additive-clipping warning into the UI.
- Steal **`#DATE`/`#TIME` placeholders** — how non-programmers get live data into text — and generalize to named slots.
- Ordering by segment ID is an implementation detail leaking into UX; use a draggable list.

---

## D. Pixel Blaze

Tabs: **Patterns · Edit · Mapper · Settings · WiFi** ([getting started](https://www.bhencke.com/pixelblazegettingstarted)).

**Mapper** ([README.mapper.md](https://github.com/simap/pixelblaze/blob/master/README.mapper.md)). One box, two input modes: a **JSON array of arrays** (`[x,y]`/`[x,y,z]`), or **JavaScript evaluated in the browser** returning that array ("handy for generated or repetitive structures"). Coordinates auto-normalize to 0.0–1.0 with two fit modes: **Fill** ("stretched or squished to fit every dimension") and **Contain** ("keeps its aspect ratio, but is resized to fit the largest dimension"). "Changes to the editor, if valid, are applied live." Patterns opt in via `render2D(index,x,y)` / `render3D(...)`.

**Settings**, in on-screen order: **Name → LED Type → Pixels → Data Speed → Color Order → Limit Brightness → CPU Speed** — identity, then what makes LEDs light at all, then safety, then tuning.

**Sequencer.** Off / **Shuffle All** / **Playlist** (`SEQ_OFF 0`, `SEQ_SHUFFLE_ALL 1`, `SEQ_PLAYLIST 2`, `SEQ_SYNCHRONIZED 3` — [forum](https://forum.electromage.com/t/api-for-changing-patterns-in-a-playlist/3108)). Shuffle = random over everything on one duration; Playlist = ordered subset with **per-pattern durations** and fade-through-black.

**Editor.** Pattern list with live previews; code compiles on the fly per keystroke; language reference inline below. **Controls auto-generate from exported function names** — prefix picks the widget, remainder becomes the label: `sliderSharpness(v)`, `hsvPickerColor(h,s,v)`, `rgbPickerColor(r,g,b)` ([expressions README](https://github.com/simap/pixelblaze/blob/master/README.expressions.md)). (electromage.com/docs is JS-rendered; the GitHub READMEs are the reliable source.)

**What this means for Luxel**
- PB is **five tabs and no modals** — the counter-example to PatternFlow. Compositing must live *inside* the existing surface, not add a sixth top-level concept.
- **Naming-convention-driven controls** extend naturally: a `textFoo` export could declare a text slot the layer UI exposes — no new panel, no schema.
- **Fill vs Contain** is the right vocabulary for placing a layer in a sub-rect. Two words, already understood.
- **Live-apply everything.** PB has no Apply button anywhere.
- Copy the settings ordering principle: identity → make-it-light → safety → tuning.
- PB has **no compositing and no text** — same gap as PatternFlow, confirming the direction is unserved by the incumbents.

---

## E. Other references

**LedFx.** Unit is the **virtual** (a logical strip from device segments); exactly **one effect** per virtual — users fake layering by slicing pixel ranges. A **scene** snapshots which effect + preset each virtual runs, and each member gets one of four actions: **Activate / Ignore / Turn Black / Stop** ([scenes](https://docs.ledfx.app/en/latest/settings/scenes.html)). *Lesson:* a saved composite needs "leave this alone" and "explicitly blank this", not just "set it".

**Hyperion / HyperHDR.** The cleanest layering model here, and it is *not* alpha compositing — it's **priority muxing**. Sources register at a numeric priority and exactly one wins; **lower = higher priority** (`FG_PRIORITY 1`, `BG_PRIORITY 254`). Each registration carries `duration_ms` so inputs **self-expire**, and `serverinfo` exposes the stack as `{priority, componentId, origin, owner, visible, active, duration_ms}` ([PriorityMuxer.h](https://github.com/hyperion-project/hyperion.ng/blob/master/include/hyperion/PriorityMuxer.h)). *Lesson:* a transient overlay should be "push at priority 10 for 5 s", not UI state someone must clean up.

**xLights.** An **effect instance** in a timeline cell, on an **effect layer**, on a model row. Per-model layers stack with a **Mix slider**, 19 blend choices, **Canvas Mode** ("draw on a previous layer without blanking it out"), masking, **Sub Buffer** ([Layer Blending](https://manual.xlights.org/xlights/chapters/chapter-four-sequencer/layers/layer-blending)). Too heavy because composition is welded to a fixed **timeline**, the model/group/submodel/buffer-style matrix multiplies, and blending is framed **pairwise** ("layer 1 vs layer 2") so you re-reason at every boundary. Take the mix/canvas/mask vocabulary; drop the timeline.

**FPP — Pixel Overlay Models.** The most directly transferable design. A **named, addressable buffer** that overrides channel data live, whose compositing is a per-model **state**, not an alpha: `Disabled | Enabled | Transparent | TransparentRGB` — **Transparent means black pixels pass through**. All REST-driven: `PUT /api/overlays/model/{m}/state|fill|pixel|data|text` ([PixelOverlay.cpp](https://github.com/FalconChristmas/fpp/blob/master/src/overlays/PixelOverlay.cpp)). *Lesson:* **name the layer and give it a URL**; "transparent = black passes through" is a one-checkbox compositing model needing no alpha channel.

**Glediator / Jinx!** Borrows from video/DJ desks: 2 effect channels (Glediator 2: 4), each split into **2 effect generators with a crossfade between them**, then a master crossfade. Physical layout lives separately in the **Output Patch** window ([manual](https://live-leds.de/jinx-usermanual-0.95a)). The generator/output-patch split is right — mapping is not a layer property — but a fixed 2×2 tree is a ceiling users hit immediately.

**Pixel-art editors** (LED Matrix Studio, Piskel, Pixilart, Arduino's ledmatrix-editor). Unit is the **frame**; layers are an authoring convenience that **flattens on export**. *Lesson:* don't rebuild bitmap authoring — the ecosystem is fine. What's missing is the **runtime**: a sprite layer that accepts an uploaded bitmap and composites it live.

**Layer-based LED projects.** WLED 16 is now the de-facto vocabulary (§C). **SmartMatrix** is the embedded precedent: typed layers (`backgroundLayer`, `scrollingLayer`, `indexedLayer`) composited **bottom-to-top in add order** ([Features](https://github.com/pixelmatix/SmartMatrix/wiki/Features)). **Pixelblaze has none**; layering exists only in third-party tooling — the PXLBLZ IDE's "Show" concept "takes one, two, or more patterns and recombines them… layering effects over them" ([PXLBLZ-IDE](https://github.com/jon-whiteroomsoftware/PXLBLZ-IDE)). SignalRGB and OpenRGB have no user-facing blend stack.

**What this means for Luxel**
- Adopt **SmartMatrix's typed layers** (background / pattern / sprite / text with different capabilities) over one generic layer that must do everything.
- Adopt **FPP's "Transparent = black passes through"** as the default rule — correct for additive LEDs, no alpha channel, explainable in six words.
- Adopt **Hyperion's self-expiring priority table** for *programmatic* overlays (alerts, banners) — a separate surface from the user's hand-built stack.
- **Name layers and give each a REST path** — scriptable with no new UI.
- Match **WLED 16's blend-mode names** where they overlap; ship a 5-mode subset.
- Avoid xLights' timeline and Jinx!'s fixed channel tree.

---

## F. Fonts for LED matrices

| Font | Cell | Coverage | License | Source |
|---|---|---|---|---|
| **Tom Thumb** | 3×5 ink in a **4×6 cell** | ASCII + a sprinkling of Latin-1 (203 glyphs) | orig. **BSD-3** (Swetland/Khachaturov); author authorized **CC0 / CC-BY 3.0** in 2015 | [robey.lag.net](https://robey.lag.net/2010/01/23/tiny-monospace-font.html), quoted in [TomThumb.h](https://github.com/adafruit/Adafruit-GFX-Library/blob/master/Fonts/TomThumb.h) |
| **X11 misc-fixed** 4×6, 5×7, 5×8, 6×10, 6×13… | as named | 4×6 = 919 glyphs; 5×7 = 1848; 6×13 = 4121 | **Public domain** — *verified in the BDFs*: `COPYRIGHT "Public domain font.  Share and enjoy."` | [ucs-fonts README](https://github.com/hzeller/rpi-rgb-led-matrix/blob/master/fonts/README) |
| **Spleen** | **5×8**, 6×12, 8×16, 12×24, 16×32, 32×64 | 5×8 = printable ASCII; larger = Latin-1+ | **BSD-2** | [LICENSE](https://github.com/fcambus/spleen/blob/master/LICENSE) |
| **Tiny5** | **5 px** | **1,749 glyphs** — Latin/Greek/Cyrillic/Armenian | **OFL 1.1** | [OFL.txt](https://github.com/Gissio/font_Tiny5/blob/main/OFL.txt) |
| **Matrix-Fonts** (trip5) | MatrixChunky6/8, MatrixLight6/8 | ASCII+ | **MIT** — *purpose-built for LED matrix clocks* | [LICENSE](https://github.com/trip5/Matrix-Fonts/blob/main/LICENSE) |
| **Pixel Operator** | 8 / 16 px | large Latin | **CC0 1.0** | [notabug](https://notabug.org/HarvettFox96/ttf-pixeloperator) |
| **creep / creep2** | ~5×9 / 6×11 | ASCII+ | **MIT** (© 2014 Romeo Van Snick) | [LICENSE](https://github.com/raymond-w-ko/creep2/blob/master/LICENSE) |
| **Cozette** 6×13 · **Terminus** 6×12+ · **Scientifica** 5×11 · **ProFont** | — | wide | **MIT** · **OFL 1.1** · **OFL 1.1** · **MIT** | per-repo LICENSE |
| **Unscii** | 8×8, 8×16 | large Unicode | **PD / CC0** — *except* `unscii-16-full`, which pulls Unifont glyphs and inherits its licence | [viznut.fi/unscii](http://viznut.fi/unscii/) |
| **Silkscreen** | 4×5 / 5×5 ink | ASCII + Latin-1 | **SIL OFL** — but **TTF/OTF only, no BDF** | [kottke.org](https://kottke.org/plus/type/silkscreen/) |

**Not safely bundleable — checked and rejected:**

| Font | Problem |
|---|---|
| **Minecraftia** | **Personal use only**; commercial requires a paid EULA ([gumroad](https://andrewtyler.gumroad.com/l/minecraftia)). Sources that call it CC BY-SA are wrong. |
| **Adafruit Picopixel / Org_01 / Tiny3x3a2pt** | **No per-font licence header** — BSD-3 inheritance from the repo is assumed, never stated. |
| **Adafruit FreeMono/Sans/Serif** | GNU FreeFont **GPL-3.0 + font exception** — fine in `firmware/`, wrong for Apache-2.0 `crates/`. |
| **`glcdfont.c` 5×7** | Provenance murky, a long-standing unresolved question upstream. |
| **04b_03** (ships *inside* u8g2) | Author's own freeware terms. **u8g2 being BSD-2 does not make its fonts free** ([fntgrp](https://github.com/olikraus/u8g2/wiki/fntgrp)). |
| **Pixellari, BM\* series, Dina** | "Free" badges with no licence instrument; Dina is also ≥10 px and ships as Windows `.FON`. |

**Flash math** (95 glyphs, ASCII 32–126, as a 16-wide 1bpp sheet — always byte-dense, since 16·W bits = 2·W bytes): 3×5 → **180 B**, 4×6 → **288 B**, 5×7 → **420 B**, 5×8 → **480 B**, 6×8 → **576 B**, 6×10 → **720 B**, 8×8 → **768 B**. Note that **proportional schemes spend 74–85 % of their bytes on the glyph table** at this size — Adafruit's Picopixel is 180 B of ink wrapped in 760 B of bookkeeping.

**The u8g2 finding that changes the calculus.** u8g2 is BSD-2 with 2,043 font arrays (14.0 MB of `u8g2_fonts.c`), format = 23-byte header + per-glyph `{codepoint, jump offset, bit-packed metrics, RLE bitstream}` ([format wiki](https://github.com/olikraus/u8g2/wiki/u8g2fontformat)). **Its RLE buys nothing at 5–8 px**: `u8g2_font_4x6_tr` is **723 B for 95 glyphs (7.6 B/glyph)** against **288 B raw (3 B/glyph)**, because the per-glyph header outweighs the pixels. u8g2's own benchmark shows the win only appearing around 20–50 px. Choose u8g2 for its 2,000 ready faces and its runtime decoder, never for compression.

**Upload formats, by parse cost**

| Format | no_std parse | ~96 glyphs @5–7 px | Verdict |
|---|---|---|---|
| **Raw 1bpp 16-wide sheet** | trivial (`row=i/16; col=i%16`) | **288–420 B** | internal format |
| **PSF v1/v2** | **trivial** — 4/32 B header, raw MSB-first rows | 704–800 B | **best upload format** |
| **u8g2 blob** | moderate (`new_from_raw_data`) | 723–804 B | best *proportional/Unicode* upload |
| **BDF** | painful (text, alloc, linear scan) | **~10–94 KB of text** | host-side only |
| **GFX `.h`** | ✗ it is C source | 918–1047 B on ESP32 | host-side only |
| **PCF** / **FON/FNT** | ✗ endian/bit-order/pad bits; NE resource walk | 5–20 KB | skip |
| **PNG sheet / BMFont** | needs a PNG decoder | — | host-side only |
| **TTF/OTF @5–8 px** | ✗ (the font file alone is 100–800 KB vs a 1 MiB OTA slot) | — | host-side, with preview |

Gotchas worth recording: `GFXglyph` is **8 B on ESP32, not 7 B** (the `u16` forces alignment); **BDF's `BBX` uses a lower-left, y-up origin** — inverted relative to a framebuffer; and **canvas cannot disable text antialiasing** (`imageSmoothingEnabled` only affects `drawImage`), so a browser TTF path must threshold coverage itself or scanline-fill outlines via opentype.js.

**Recommendation: do what WLED already did.** WLED solved this exact problem — a browser **Font Factory** takes TTF/OTF/WOFF/BDF and emits `.wbf`: a **12-byte header** + optional width table + MSB-first row-packed bitstream ([fontmanager.h](https://github.com/wled/WLED/blob/main/wled00/fontmanager.h)). Their Tom Thumb `.wbf` is **297 bytes** — 12 + 95×3, the theoretical minimum. ESPHome does the same (Pillow converts host-side); embedded-graphics does the same (BDF→PNG→`.raw`). **Nobody parses BDF or TTF on the MCU.**

**Default bundle — three fonts, ~1.1–1.4 KB, all PD/CC0/BSD-2 so no attribution plumbing and no Apache-vs-GPL reasoning:**

| Font | Cell | Licence | Flash |
|---|---|---|---|
| **Tom Thumb** | 3×5 in 4×6 | CC0 / CC-BY-3.0 | ~190 B |
| **X11 misc-fixed 5×7** | 5×7 | Public domain | 420 B |
| **Spleen 5×8** *or* **misc-fixed 6×10** | 5×8 / 6×10 | BSD-2 / PD | 480 / 720 B |

Tom Thumb fits ~16 chars per line on a 64-wide panel; 5×7 misc-fixed is the readability sweet spot **and is the same glyph data `embedded-graphics` already ships, so its 420-byte `.raw` can be `include_bytes!`'d today**; the third is a comfortable "large". Add **Tiny5** (OFL, BDF, 5 px, 1749 glyphs) when non-ASCII is wanted, and **trip5/Matrix-Fonts** (MIT, built for LED clocks) as a curated extra.

**Recommended user-upload format: PSF2.** A 32-byte little-endian header (`0x72 0xB5 0x4A 0x86`, version, headersize, flags, length, charsize, height, width) then `length × charsize` bytes of MSB-first row-packed glyphs — **O(1) lookup, zero allocation, ~20 lines of no_std Rust**, and if you skip the Unicode table *the body is the internal format verbatim* ([spec](https://www.win.tue.nl/~aeb/linux/kbd/font-formats-1.html)). Tooling is ubiquitous (`bdf2psf`, FontForge, psftools) and Spleen/Terminus/Unscii already ship `.psf`. But make the **browser the real front door**: convert BDF, GFX `.h`, PNG sheets, BMFont and TTF to that blob before upload — [`bdfparser`](https://www.npmjs.com/package/bdfparser) (MIT, zero-dep TS) and [`lgfx-font-tool`](https://github.com/tanakamasayuki/LGFXFontToolJs) (MIT, decodes u8g2/GFXfont/BDF/VLW/BFF/FONTX2 *and* rasterizes TTF in-browser) do ~90 % off the shelf.

**What this means for Luxel**
- **Firmware parses exactly one trivial format forever — use PSF2 rather than inventing one.** It is already the shape we'd design, with a spec and a decade of tooling behind it.
- Bundle **3 fonts / ~1.2 KB**, all PD/CC0/BSD-2. **`embedded-graphics`' 5×7 `.raw` is a 420-byte drop-in for font #2 right now.**
- **Check the primary source, not the aggregator — two widely-repeated licence claims are wrong.** Pixel Operator is **CC0 1.0** (commonly cited as OFL) and Minecraftia is **personal-use-only** (commonly cited as CC BY-SA). Conversely creep *is* cleanly MIT despite its BDF carrying only a bare copyright line.
- **Do not link u8g2 fonts into the binary.** `u8g2_font_wqy16_t_gb2312` alone is 308 KB — ~30 % of the OTA slot. If proportional/Unicode text is wanted later, add `u8g2-fonts` and load blobs from the assets partition or PSRAM.
- Store **per-glyph advance widths from day one** even if v1 renders monospace — but note the 74–85 % table overhead, so keep them a separate optional table rather than a per-glyph struct.
- Show a **live 64×64 preview with a threshold slider** on the TTF path: at 6 px the threshold changes glyph *identity*, and canvas antialiasing cannot be turned off.
- Avoid the Adafruit tiny fonts (Picopixel/Org_01/Tiny3x3) despite their popularity — no per-font licence grant.

---

## G. Text rendering APIs, and candidate Luxel builtins

**WLED** (`mode_2Dscrollingtext`). Text = segment name; empty falls back to `"#MON #DD #YYYY #TIME"`. Placeholders parse on `#` with an optional trailing `0` for leading zeros: `#DATE #DDMM #MMDD #TIME #HHMM #YYYY #MONL #DDDD #YY #HH #MM #SS #MON #MO #DAY #DD`. Controls ride the generic slider slots: intensity=Y Offset, custom1=**Trail** (a `fade_out` decay — motion blur for one parameter), custom2=Font size, custom3=Rotate (±1 = 90°, ±2 = 180°), check1=Gradient, check2=Custom Font, check3=Reverse. Modern WLED is UTF-8 variable-width (`getGlyphWidth(unicode)`, `drawCharacter(unicode, x, y, col1, col2, rotate)`); when text fits the width it centres and scrolls **vertically** instead ([FX.cpp](https://github.com/wled/WLED/blob/main/wled00/FX.cpp)).

**Adafruit GFX** — the API everything imitates: `setCursor(x,y)`, `setTextSize(s)`, `setTextColor(c[,bg])`, `setTextWrap(bool)`, `setFont(const GFXfont*)`, `drawChar(x,y,c,color,bg,size)`, `getTextBounds(str,x,y,&x1,&y1,&w,&h)`. **No alignment constants** — you call `getTextBounds` and subtract. Footgun: the built-in font draws from **top-left**, GFXfonts from the **baseline** ([Adafruit_GFX.h](https://github.com/adafruit/Adafruit-GFX-Library/blob/master/Adafruit_GFX.h)).

**SmartMatrix** — text as a *layer*, the closest analogue to a layered UI: `start(text, numScrolls)`, `update(text)` (change without restarting), `stop()`, `setMode(ScrollMode)`, `setSpeed(pixels_per_second)`, `setFont(fontChoices)`, `setOffsetFromTop(int)`, with `ScrollMode { wrapForward, bounceForward, bounceReverse, stopped, off, wrapForwardFromLeft }` and `fontChoices { font3x5, font5x7, font6x10, font8x13, gohufont11, gohufont11b }` ([Layer_Scrolling.h](https://github.com/pixelmatix/SmartMatrix/blob/master/src/Layer_Scrolling.h)).

**FPP** — `PUT /api/overlays/model/{m}/text` with `{Message, Color, Font, FontSize, AntiAlias, Position, PixelsPerSecond, AutoEnable}`. `Position` is `Center | Right to Left | Left to Right | Bottom to Top | Top to Bottom` — **direction and static-vs-scrolling folded into one enum**. The only system here using real TTFs with an anti-alias flag.

**Pixel Blaze — no text API at all.** Users hand-roll it: the canonical [thread](https://forum.electromage.com/t/heres-code-for-scrolling-text-across-a-matrix/321) embeds a public-domain IBM BIOS font as arrays in pattern source (8 bytes/glyph), rasterizes to a buffer, and swaps the message by poking an exported var over WebSocket. The [upgraded Marquee](https://forum.electromage.com/t/upgraded-pixelblaze-marquee-gradients-zoom-and-easy-text-tool/4630) ships an offline HTML tool you type into that emits an ASCII array to paste in. **This is the gap.**

**p5.js** — `text(str, x, y, [maxWidth], [maxHeight])`, `textSize()`, `textAlign(horiz, vert)` with `LEFT|CENTER|RIGHT` × `TOP|BOTTOM|CENTER|BASELINE`, `textWidth(str)`, `textWrap(WORD|CHAR)`. Default `(LEFT, BASELINE)` — wrong for a 64×64 grid ([text()](https://p5js.org/reference/p5/text/), [textAlign()](https://p5js.org/reference/p5/textAlign/)).

**Rust prior art.** `embedded-graphics` treats text as a **drawable value, not a cursor**: `Text::with_alignment(text, position, character_style, alignment)`, `Baseline { Top, Bottom, Middle, Alphabetic }`, `Alignment { Left, Center, Right }`; drawing returns the **next cursor `Point`** so runs chain. Two orthogonal style objects — `character_style` (font, colour) vs `text_style` (alignment, baseline, line height) — is a clean split. `u8g2-fonts` adds `render_aligned(content, position, vertical_pos, horizontal_align, color, display)` and `get_rendered_dimensions*()` to **measure without drawing**; non-Left alignment costs two passes, which is why they're separate entry points.

**Scrolling and kerning.** Only SmartMatrix, FPP and WLED treat scrolling as an API rather than caller-side `x -= 1`; SmartMatrix's is richest and the only one with **bounce**. **Nobody implements true pair kerning** — uniformly per-glyph advance plus constant letter spacing. Monospace-only v1 is entirely in keeping with the field.

### Candidate builtins for a fixed-point VM with no string type

The constraint is real but small: text enters the VM as a **number**. Three mechanisms, best shipped together.

**1. Compiler-interned literals (primary).** The parser accepts `"…"` *only in builtin call position*, interns the bytes into a per-pattern **string constant table** emitted alongside bytecode, and passes the VM an index. No string *type*, no concatenation, nothing for the VM to allocate.
```
drawText("HELLO", x, y)
drawText("HELLO", x, y, align)     // align packs H (L/C/R) and V (T/C/B)
textWidth("HELLO") -> pixels       // measure without drawing
setFont(1); setTextColor(h, s, v)  // modal state, GFX-style
```

**2. Host-set text slots (companion).** A fixed array of N slots (say 8) written by the UI, REST API or MQTT; patterns refer to one by index. This is how a Text *layer* gets its content and how `#TIME` generalizes.
```
drawTextSlot(0, x, y, align)
textSlotWidth(0) -> pixels
```
Slot 0 = "this layer's own text", so a Text layer needs no pattern code at all.

**3. Number formatting.** Without strings there is no `itoa`:
```
drawNumber(value, x, y, digits, decimals)   // fixed-point aware
```
covering clocks, counters and temperatures — most of what people actually put on a panel.

**What this means for Luxel**
- **Ship all three.** Each is a few dozen lines and they cover disjoint needs.
- Make the **Text layer require zero code**: the layer UI writes slot 0, the built-in renderer draws it. `drawText` is the power-user path, not the only path.
- **Default alignment `(LEFT, TOP)`**, not p5's `(LEFT, BASELINE)`. One origin convention, top-left, always — avoid GFX's dual-origin footgun.
- Expose **`textWidth()` / measure-without-draw** from day one; every centering and scroll-wrap computation needs it.
- Fold direction into one enum like FPP's `Position` (`Static | Left | Right | Up | Down | Bounce`) rather than direction + reverse + mode flags, and use SmartMatrix's **`pixels_per_second`** (frame-rate independent, unlike WLED's slider).
- Steal WLED's **Trail** — one parameter, looks far better than it costs.
- Monospace v1, but design **per-glyph advance widths** into the API immediately. Skip kerning, as everyone does.

---

## Synthesis: the one-sentence model

> **A composite is an ordered list of layers; the top layer draws in front; each layer is a pattern, some text, or a sprite, placed in a rectangle and blended.**

Five nouns, one ordering rule. Everything above either supports that sentence (OBS's list, SmartMatrix's typed layers, FPP's transparent state, PB's Fill/Contain) or is a warning about what happens when a project needs more than one sentence (PatternFlow's 13 nouns and nine panels, xLights' timeline, WLED's segment-name-as-text).

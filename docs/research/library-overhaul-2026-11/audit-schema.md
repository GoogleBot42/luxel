# Per-pattern audit record (one JSON object per line, file: $SCRATCH/libfix/audit/<range>.jsonl)

{
  "file": "library/<stem>.js",
  "name": "<display name from // name:>",
  "provenance": "port" | "original",
  "dims": "strip" | "grid" | "cloud" | "any",
  "concept": "<one sentence: what it looks like / the idea>",
  "technique": "<one phrase: per-pixel hsv math | canvas sim | particles | palette | noise | renderBulk | sensor | game | ...>",
  "controls": <count of exported controls>,
  "category": "showcase" | "ambient" | "holiday" | "sound" | "sensor" | "game" | "utility" | "test" | "teaching-example" | "clock",
  "coolness": 1-5,            // 5 = jaw-dropping on a 64x64 panel or a 150 px strip; 1 = sad/boring/test pattern
  "visual_verdict": "<what you actually SAW on the contact sheet, one sentence; note if dark, static, broken, or garish>",
  "perf_smell": "<none | one phrase: e.g. 3x array(pixelCount), perlinFbm per pixel, hand-rolled blur loop, sqrt per pixel per sprite, ...>",
  "host_ms_per_frame_1024px": <number from the sheet json>,
  "duplicate_of": ["<stem>", ...],    // patterns in the WHOLE library (not just your range) this is a near-duplicate of; use the inventory
  "scene_or_sprite_candidate": "<no | yes: why — e.g. 'this is a static picture: should be a sprite'; 'this is pattern + clock text: should be a scene'>",
  "cleanroom_quality": "<n/a for originals | one sentence: does the port deliver the idea the spec describes? what is lost or wrong?>",
  "recommended_action": "keep" | "polish" | "rewrite" | "merge-into:<stem>" | "convert-to-scene" | "convert-to-sprite" | "delete",
  "action_notes": "<one or two sentences: what specifically to do — the concrete merge/param plan, the rewrite angle, the perf fix>"
}

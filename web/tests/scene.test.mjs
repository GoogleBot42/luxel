// The TypeScript scene codec (src/lib/scene.ts) against the Rust one it
// mirrors (crates/luxel-core/src/scene.rs, spec docs/spec/scenes.md).
//
// The fixtures below are copied VERBATIM from the Rust unit tests and the
// spec, because the whole point of this module is that the browser can build
// a block the device parses and parse a block the device wrote. A drift in
// either direction is a scene that round-trips into something else.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import {
  decStr,
  emptyScene,
  newLayer,
  parseScene,
  parseScenes,
  patternLayerCount,
  sceneFromJson,
  sceneJson,
  serializeScene,
  serializeScenes,
  truncateUtf8,
  validSceneId,
  withPatternOnTop,
} from "../src/lib/scene.ts";

/** The fixture `scene::tests::json_carries_the_api_shape` parses. */
const FIXTURE_WIRE =
  "S 0000000a My \"scene\"\n" +
  "L pat 1 2 16 8 multiply 60 luma tile 7\n" +
  "I 0123abcd\n" +
  "C speed 32768\n" +
  "P xy\n" +
  "R 80 0:000000 255:ffffff\n" +
  "L text 0 0 0 0 normal 100 none fill 1\n" +
  "T slot 3\n" +
  "L sprite 0 0 0 0 normal 100 black fill 1\n" +
  "I 0123abce\n" +
  "L color 0 0 0 0 add 50 none fill 1\n" +
  "K ff8800\n";

/** The bytes that Rust test asserts `push_json` writes, byte for byte. */
const FIXTURE_JSON =
  '{"id":"0000000a","name":"My \\"scene\\"","layers":[' +
  '{"type":"pat","name":"","x":1,"y":2,"w":16,"h":8,"blend":"multiply",' +
  '"opacity":60,"key":"luma","fit":"tile","visible":true,"flipx":true,"flipy":true,' +
  '"rot180":false,"pat":{"id":"0123abcd","controls":{"speed":[0.5]},"proj":"xy",' +
  '"ramp":{"pct":80,"stops":[[0,"000000"],[255,"ffffff"]]}}},' +
  '{"type":"text","name":"Text","x":0,"y":0,"w":0,"h":0,"blend":"normal",' +
  '"opacity":100,"key":"none","fit":"fill","visible":true,"flipx":false,' +
  '"flipy":false,"rot180":false,"text":{"source":"slot","slot":3,"font":"regular",' +
  '"color":"ffffff","align":"l","scroll":"none","speed":0}},' +
  '{"type":"sprite","name":"Sprite","x":0,"y":0,"w":0,"h":0,"blend":"normal",' +
  '"opacity":100,"key":"black","fit":"fill","visible":true,"flipx":false,' +
  '"flipy":false,"rot180":false,"sprite":{"id":"0123abce"}},' +
  '{"type":"color","name":"Color","x":0,"y":0,"w":0,"h":0,"blend":"add",' +
  '"opacity":50,"key":"none","fit":"fill","visible":true,"flipx":false,' +
  '"flipy":false,"rot180":false,"color":"ff8800"}]}';

const ok = (r) => {
  assert.equal(r.ok, true, r.ok ? "" : String(r.error));
  return r.scene;
};

test("the spec fixture parses into the record the Rust test describes", () => {
  const s = ok(parseScene(FIXTURE_WIRE));
  assert.equal(s.id, "0000000a");
  assert.equal(s.name, 'My "scene"');
  assert.equal(s.layers.length, 4);
  assert.equal(patternLayerCount(s), 1);

  const [pat, text, sprite, color] = s.layers;
  // flags 7 = visible | flipx | flipy, rot180 clear
  assert.deepEqual(pat.style, {
    rect: { x: 1, y: 2, w: 16, h: 8 },
    blend: "multiply",
    opacity: 60,
    key: "luma",
    fit: "tile",
    visible: true,
    flipx: true,
    flipy: true,
    rot180: false,
  });
  assert.equal(pat.name, ""); // a pat layer's default name is EMPTY
  assert.equal(pat.body.pat.id, "0123abcd");
  assert.deepEqual(pat.body.pat.controls, { speed: [0.5] });
  assert.equal(pat.body.pat.proj, "xy");
  assert.deepEqual(pat.body.pat.ramp, {
    pct: 80,
    stops: [
      [0, "000000"],
      [255, "ffffff"],
    ],
  });
  assert.deepEqual(text.body.text.source, { kind: "slot", slot: 3 });
  assert.equal(text.name, "Text");
  assert.equal(sprite.body.id, "0123abce");
  assert.equal(color.body.color, "ff8800");
});

test("serialize ∘ parse is a fixed point on the fixture's text", () => {
  assert.equal(serializeScene(ok(parseScene(FIXTURE_WIRE))), FIXTURE_WIRE);
});

test("the JSON is byte-identical to luxel_core::scene::push_json", () => {
  assert.equal(sceneJson(ok(parseScene(FIXTURE_WIRE))), FIXTURE_JSON);
});

test("a scene from the device's JSON serializes back to the same wire", () => {
  const viaJson = sceneFromJson(JSON.parse(FIXTURE_JSON));
  assert.equal(serializeScene(viaJson), FIXTURE_WIRE);
});

test("defaults are omitted, so a blank layer is one line", () => {
  const s = emptyScene("Clock overlay");
  s.id = "5eed1c92";
  s.layers.push(newLayer("text"));
  assert.equal(
    serializeScene(s),
    "S 5eed1c92 Clock overlay\nL text 0 0 0 0 normal 100 none fill 1\n",
  );
});

test("a named, styled text layer emits N, T and F — and nothing else", () => {
  const s = emptyScene("x");
  s.id = "5eed1c92";
  const l = newLayer("text");
  l.name = "Time";
  l.style.rect = { x: 0, y: 28, w: 64, h: 8 };
  l.body.text.source = { kind: "clock", fmt: "HH:MM" };
  l.body.text.align = "c";
  s.layers.push(l);
  assert.equal(
    serializeScene(s),
    "S 5eed1c92 x\n" +
      "L text 0 28 64 8 normal 100 none fill 1\n" +
      "N Time\n" +
      "T clock HH:MM\n" +
      "F regular ffffff c none 0\n",
  );
  assert.equal(serializeScene(ok(parseScene(serializeScene(s)))), serializeScene(s));
});

test("a colour layer only emits K when it is not black", () => {
  const s = emptyScene("x");
  s.id = "5eed1c92";
  s.layers.push(newLayer("color"));
  s.layers[0].body.color = "000000";
  assert.equal(serializeScene(s), "S 5eed1c92 x\nL color 0 0 0 0 normal 100 none fill 1\n");
  s.layers[0].body.color = "e8a33d";
  assert.match(serializeScene(s), /\nK e8a33d\n$/);
});

test("a blob of several scenes splits on its S lines", () => {
  const a = "S 00000001 One\nL color 0 0 0 0 normal 100 none fill 1\nK ff0000\n";
  const b = "S 00000002 Two\nL text 0 0 0 0 normal 100 none fill 1\n";
  const r = parseScenes(a + b);
  assert.equal(r.ok, true);
  assert.equal(r.scenes.length, 2);
  assert.deepEqual(
    r.scenes.map((s) => s.name),
    ["One", "Two"],
  );
  assert.equal(serializeScenes(r.scenes), a + b);
});

test("parse errors name the line, in the device's own words", () => {
  const bad = (wire) => {
    const r = parseScene(wire);
    assert.equal(r.ok, false, "expected a rejection");
    return r.error;
  };
  assert.equal(
    bad("S 0000000a n\nL pat 0 0 0 0 foo 100 none fill 1\n"),
    'scene: line 2: unknown blend "foo"',
  );
  assert.equal(
    bad("S 0000000a n\nL pat 0 0 0 0 normal 101 none fill 1\n"),
    'scene: line 2: opacity must be 0..100 "101"',
  );
  assert.equal(bad("S zz n\n"), 'scene: line 1: bad scene id "zz"');
  assert.equal(bad("L color 0 0 0 0 normal 100 none fill 1\n"), "scene: line 1: an L line before the S line");
  assert.equal(bad(""), "scene: line 1: expected an S line");
  assert.equal(
    bad("S 0000000a n\nL pat 0 0 0 0 normal 100 none fill 1\nR 50 0:000000\n"),
    "scene: line 3: a ramp needs at least two stops",
  );
  assert.equal(
    bad("S 0000000a n\nL text 0 0 0 0 normal 100 none fill 1\nT slot 9\n"),
    'scene: line 3: a text slot is 0..7 "9"',
  );
  // blob-global line numbers
  const r = parseScenes("S 00000001 a\nS 00000002 b\nL pat 0 0 0 0 nope 100 none fill 1\n");
  assert.equal(r.ok, false);
  assert.equal(r.error, 'scene: line 3: unknown blend "nope"');
});

test("unknown tags and inapplicable binding lines are ignored, not fatal", () => {
  const s = ok(
    parseScene(
      "S 0000000a n\n" +
        "Z something from a newer console\n" +
        "L color 0 0 0 0 normal 100 none fill 1\n" +
        "T lit ignored on a colour layer\n" +
        "K 112233\n",
    ),
  );
  assert.equal(s.layers.length, 1);
  assert.equal(s.layers[0].body.color, "112233");
});

test("decStr is Fx::dec_str, exactly", () => {
  assert.equal(decStr(0), "0");
  assert.equal(decStr(0.5), "0.5");
  assert.equal(decStr(-3.5), "-3.5");
  assert.equal(decStr(3), "3");
  assert.equal(decStr(1 / 65536), "0.0000152587890625");
});

test("ids and utf-8 truncation", () => {
  assert.equal(validSceneId("5eed1c92"), true);
  assert.equal(validSceneId("5EED1C92"), false);
  assert.equal(validSceneId("5eed1c9"), false);
  assert.equal(truncateUtf8("ab€cd", 4), "ab"); // € is 3 bytes
});

// ---- `Add to scene ▸` (Gitea #478, proposal §5.4b) ----
//
// The shortcut is read-modify-write: the scene the store holds, plus this
// pattern as its TOP layer, handed back to `saveScene`. What the device sees
// is `serializeScene` of the result, so the layers

// ---- `Add to scene ▸` (Gitea #478, proposal §5.4b) ----
//
// The shortcut is read-modify-write: the scene the store holds, plus this
// pattern as its TOP layer, handed back to `saveScene`. What the device sees
// is `serializeScene` of the result, so the layer's defaults are pinned here —
// a full-layout box, normal, 100 %, unkeyed, visible.

test("withPatternOnTop appends a full-layout pattern layer, last", () => {
  const base = parseScene(
    "S 0000000a Clock overlay\nL color 0 0 0 0 normal 100 none fill 1\nK 112233\n",
  );
  assert.equal(base.ok, true);
  const next = withPatternOnTop(base.scene, "5eed1c92");
  assert.equal(base.scene.layers.length, 1, "the input scene is not mutated");
  assert.equal(next.layers.length, 2);
  assert.equal(next.layers[1].body.kind, "pat");
  assert.equal(next.layers[1].body.pat.id, "5eed1c92");
  assert.equal(patternLayerCount(next), 1);
  const wire = serializeScene(next).split("\n");
  assert.ok(wire.includes("L pat 0 0 0 0 normal 100 none fill 1"), wire.join(" | "));
  assert.ok(wire.includes("I 5eed1c92"), wire.join(" | "));
});

test("withPatternOnTop carries the values and the projection it was shown at", () => {
  const next = withPatternOnTop(emptyScene("S"), "5eed1c92", { speed: [0.5], hue: [0.25, 1] }, "x");
  const wire = serializeScene(next);
  assert.match(wire, /^C speed 32768$/m);
  assert.match(wire, /^C hue 16384 65536$/m);
  assert.match(wire, /^P x$/m);
});

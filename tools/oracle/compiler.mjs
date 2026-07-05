// Extract the Pixel Blaze pattern compiler from the device's own web UI and
// run it in a node:vm sandbox. Recipe ported from pixelblaze-client (MIT,
// zranger1) — the v3 > 3.4 firmware adapter. Works against fw 3.67.

import vm from "node:vm";

function sub(text, startValue, endValue) {
  const s = text.indexOf(startValue);
  if (s < 0) throw new Error(`marker not found: ${startValue}`);
  const e = text.indexOf(endValue, s);
  if (e < 0) throw new Error(`end marker not found: ${endValue}`);
  return text.slice(s, e);
}

function extractComponents(webUI) {
  const hardwareVariant = "var " + sub(webUI, "hardwareVariant=", ",varWatcherPoller") + ";";
  const extendedOperators = sub(webUI, "extendedOperators={", ",lastErrorMarkers=") + ";";
  const constants = "var constants;" + sub(webUI, '"ESP8266"===hardwareVariant&&', ",[])") + ";";
  let compiler = null;
  let rest = webUI;
  for (;;) {
    const a = rest.indexOf("<script>");
    if (a < 0) break;
    const b = rest.indexOf("</script>", a);
    const script = rest.slice(a + 8, b);
    rest = rest.slice(b + 9);
    if (script.includes("window.compile")) {
      compiler = script + ";";
      break;
    }
  }
  if (!compiler) throw new Error("compiler <script> block not found");
  return { hardwareVariant, extendedOperators, constants, compiler };
}

/** Build a compile(source) → {compiled: int32[], exports: [{address,name}]} */
export function buildCompiler(webUI) {
  const c = extractComponents(webUI);
  const bootstrap =
    'window = {};\nvar predefinedGlobals = ["pixelCount"];\n' +
    `${c.hardwareVariant}\n${c.constants}\n${c.extendedOperators}\n${c.compiler}\n`;
  const ctx = vm.createContext({});
  vm.runInContext(bootstrap, ctx, { filename: "pb-webui.js" });
  vm.runInContext(
    `function __compile(src) {
       try {
         var program = window.compile(src, {
           predefinedGlobals: predefinedGlobals,
           extendedOperators: extendedOperators,
           constants: constants,
         });
         var exports = Object.keys(program.exports).reduce(function (r, k) {
           return r.concat(program.exports[k]);
         }, []);
         return JSON.stringify({
           ok: true,
           compiled: program.compiled,
           exports: exports.map(function (e) { return { address: e.address, name: e.name }; }),
         });
       } catch (ex) {
         return JSON.stringify({
           ok: false,
           error: String(ex.description || ex.message || ex) +
             (ex.lineNumber != null ? " at line " + ex.lineNumber + ":" + ex.column : ""),
         });
       }
     }`,
    ctx,
  );
  return (source) => {
    const json = vm.runInContext(`__compile(${JSON.stringify(source)})`, ctx);
    return JSON.parse(json);
  };
}

/** Pack compiler output into the on-wire bytecode container. */
export function packBytecode(program) {
  let exportSize = 0;
  for (const sym of program.exports) exportSize += 4 + sym.name.length + 1;
  const buf = Buffer.alloc(8 + 4 * program.compiled.length + exportSize);
  let off = 0;
  off = buf.writeUInt32LE(4 * program.compiled.length, off);
  off = buf.writeUInt32LE(exportSize, off);
  for (const op of program.compiled) off = buf.writeInt32LE(op | 0, off);
  for (const sym of program.exports) {
    off = buf.writeUInt32LE(sym.address, off);
    off += buf.write(sym.name, off, "ascii");
    off = buf.writeUInt8(0, off);
  }
  return buf;
}

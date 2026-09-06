import fs from 'node:fs';
import { lxpBody } from './lxp.mjs';
const ip = process.argv[2]; const idx = process.argv.slice(3).map(Number);
const g = JSON.parse(fs.readFileSync('../web/public/gallery.json','utf8'));
const sleep = ms => new Promise(r=>setTimeout(r,ms));
const st = async () => { try { const r = await fetch(`http://${ip}/api/status`, {signal: AbortSignal.timeout(20000)}); return await r.json(); } catch(e){ return null; } };
for (const i of idx) {
  const p = g[i-1]; let res;
  try { const r = await fetch(`http://${ip}/api/code`, {method:'POST', headers:{'content-type':'application/octet-stream'}, body: await lxpBody('', p.source), signal: AbortSignal.timeout(30000)}); res = await r.text(); } catch(e) { res = 'PUSH ERR '+(e.cause?.code||e.message); }
  console.log(new Date().toISOString().slice(11,19), `#${i} ${p.name} push -> ${res.slice(0,70)}`);
  for (let k=0;k<3;k++){ await sleep(8000); const s = await st(); console.log('   +'+(k+1)*8+'s', s? `fps=${s.fps} heap=${s.heap_free} vmerr=${String(s.vmerr).slice(0,50)}` : 'UNREACHABLE'); }
}

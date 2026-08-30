// name: Post-Process Chain
// Original Luxel demo (not a port): the pattern itself draws nothing but
// hard-edged single-pixel sparks on black. Everything soft you see is the
// global post-process chain — setOutputPalette → setBlur → setGlow →
// setGamma — running once per frame after render(). Pull a slider to zero
// to switch that stage off and watch what it was doing.

export function sliderBlur(v) { blurAmt = v }      //# min=0 max=1 step=0.01 default=0.45
export function sliderGlow(v) { glowAmt = v }      //# min=0 max=1 step=0.01 default=0.55
export function sliderRecolor(v) { recolor = v }   //# min=0 max=1 step=0.01 default=1
export function sliderGamma(v) { gammaExp = v * 3 } //# min=0 max=1 step=0.01 default=0.6

var blurAmt = 0.45
var glowAmt = 0.55
var recolor = 1
var gammaExp = 1.8

// Sparks: one lit pixel each, walking the strip at its own speed. No
// anti-aliasing here on purpose — the chain is what makes them look good.
var COUNT = 7
var pos = array(COUNT)
var spd = array(COUNT)
var lvl = array(COUNT)

for (var i = 0; i < COUNT; i++) {
  pos[i] = random(1)
  spd[i] = 0.05 + random(0.25)
  if (random(1) < 0.5) spd[i] = -spd[i]
  lvl[i] = 0.6 + random(0.4)
}

// Output palette: indigo → electric blue → magenta → warm white. Flat
// [pos,r,g,b] quadruples, the same shape setPalette takes.
var pal = array(16)
pal[0]  = 0.00; pal[1]  = 0.04; pal[2]  = 0.00; pal[3]  = 0.22
pal[4]  = 0.40; pal[5]  = 0.10; pal[6]  = 0.35; pal[7]  = 0.95
pal[8]  = 0.75; pal[9]  = 0.95; pal[10] = 0.20; pal[11] = 0.70
pal[12] = 1.00; pal[13] = 1.00; pal[14] = 0.92; pal[15] = 0.70

export function beforeRender(delta) {
  var dt = delta / 1000

  for (var i = 0; i < COUNT; i++) {
    pos[i] = frac(pos[i] + spd[i] * dt + 1)
  }

  // The whole point of the demo: four calls, four whole-frame stages.
  // Each is off at 0, so the sliders bottom out to the raw sparks.
  setOutputPalette(pal, recolor)
  setBlur(blurAmt, 2)
  setGlow(glowAmt)
  setGamma(gammaExp)
}

export function render(index) {
  var v = 0
  for (var i = 0; i < COUNT; i++) {
    // one hard pixel per spark, no falloff
    if (floor(pos[i] * pixelCount) == index) v = max(v, lvl[i])
  }
  rgb(v, v, v)
}

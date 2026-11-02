// name: perlin fire wind tunnel
// Clean-room reimplementation from a prose functional description of the
// community pattern "perlin fire wind tunnel"; original source never consulted.

// Wind-blown fractal-noise flame twisted into a vortex around the panel
// center: rotation angle grows inversely with distance from center, so
// near pixels spiral hard and far pixels drift gently. A sinusoidal wind
// wobble sways the flame columns; hot cores bleach toward white.

// controls (with sane startup defaults)
var baseHue = 0      // fire ramp base color
var mode = 0         // 0 perlin, 1 ridge, 2 fbm, 3 turbulence
var density = 1      // zoom; also feeds twist strength and wobble weight
var wind = 0.5       // sway amplitude
var speed = 0.5      // texture streaming rate (up = faster)

//# min=0 max=1 step=0.01 default=0
export function sliderHue(v) { baseHue = v }
//# min=0 max=1 step=0.05 default=0
export function sliderMode(v) { mode = floor(v * 3.999) }
export function showNumberMode() { return mode }
//# min=0 max=1 step=0.01 default=0.27
export function sliderDensity(v) { density = 0.25 + v * 2.75 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderWind(v) { wind = v }
//# min=0 max=1 step=0.01 default=0.5
export function sliderSpeed(v) { speed = v }

// tile the noise lattice so both scroll axes wrap seamlessly
setPerlinWrap(16, 16, 16)

var rotPhase = 0     // tunnel rotation, a few seconds per revolution
var wob1 = 0         // wind wobble phases (two beating clocks)
var wob2 = 0
var zSlow = 0        // slow walk across one lattice repeat (minutes)
var yScroll = 0      // fast flame streaming, rate from the speed control

export function beforeRender(delta) {
  rotPhase = time(0.08) * PI2                 // ~5.2 s per revolution
  wob1 = time(0.11) * PI2                     // ~7.2 s
  wob2 = triangle(time(0.16)) * PI2           // ~10.5 s, smoothed
  zSlow = time(3) * 16                        // one lattice repeat in ~3.3 min
  // inverted speed mapping: slider up = shorter period = faster streaming
  yScroll = time(1 - speed * 0.94) * 16       // ~65 s down to ~4 s per repeat
}

export function render2D(index, x, y) {
  // center the panel and zoom by density
  x = (x - 0.5) * density
  y = (y - 0.5) * density

  // wind wobble: horizontal sway, stronger toward one vertical extreme
  x += wind * sin(y * 4 + wob1 + wob2) * 0.15 * (density - y)

  // tunnel twist: rotation inversely proportional to radius
  var ang = rotPhase + density / hypot(x, y)   // /0 yields 0 at dead center
  var c = cos(ang)
  var s = sin(ang)
  var xr = x * c - y * s
  var yr = x * s + y * c

  // flame intensity from the selected fractal-noise flavor
  var ny = yr * 0.5 + yScroll
  var v
  if (mode == 0) v = 2 * abs(perlin(xr, ny, zSlow, 0))
  else if (mode == 1) v = perlinRidge(xr, ny, zSlow, 2, 0.5, 1, 3)
  else if (mode == 2) v = 1.5 * abs(perlinFbm(xr, ny, zSlow, 2, 0.5, 3))
  else v = perlinTurbulence(xr, ny, zSlow, 2, 0.5, 3)
  v = clamp(v, 0, 1)

  // hot end drifts a little further along the wheel; cores bleach white;
  // fringes crushed dark by the cubic brightness curve
  var h = baseHue + v * 0.08
  var sat = min(1, 1.25 - v)
  hsv(h, sat, v * v * v)
}

// 1D fallback: sample a horizontal line through the tunnel
export function render(index) {
  render2D(index, index / pixelCount, 0.3)
}

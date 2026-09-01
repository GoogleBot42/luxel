// name: Eye of Sauron with movement
// Clean-room reimplementation from a prose functional description of the
// community pattern "Eye of Sauron with movement"; original source never
// consulted.

// A fiery lidless eye: ridge-noise flame tendrils stream outward from the
// center through a black-red-orange-yellow-white heat palette, fading out
// in an oval toward the rim. A dark vertical slit pupil sits at the gaze
// point, which darts to random targets with eased, saccade-like motion —
// quick twitches while focused, big sweeps after a long stare.

// heat palette: black -> pure red at ~1/5 -> orange/yellow at ~4/5 -> white
setPalette([
  0,    0, 0, 0,
  0.2,  1, 0, 0,
  0.8,  1, 0.85, 0.1,
  1,    1, 1, 1
])

// --- controls ---
var angDen = 8      // flame tendrils around the circle (whole numbers)
var radDen = 1.2    // radial stretch of the tendrils
var dilation = 0.5  // pupil size
var slitW = 0.14    // pupil horizontal compression (small = thin slit)

//# min=0 max=1 step=0.077 default=0.5
export function sliderAngularDensity(v) { angDen = 2 + floor(v * 12) }
//# min=0 max=1 step=0.01 default=0.3
export function sliderRadialDensity(v) { radDen = 0.3 + v * 3 }
//# min=0 max=1 step=0.01 default=0.5
export function sliderDilation(v) { dilation = 0.15 + v * 0.7 }
//# min=0 max=1 step=0.01 default=0.8
export function sliderSlitness(v) { slitW = 0.5 - v * 0.45 }

// --- gaze state ---
var gx = 0, gy = 0    // current eased gaze offset
var tx = 0, ty = 0    // gaze target being eased toward
var dwell = 0.5       // countdown to next retarget (seconds)
var prevDwell = 0.5   // length of the previous dwell (scales the next jump)

// --- looping noise phases ---
var WRAP = 16         // noise lattice repeat span on the flow/morph axes
var morph = 0         // very slow shape morph (full cycle ~4+ minutes)
var flow = 0          // radial outward streaming (full cycle ~49 s)
var burn = 0          // seconds-scale clock: the flame licks and flickers

export function beforeRender(delta) {
  var dt = delta / 1000

  // sawtooth clocks scaled to exactly the lattice wrap -> seamless loops
  morph = time(4) * WRAP
  flow = time(0.75) * WRAP
  burn = time(0.09) * WRAP

  // tile the noise: angle axis wraps at the tendril count (no seam at 2pi),
  // flow and morph axes wrap at WRAP
  setPerlinWrap(angDen, WRAP, WRAP)

  // recenter the 0..1 map, stretch ~3x in x and ~40% more in y (oval fade)
  resetTransform()
  translate(-0.5, -0.5)
  scale(3, 4.2)

  // exponential ease: cover ~40% of the remaining distance each frame
  gx += (tx - gx) * 0.4
  gy += (ty - gy) * 0.4

  dwell -= dt
  if (dwell <= 0) {
    // long stares earn big jumps; rapid twitches stay local
    var reach = clamp(prevDwell * 0.4, 0.05, 0.8)
    tx = (random(2) - 1) * reach
    ty = (random(2) - 1) * reach
    prevDwell = 0.03 + random(1.97)   // tens of ms .. ~2 s
    dwell = prevDwell
  }
}

export function render2D(index, x, y) {
  // radius stays centered (the flame ring doesn't move);
  // angle and pupil follow the gaze
  var r = hypot(x, y)
  var a = (atan2(y - gy, x - gx) / PI2 + 0.5) * angDen  // 0..angDen turn

  // ridged fractal noise: sharp creases = wispy licking flame filaments
  var v = perlinRidge(a, r * radDen - flow, morph, 2, 0.5, 1.05, 3)

  // oval edge fade: gentle inside, sharp at the rim
  var edge = clamp(2 - r, 0, 1)
  v = v * edge * edge

  // slit pupil: cone of darkness at the gaze point, x compressed by slitness
  var px = (x - gx) / slitW
  var py = (y - gy) / dilation
  var pd = hypot(px, py)
  var cone = dilation * (1 - pd / 3)
  if (cone > 0) v -= cone * 2

  // Everything below is a pure MULTIPLY on the post-pupil value, so it never
  // moves the zero crossing: the slit keeps exactly the size and shape it had.

  // Burning turbulence: the same ridged field at double frequency on a
  // seconds-scale clock. It flares and gutters every filament so the fire
  // seethes between darts instead of sitting still.
  var lick = perlinRidge(a * 2, r * radDen * 2 - burn, burn, 2, 0.5, 1.05, 2)
  v = v * (0.5 + 1.5 * lick)

  // White-hot corona hugging the slit: the pupil's own cone footprint (radius
  // 3 in slit units) stretched a third again, so the fire runs hottest right
  // where it laps the pupil's edge and cools toward the rim.
  var near = clamp(1 - pd / 4, 0, 1)
  v = v * (1 + 2.2 * near * near)

  // clamp to the palette top so hot spots never wrap back to black
  v = clamp(v, 0, 1)
  paint(v, v)
}

// name: millipede
// Clean-room reimplementation from a prose functional description of the
// community pattern "millipede"; original source never consulted.

// Two free-running clocks: t1 ~3.3 s, t2 ~6.6 s (drives the band travel).
// They are accumulated from delta rather than sampled from time() so the
// Speed control can stretch them; at Speed = 100% they run at exactly the
// periods time(0.05) / time(0.1) produce.
var T1_PERIOD = 3.2768
var T2_PERIOD = 6.5536
var t1 = 0, t2 = 0

var speedPct = 100   // crawl rate, % of the reference gait
var segCount = 10    // repeating "leg" segments along the whole strip
var spread = 0.5     // fraction of the hue wheel one segment sweeps
var gait = 1         // undulation depth, 1 = reference

//# min=10 max=300 step=5 default=100
export function sliderSpeed(v) { speedPct = v }

//# min=2 max=30 step=1 default=10
export function sliderSegments(v) { segCount = floor(v) }

//# min=10 max=100 step=5 default=50
export function sliderColorSpread(v) { spread = v / 100 }

//# min=0 max=200 step=5 default=100
export function sliderGaitDepth(v) { gait = v / 100 }

export function beforeRender(delta) {
  var dt = (delta / 1000) * speedPct * 0.01
  t1 = mod(t1 + dt / T1_PERIOD, 1)
  t2 = mod(t2 + dt / T2_PERIOD, 1)
}

export function render(index) {
  var p = index / pixelCount

  // Scrolling segmented ramp: bands travel one strip length per t2 cycle;
  // scaling by segCount*spread and wrapping at `spread` makes `segCount`
  // repeating segments, each sweeping `spread` of the hue wheel.
  var seg = mod((p + t2) * segCount * spread, spread)

  // Add a static end-to-end gradient and a slow SINUSOIDAL wobble of the
  // faster clock that shifts every hue together. The wobble's derivative is
  // what modulates the crawl speed, so it must be a sine (smooth, continuous
  // surge-and-stall) and not a triangle (whose derivative is a square wave —
  // two flat speed plateaus, no gait). Its depth sets how hard the legs
  // stall: at the trough the body nearly stops and the coarse colour envelope
  // swings backwards, which is what reads as the millipede taking a step.
  var h = seg + p + wave(t1) * 0.9 * gait

  // Brightness rides on the hue value itself (plus the slower clock's
  // phase) so the ripples stay locked to the color bands; squaring deepens
  // the troughs and sharpens the crests.
  var v = wave(h + t2)
  v = v * v

  hsv(h, 1, v)
}

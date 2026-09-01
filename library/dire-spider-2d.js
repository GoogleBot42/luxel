// name: Dire Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Dire Spider 2D"; original source never consulted.

// A glowing orange spider crawls across a 2D matrix, legs stepping and body
// swaying, re-entering from a random direction after each pass, while swirling
// toxic-green mist, rays and smoke boil outward around it.
//
// Two things carry the look:
// 1. The silhouette is exaggerated for low-resolution panels: a fat round
//    abdomen behind a smaller head, joined by a thin pedicel, and eight legs
//    that are longer than anatomically honest. Each leg is a femur plus a
//    tibia with a real angle break at the knee, drawn as a thick capsule
//    (point-to-segment distance), so the shape survives a 16x16 grid instead
//    of aliasing into loose specks.
// 2. All crawl motion lives in the map transform: recenter, rotate to the
//    heading, translate along the crawl axis. The spider is always drawn at
//    the origin and the world moves under it.
//
// Spider and haze are composited ADDITIVELY in RGB, so where a leg crosses a
// hot wisp the two clip together into yellow and white the way a real bloom
// would, rather than the haze being punched out by an opaque leg.

var crawlPeriod = 9       // seconds per pass
var t = 0                 // wall clock, seconds (wrapped hourly)
var lastPhase = 1
var heading = 0           // crawl direction, radians (re-rolled each pass)

// body: +x is the direction of travel, so the head leads
var HEAD_X = 0.055, HEAD_R = 0.07
var ABD_X = -0.215, ABD_R = 0.135
var WAIST_W = 0.045       // half-width of the pedicel joining the two lumps

var HIP_Y = 0.045         // hips sit just off the body axis
var GAIT_RATE = 1.7       // step cycles per second
var GAIT_AMP = 0.26       // radians of swing at the hip
var KNEE_K = 1.4          // the knee flexes rather more than the hip swings

// Four legs per side, mirrored across the body axis. Each hangs off its own
// hip station marching back from the head, so the eight legs do not fuse into
// one lump near the middle. Every femur cocks outward and only the shin turns
// to reach forward or back — that break is what reads as a joint.
var NLEG = 4
var legHX = array(NLEG)
var legA1 = array(NLEG)
var legL1 = array(NLEG)
var legA2 = array(NLEG)
var legL2 = array(NLEG)
var legPh = array(NLEG)

legHX[0] = 0.095; legA1[0] = 1.05; legL1[0] = 0.135; legA2[0] = 0.18; legL2[0] = 0.25; legPh[0] = 0
legHX[1] = 0.070; legA1[1] = 1.30; legL1[1] = 0.135; legA2[1] = 0.92; legL2[1] = 0.23; legPh[1] = PI
legHX[2] = 0.045; legA1[2] = 1.78; legL1[2] = 0.135; legA2[2] = 2.22; legL2[2] = 0.23; legPh[2] = 0
legHX[3] = 0.020; legA1[3] = 2.05; legL1[3] = 0.135; legA2[3] = 2.97; legL2[3] = 0.27; legPh[3] = PI

// sixteen segments: 8 legs x 2 bones, +y side first then the -y side
var NSEG = 16
var segAX = array(NSEG)
var segAY = array(NSEG)
var segDX = array(NSEG)
var segDY = array(NSEG)
var segInv = array(NSEG)   // 1 / |d|^2, for the projection
var segIW2 = array(NSEG)   // 1 / halfWidth^2

function setSeg(j, x0, y0, x1, y1, w) {
  var dx = x1 - x0
  var dy = y1 - y0
  segAX[j] = x0
  segAY[j] = y0
  segDX[j] = dx
  segDY[j] = dy
  segInv[j] = 1 / max(dx * dx + dy * dy, 0.000001)
  segIW2[j] = 1 / (w * w)
}

// Leg thickness as a fraction of display width (half-width of the femur; the
// shin is drawn a little finer). Real units, so a stock 0..1 slider clamps to
// the top of the range rather than misbehaving.
var legWidth = 0.055
//# min=0.03 max=0.09 step=0.005 default=0.055
export function sliderLegWidth(v) {
  legWidth = clamp(v, 0.03, 0.09)
}

// How hard the haunting glow, smoke and rays burn. 0 leaves a bare spider on
// black, 1 is the stock haze, 2 engulfs the panel.
var glow = 1
//# min=0 max=2 step=0.05 default=1
export function sliderGlowIntensity(v) {
  glow = clamp(v, 0, 2)
}

var bgLevel = 0.03
//# min=0 max=0.35 step=0.01 default=0.03
export function sliderBackgroundLevel(v) {
  bgLevel = clamp(v, 0, 0.35)   // real units, see above
}

var mistMorph, mistScroll, rayPhase
var ANG_DENSITY = 8
var RAYS = 6                      // god-rays per turn
setPerlinWrap(ANG_DENSITY, 0, 0)  // hide the seam at the angle wraparound

export function beforeRender(delta) {
  t = t + delta / 1000
  if (t > 3600) t -= 3600

  // Crawl position: sawtooth spanning ~1.4 display-widths off both edges
  var phase = frac(t / crawlPeriod)
  if (phase < lastPhase) heading = random(PI2)  // wrapped: new entry angle
  lastPhase = phase
  var crawl = -1.4 + 2.8 * phase

  var clock = t * PI2 * GAIT_RATE
  var femurW = legWidth
  var tibiaW = legWidth * 0.85

  // Rebuild every leg. Diagonally opposite legs step together (the alternating
  // tetrad a real spider walks with), so the two sides run half a cycle apart.
  for (var s = 0; s < 2; s++) {
    var sy = s < 1 ? 1 : -1
    var sidePh = s < 1 ? 0 : PI
    for (var i = 0; i < NLEG; i++) {
      var swing = GAIT_AMP * sin(clock + legPh[i] + sidePh)
      var a1 = legA1[i] + swing
      var a2 = legA2[i] + swing * KNEE_K
      var hx = legHX[i]
      var kx = hx + legL1[i] * cos(a1)
      var ky = HIP_Y + legL1[i] * sin(a1)
      var fx = kx + legL2[i] * cos(a2)
      var fy = ky + legL2[i] * sin(a2)
      var j = (s * NLEG + i) * 2
      setSeg(j, hx, HIP_Y * sy, kx, ky * sy, femurW)
      setSeg(j + 1, kx, ky * sy, fx, fy * sy, tibiaW)
    }
  }

  // Haze clocks: slow morph, faster radial outward scroll, slow ray swirl
  mistMorph = time(0.12) * 4        // ~7.9 s per lap, scaled to noise units
  mistScroll = time(0.05) * 3       // ~3.3 s per lap
  rayPhase = time(0.09)             // ~5.9 s per lap

  // World moves under the spider: recenter, rotate to heading, slide along
  // the crawl axis; a slice of the gait becomes lateral body sway.
  resetTransform()
  translate(-0.5, -0.5)
  rotate(heading)
  translate(-crawl + 0.012 * sin(clock * 2), 0.022 * sin(clock))
}

export function render2D(index, x, y) {
  var r = hypot(x, y)
  var ang = atan2(y, x) / PI2 + 0.5   // 0..1 turn

  // ---- spider ------------------------------------------------------------
  // Every leg stays on its own side of the body axis, so only the eight
  // segments of the near side can ever light this pixel.
  var base = y < 0 ? NSEG / 2 : 0
  var leg = 0
  for (var k = 0; k < 8; k++) {
    var j = base + k
    var dx = x - segAX[j]
    var dy = y - segAY[j]
    // nearest point on the bone, clamped to its ends -> round joints and caps
    var u = clamp((dx * segDX[j] + dy * segDY[j]) * segInv[j], 0, 1)
    var ox = dx - u * segDX[j]
    var oy = dy - u * segDY[j]
    var lv = 1 - (ox * ox + oy * oy) * segIW2[j]
    if (lv > leg) leg = lv
  }
  // hard contrast: the core of the stroke burns to full and only the rim
  // feathers, so a leg never dissolves into stray dim pixels
  leg = clamp(leg * 3, 0, 1)

  var dAbd = hypot(x - ABD_X, y)
  var dHead = hypot(x - HEAD_X, y)
  var body = max(clamp((ABD_R - dAbd) * 40, 0, 1), clamp((HEAD_R - dHead) * 40, 0, 1))
  var span = clamp((x - ABD_X) * 40, 0, 1) * clamp((HEAD_X - x) * 40, 0, 1)
  body = max(body, span * clamp((WAIST_W - abs(y)) * 40, 0, 1))

  var spider = max(leg, body)
  // the two body lumps stay a deep red; the legs warm through orange to amber
  // as they run out to the feet
  var warm = body >= leg ? 0.1 : 0.16 + clamp((dHead - HEAD_R) * 2.6, 0, 1) * 0.6

  // ---- haze: wisps + swirling rays + a corona, all radially attenuated ----
  var fall = clamp(1 - r * 0.95, 0, 1)
  fall = fall * fall

  var wisp = perlinRidge(ang * ANG_DENSITY, r * 3 - mistScroll, mistMorph, 2, 0.5, 1, 3)
  wisp = clamp(wisp, 0, 1)
  wisp = wisp * wisp                  // sharpened into tendrils, but not so
                                      // hard that only pinpoints survive

  // rays spiral outward because the angle is offset by the radius
  var ray = 0.5 + 0.5 * sin((ang + r * 0.4 + rayPhase) * PI2 * RAYS)
  ray = ray * ray

  // the corona is an annulus, dark at the very centre, so it wreathes the
  // spider instead of washing the silhouette out from underneath
  var halo = clamp(1 - r * 2.2, 0, 1)
  halo = halo * halo * clamp(r * 5, 0, 1)

  var mist = glow * (fall * (wisp * 1.1 + ray * 0.6 + 0.3) + halo * 0.85)
  // the spider's own body occludes most of the haze behind it; the rim still
  // adds, which is what makes the legs flare where they cross a hot wisp
  mist = mist * (1 - 0.8 * spider)
  mist = max(mist, bgLevel * (1 - spider))

  // ---- additive composite ------------------------------------------------
  // Spider light (red/amber) and haze light (green) sum, so a leg crossing a
  // hot wisp clips through yellow to white instead of masking the haze.
  var hot = max(mist - 1, 0)                     // hottest cores go whitish
  var gr = mist * 0.16 + hot * 0.7
  var gg = mist
  var gb = mist * 0.1 + hot * 0.8
  rgb(clamp(spider + gr, 0, 1), clamp(spider * warm + gg, 0, 1), clamp(gb, 0, 1))
}

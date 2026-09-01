// name: Crawling Spider 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Crawling Spider 2D"; original source never consulted.

// A red-orange spider crawls straight across the display (~9 s per pass),
// legs stepping in an alternating gait and body swaying, re-entering from a
// new random direction each pass. Requires a mapped 2D display.
//
// The silhouette is deliberately exaggerated so it still reads as a SPIDER on
// a 16x16 panel: a fat round abdomen trailing a smaller head, and eight legs
// that are longer than anatomically honest, each drawn as a two-segment
// knee-bent capsule several pixels thick. Legs are stored as line segments
// rebuilt once per frame in the spider's own frame; the per-pixel test is a
// point-to-segment distance, which gives round joints and caps for free.

var PERIOD = 9        // seconds per crossing (incl. off-screen dead time)
var SWEEP = 2.6       // travel spans ~2.6 display-widths
var BASE_HUE = 0.02   // warm red-orange

// body: +x is the direction of travel, so the head leads and the fat
// abdomen trails behind it
var HEAD_X = 0.06, HEAD_R = 0.07
var ABD_X = -0.215, ABD_R = 0.135
var WAIST_W = 0.045    // half-width of the pedicel joining the two lumps

var HIP_Y = 0.045     // hips sit just off the body axis

// stroke half-widths: thighs heavier than shins, so the legs taper outward
var FEMUR_W = 0.05
var TIBIA_W = 0.046

var GAIT_RATE = 1.6   // step cycles per second
var GAIT_AMP = 0.26   // radians of swing at the hip
var KNEE_K = 1.4      // the knee flexes rather more than the hip swings

// Four legs per side, mirrored to the other side. Each hangs off its own hip
// station along the body (they march back from the head, as on a real spider,
// which keeps the eight legs from fusing into one blob near the middle) and is
// a femur (angle from the +x travel axis, length) plus a tibia (its own angle,
// length). The angle break at the knee is what makes the limb read as jointed
// rather than as a spoke: every femur cocks outward and only the shin turns to
// reach forward or back.
var NLEG = 4
var legHX = array(NLEG)
var legA1 = array(NLEG)
var legL1 = array(NLEG)
var legA2 = array(NLEG)
var legL2 = array(NLEG)
var legPh = array(NLEG)

legHX[0] = 0.100; legA1[0] = 1.05; legL1[0] = 0.135; legA2[0] = 0.20; legL2[0] = 0.24; legPh[0] = 0
legHX[1] = 0.075; legA1[1] = 1.30; legL1[1] = 0.135; legA2[1] = 0.95; legL2[1] = 0.22; legPh[1] = PI
legHX[2] = 0.050; legA1[2] = 1.75; legL1[2] = 0.135; legA2[2] = 2.20; legL2[2] = 0.22; legPh[2] = 0
legHX[3] = 0.025; legA1[3] = 2.05; legL1[3] = 0.135; legA2[3] = 2.95; legL2[3] = 0.26; legPh[3] = PI

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

var t = 0             // wall-clock accumulator, seconds
var crawlAngle = 0
var lastPos = -10

export function beforeRender(delta) {
  t += delta / 1000
  if (t > 3600) t -= 3600  // wrap after a long period

  // linear sweep from well off one edge to well off the other
  var p = mod(t, PERIOD) / PERIOD
  var pos = (p - 0.5) * SWEEP

  // new pass: pick a fresh travel direction over the full circle
  if (pos < lastPos) crawlAngle = random(PI2)
  lastPos = pos

  var clock = t * PI2 * GAIT_RATE

  // Rebuild every leg. Diagonally opposite legs step together (the alternating
  // tetrad a real spider walks with), so the two sides are half a cycle apart.
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
      setSeg(j, hx, HIP_Y * sy, kx, ky * sy, FEMUR_W)
      setSeg(j + 1, kx, ky * sy, fx, fy * sy, TIBIA_W)
    }
  }

  // body bob: a little lateral sway plus a shorter surge along the travel
  // axis, both locked to the step cycle
  var sway = 0.022 * sin(clock)
  var surge = 0.012 * sin(clock * 2)

  // spider-centered frame: center origin, rotate to the travel axis, then
  // slide along it
  resetTransform()
  translate(-0.5, -0.5)
  rotate(crawlAngle)
  translate(-pos + surge, sway)
}

export function render2D(index, x, y) {
  // Every leg stays strictly on its own side of the body axis, so only the
  // eight segments of the near side can ever light this pixel.
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
    var v = 1 - (ox * ox + oy * oy) * segIW2[j]
    if (v > leg) leg = v
  }
  // hard contrast: the core of the stroke burns to full and only the outer
  // fifth feathers, so a leg never dissolves into stray dim pixels
  leg = clamp(leg * 3, 0, 1)

  var dAbd = hypot(x - ABD_X, y)
  var dHead = hypot(x - HEAD_X, y)
  // both body lumps are near-solid discs with a half-pixel soft rim, joined by
  // a thin pedicel so the two lumps read as one animal and not two blobs
  var body = max(clamp((ABD_R - dAbd) * 40, 0, 1), clamp((HEAD_R - dHead) * 40, 0, 1))
  var span = clamp((x - ABD_X) * 40, 0, 1) * clamp((HEAD_X - x) * 40, 0, 1)
  body = max(body, span * clamp((WAIST_W - abs(y)) * 40, 0, 1))

  // Colour separates the parts rather than blending them: the two body lumps
  // stay a deep saturated red while the legs run amber near the hub out to
  // yellow-green at the feet. On a 16x16 panel that hue break is what tells a
  // blob-with-sticks apart from a blob.
  if (body >= leg) {
    hsv(BASE_HUE, 1, body)
  } else {
    hsv(BASE_HUE + clamp((dHead - HEAD_R) * 0.55, 0, 0.2), 1, leg)
  }
}

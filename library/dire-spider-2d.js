// name: Dire Spider
// tags: panel, holiday
// Clean-room reimplementation from the prose specs docs/pattern-specs/dire-spider-2d.md,
// crawling-spider-2d.md — reimagined, not a replica.
// A blood-red spider with glowing eyes stalks across its lair in stop-and-go
// bursts, entering from a new direction each pass, while toxic mist boils
// outward from the lair's heart. Mist: ridged noise + spiral rays on a small
// polar-tabled canvas, once per frame, through paintCanvas. Spider: 16 jointed
// leg capsules (drawLine) and a body built from circles, in world coordinates.
// Glow 0 is the bare spider on black.

var CAP = 32                      // mist canvas cap per axis
var hv = array(CAP * CAP)         // mist intensity -> palette
var tabA = array(CAP * CAP)       // per-cell angle (turns) about the centre
var tabR = array(CAP * CAP)       // per-cell radius
var CW = 0, CH = 0, gw = -1, gh = -1, parity = 0

var pal = [0, 0, 0, 0,  0.3, 0, 0.16, 0.03,  0.65, 0.12, 0.75, 0.08,  1, 0.8, 1, 0.55]
setPalette(pal)
setPerlinWrap(8, 0, 0)            // the mist's angle axis wraps seamlessly

var passSec = 9
//# min=4 max=30 step=1 default=9
export function sliderPassSeconds(v) { passSec = clamp(v, 4, 30) }

var glowPct = 60
//# min=0 max=100 step=1 default=60
export function sliderGlowPercent(v) { glowPct = clamp(v, 0, 100) }

var legPct = 4.5
//# min=3 max=9 step=0.5 default=4.5
export function sliderLegThickness(v) { legPct = clamp(v, 3, 9) }

// body frame: +x is forward. Leg tables: hip station, femur/tibia rest angle
// and length, gait phase. Mirrored for the other side.
var HEAD_X = 0.055, HEAD_R = 0.07, ABD_X = -0.215, ABD_R = 0.135, HIP_Y = 0.045
var legHX = [0.095, 0.07, 0.045, 0.02]
var legA1 = [1.05, 1.3, 1.78, 2.05]
var legL1 = [0.16, 0.16, 0.15, 0.16]
var legA2 = [0.18, 0.92, 2.22, 2.97]
var legL2 = [0.25, 0.23, 0.23, 0.27]
var legPh = [0, PI, 0, PI]

var heading = 0, ox = 0.5, oy = 0.5, s = 0, pathEnd = 1
var spd = 1, pause = 0, gait = 0, morph = 0, scroll = 0, rayPh = 0
var cx = 0, cy = 0, hc = 1, hs = 0

// A pass runs exactly from where the spider's reach first touches the panel
// to where it leaves, so a new spider is always about to appear.
function newPass() {
  heading = random(PI2)
  var off = random(0.4) - 0.2     // the path misses the centre a little
  var dx = cos(heading), dy = sin(heading), R = 0.43
  if (abs(dx) < 0.01) dx = 0.01
  if (abs(dy) < 0.01) dy = 0.01
  ox = 0.5 - dy * off
  oy = 0.5 + dx * off
  var a = (-R - ox) / dx, b = (1 + R - ox) / dx, c = (-R - oy) / dy, d = (1 + R - oy) / dy
  s = max(min(a, b), min(c, d))
  pathEnd = min(max(a, b), max(c, d))
  // trim the box's corners: walk in until the reach really touches the panel
  while (s < pathEnd && rectDist(ox + dx * s, oy + dy * s) > R) s += 0.02
  while (pathEnd > s && rectDist(ox + dx * pathEnd, oy + dy * pathEnd) > R) pathEnd -= 0.02
}
function rectDist(x, y) { return hypot(max(abs(x - 0.5) - 0.5, 0), max(abs(y - 0.5) - 0.5, 0)) }
newPass()

// body-frame point -> world
function wx(bx, by) { return cx + bx * hc - by * hs }
function wy(bx, by) { return cy + bx * hs + by * hc }

function buildTables() {
  gw = gridWidth()
  gh = gridHeight()
  CW = gw > 0 ? min(gw, CAP) : CAP
  CH = gh > 0 ? min(gh, CAP) : CAP
  for (var r = 0; r < CH; r++) {
    for (var c = 0; c < CW; c++) {
      var dx = (c + 0.5) / CW - 0.5
      var dy = (r + 0.5) / CH - 0.5
      tabA[r * CW + c] = atan2(dy, dx) / PI2 + 0.5
      tabR[r * CW + c] = hypot(dx, dy)
    }
  }
}

export function beforeRender(delta) {
  var dt = min(delta, 100) / 1000
  if (gridWidth() != gw || gridHeight() != gh) buildTables()

  // stop-and-go: freeze for a beat now and then while on screen
  if (pause > 0) {
    pause -= dt
    spd = max(spd - dt * 6, 0)
  } else {
    spd = min(spd + dt * 3, 1)
    if (abs(s) < 0.45 && random(1) < dt * 0.35) pause = 0.5 + random(0.9)
  }
  s += spd * dt * 2.2 / passSec     // ~2.2 panel units per pass, pauses aside
  if (s > pathEnd) newPass()
  gait = mod(gait + spd * dt * 1.8, 1)

  hc = cos(heading)
  hs = sin(heading)
  var clock = gait * PI2
  cx = ox + hc * s - hs * 0.02 * sin(clock)
  cy = oy + hs * s + hc * 0.02 * sin(clock)

  morph = morph + dt * 0.5
  if (morph > 200) morph -= 200
  scroll = mod(scroll + dt * 0.9, 256)
  rayPh = time(0.09)
  setBlur(gw > CW ? 0.4 : 0)        // soften the coarse mist on big panels

  // the mist drifts slowly, so each frame refreshes alternate canvas rows
  var g = glowPct / 62
  if (g <= 0) return
  parity = 1 - parity
  for (var row = parity; row < CH; row += 2) for (var i = row * CW; i < row * CW + CW; i++) {
    var a = tabA[i], r = tabR[i]
    var w = clamp(perlinRidge(a * 8, r * 3 - scroll, morph, 2, 0.5, 1, 2), 0, 1)
    var ray = 0.5 + 0.5 * sin((a + r * 0.4 + rayPh) * PI2 * 6)
    var fall = clamp(1.05 - r * 1.1, 0.12, 1)
    hv[i] = min(g * fall * (w * w * 1.1 + ray * ray * 0.4 + 0.18), 0.999)
  }
}

// the eight legs as hip, knee, foot points in world space
var hipX = array(8), hipY = array(8), kneeX = array(8), kneeY = array(8)
var footX = array(8), footY = array(8)
function buildLegs() {
  for (var j = 0; j < 8; j++) {
    var i = j % 4, side = j < 4 ? 1 : -1
    var sw = 0.26 * sin(gait * PI2 + legPh[i] + (side > 0 ? 0 : PI))
    var a1 = legA1[i] + sw, a2 = legA2[i] + sw * 1.4
    var kx = legHX[i] + legL1[i] * cos(a1), ky = HIP_Y + legL1[i] * sin(a1)
    var fx = kx + legL2[i] * cos(a2), fy = ky + legL2[i] * sin(a2)
    hipX[j] = wx(legHX[i], HIP_Y * side); hipY[j] = wy(legHX[i], HIP_Y * side)
    kneeX[j] = wx(kx, ky * side); kneeY[j] = wy(kx, ky * side)
    footX[j] = wx(fx, fy * side); footY[j] = wy(fx, fy * side)
  }
}

export function renderBulk() {
  if (glowPct > 0) paintCanvas(hv, CW, CH)
  else clear()

  // a poisonous corona rides with the spider
  rgb(0.02, glowPct / 400, 0.01)
  splat(cx, cy, 0.42, 1)

  // legs: dark rims punched through the mist first, then the solid cores
  buildLegs()
  var femW = legPct / 100, tibW = femW * 0.8, j
  rgb(0.5, 0.06, 0)
  for (j = 0; j < 8; j++) {
    drawLine(hipX[j], hipY[j], kneeX[j], kneeY[j], femW * 1.35, 0)
    drawLine(kneeX[j], kneeY[j], footX[j], footY[j], tibW * 1.35, 0)
  }
  for (j = 0; j < 8; j++) {
    rgb(0.85, 0.1, 0)
    drawLine(hipX[j], hipY[j], kneeX[j], kneeY[j], femW, 1)
    rgb(1, 0.32, 0)
    drawLine(kneeX[j], kneeY[j], footX[j], footY[j], tibW, 1)
  }

  // pedicel, then abdomen and head each over a dark rim; gloss; glowing eyes
  var px = 1.2 / max(gridWidth(), 16)
  var ax = wx(ABD_X, 0), ay = wy(ABD_X, 0), hx = wx(HEAD_X, 0), hy = wy(HEAD_X, 0)
  rgb(0.6, 0.04, 0)
  drawLine(ax, ay, hx, hy, 0.05, 2)
  rgb(0.12, 0, 0)
  fillCircle(ax, ay, ABD_R + px)
  fillCircle(hx, hy, HEAD_R + px)
  rgb(0.7, 0.03, 0.01)
  fillCircle(ax, ay, ABD_R)
  rgb(0.75, 0.08, 0)
  fillCircle(hx, hy, HEAD_R)
  rgb(0.5, 0.18, 0.08)
  splat(wx(ABD_X + 0.04, -0.04), wy(ABD_X + 0.04, -0.04), ABD_R * 0.6, 1)
  rgb(0.55, 0.8, 0.15)
  var er = max(0.03, px * 1.3)
  for (var e = -1; e <= 1; e += 2) {
    splat(wx(HEAD_X + 0.035, 0.03 * e), wy(HEAD_X + 0.035, 0.03 * e), er, 1)
    splat(wx(HEAD_X + 0.035, 0.03 * e), wy(HEAD_X + 0.035, 0.03 * e), er, 1)
  }
}

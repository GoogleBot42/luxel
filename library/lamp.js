// name: Lamp
// Clean-room reimplementation from the prose spec(s) docs/pattern-specs/utilitycolortemp.md,
// single-color-picker-wide-or-spot.md, naturallightsync.md, real-world-lights.md — reimagined, not a replica.
// Utility: a solid lamp colour, not an animation — the fixture as a room light,
// so a steady flat tile is it working. Color: any picked colour. White: a blackbody white
// by Kelvin. Follow the day: warm at night, easing to a daylight white across
// sunrise and back across sunset (needs the device clock). Preset: six real lamps.
// Optional spot: the light gathered into a soft-edged pool (anti-aliased ranges on
// a strip, a filtered disc canvas on a panel). Without the spot a frame is one fill().

var mode = 1           // 0 colour, 1 white, 2 follow the day, 3 preset
//# min=0 max=3 step=1 default=1
export function sliderMode(v) { mode = clamp(floor(v), 0, 3); dirty = 1 }

// Colour mode's colour. The picker's brightness is ignored: use the global brightness.
var pickH = 0.78, pickS = 1
export function hsvPickerColor(h, s, v) { pickH = h; pickS = s; dirty = 1 }

var kelvin = 2700      // White mode
//# min=1500 max=10000 step=100 default=2700
export function sliderKelvin(v) { kelvin = clamp(v, 1500, 10000); dirty = 1 }

var preset = 1         // candle-warm, incandescent, halogen, fluorescent, daylight, overcast
//# min=0 max=5 step=1 default=1
export function sliderPreset(v) { preset = clamp(floor(v), 0, 5); dirty = 1 }

var sunriseH = 6.5     // Follow the day
//# min=4 max=9 step=0.25 default=6.5
export function sliderSunriseHour(v) { sunriseH = clamp(v, 4, 9) }

var sunsetH = 20
//# min=16 max=22 step=0.25 default=20
export function sliderSunsetHour(v) { sunsetH = clamp(v, 16, 22) }

var dayK = 5000
//# min=4000 max=6500 step=100 default=5000
export function sliderDayKelvin(v) { dayK = clamp(v, 4000, 6500) }

var nightK = 2200
//# min=1800 max=3000 step=100 default=2200
export function sliderNightKelvin(v) { nightK = clamp(v, 1800, 3000) }

var spot = 0
//# default=0
export function toggleSpot(on) { spot = on ? 1 : 0; dirty = 1 }

var spotPos = 50       // % along the strip / across the panel
//# min=0 max=100 step=1 default=50
export function sliderSpotPositionPercent(v) { spotPos = clamp(v, 0, 100); dirty = 1 }

var spotWidth = 30     // % of the strip length / panel width
//# min=5 max=100 step=1 default=30
export function sliderSpotWidthPercent(v) { spotWidth = clamp(v, 5, 100); dirty = 1 }

var softness = 50      // % of the spot's half-width that fades
//# min=0 max=100 step=1 default=50
export function sliderSoftnessPercent(v) { softness = clamp(v, 0, 100); dirty = 1 }

// Blackbody white: the well-known piecewise fit to the published blackbody tables,
// in hundreds of kelvin. (The same function lives in sunrise-alarm-clock.js.)
var kr = 1, kg = 1, kb = 1
function kelvinRGB(k) {
  var t = clamp(k, 1000, 15000) / 100
  if (t <= 66) {
    kr = 1
    kg = 0.39008157876 * log(t) - 0.63184144378
    kb = t <= 19 ? 0 : 0.54320678911 * log(t - 10) - 1.19625408914
  } else {
    kr = 1.29293618606 * pow(t - 60, -0.1332047592)
    kg = 1.12989086089 * pow(t - 60, -0.0755148492)
    kb = 1
  }
  kr = saturate(kr); kg = saturate(kg); kb = saturate(kb)
}

var presetK = [1900, 2700, 3200, 4100, 5600, 6500]
var hsvOut = array(3)
var cr = 1, cg = 1, cb = 1, ch = 0, cs = 0, dirty = 1, lastK = 0

// Follow the day: night white, eased up to the day white over the hour around
// sunrise and back down over the hour around sunset.
function dayKelvin() {
  var h = clockHour() + clockMinute() / 60
  var up = smoothstep(sunriseH - 0.5, sunriseH + 0.5, h)
  var down = 1 - smoothstep(sunsetH - 0.5, sunsetH + 0.5, h)
  return round(nightK + (dayK - nightK) * up * down)
}

export function beforeRender(delta) {
  if (mode == 2) {
    var k = dayKelvin()
    if (k != lastK) { lastK = k; dirty = 1 }
  }
  if (!dirty) return
  if (mode == 0) {
    hsv2rgb(pickH, pickS, 1, hsvOut)
    cr = hsvOut[0]; cg = hsvOut[1]; cb = hsvOut[2]
  } else {
    kelvinRGB(mode == 1 ? kelvin : mode == 2 ? dayKelvin() : presetK[preset])
    cr = kr; cg = kg; cb = kb
  }
  rgb2hsv(cr, cg, cb, hsvOut)
  ch = hsvOut[0]; cs = hsvOut[1]
  buildCanvas = 1
  dirty = 0
}

// A mapless strip stays a strip: this export suppresses the default grid.
// Luxel always runs renderBulk.
export function render(index) { rgb(cr, cg, cb) }

// --- panel spot: a disc canvas, rebuilt only when a control moves -------------
var CAP = 48
var disc = array(CAP * CAP)
var cw = 0, chh = 0, buildCanvas = 1
function panelSpot() {
  var gw = gridWidth(), gh = gridHeight()
  var w = gw > 0 ? min(gw, CAP) : 32, h = gh > 0 ? min(gh, CAP) : 32
  if (buildCanvas || w != cw || h != chh) {
    cw = w; chh = h; buildCanvas = 0
    var asp = gw > 0 && gh > 0 ? gh / gw : 1          // keep the disc round
    var R = spotWidth / 200, edge = max(R * softness / 100, 0.6 / cw)
    for (var y = 0; y < chh; y++) {
      var dy = ((y + 0.5) / chh - 0.5) * asp
      for (var x = 0; x < cw; x++) {
        var e = saturate((R - hypot((x + 0.5) / cw - spotPos / 100, dy)) / edge)
        disc[y * cw + x] = e * e * (3 - 2 * e)
      }
    }
  }
  fillCanvas(ch, cs, disc, cw, chh, 1)
}

// --- strip spot: anti-aliased core plus two linear shoulders ------------------
function stripSpot() {
  var n = pixelCount, c = spotPos / 100 * n, half = spotWidth / 200 * n
  var e = max(half * softness / 100, 0.5)
  clear()
  rgb(cr, cg, cb)
  if (half - e > 0) fillRangeAA(c - half + e, c + half - e)
  gradientRange(c - half, c - half + e, ch, cs, 0, ch, cs, 1, 1)
  gradientRange(c + half - e, c + half, ch, cs, 1, ch, cs, 0, 1)
}

export function renderBulk() {
  if (!spot) {
    rgb(cr, cg, cb)
    fill()
  } else if (has2DMap()) panelSpot()
  else stripSpot()
}

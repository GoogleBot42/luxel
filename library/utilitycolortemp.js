// name: UtilityColorTemp
// Clean-room reimplementation from a prose functional description of the
// community pattern "UtilityColorTemp"; original source never consulted.

// Fills the whole display with one solid color on the blackbody locus,
// chosen by a color-temperature slider (~1000..15000 K). Parked at the
// very bottom of travel, it demos itself: the temperature sweeps back and
// forth between roughly candle (1000 K) and daylight (8000 K).
// Temperature -> RGB uses the well-known piecewise analytic fit to the
// published blackbody tables (the Tanner Helland approximation approach),
// working in hundreds of kelvin.

var sliderPos = 0    // default: bottom of travel = auto-sweep demo
var rC = 1
var gC = .6
var bC = .2

//# min=0 max=1 step=0.01 default=0
export function sliderColorTemperature(v) { sliderPos = v }

export function beforeRender(delta) {
  var kelvin
  if (sliderPos < 1 / 15) {
    // demo: triangle sweep, candle to daylight, ~10 s cycle
    kelvin = 1000 + triangle(time(.15)) * 7000
  } else {
    kelvin = 1000 + sliderPos * 14000
  }

  var t = kelvin / 100   // work in hundreds of kelvin
  if (t <= 66) {
    rC = 1
    gC = (99.4708025861 * log(t) - 161.1195681661) / 255
    if (t <= 19) {
      bC = 0
    } else {
      bC = (138.5177312231 * log(t - 10) - 305.0447927307) / 255
    }
  } else {
    rC = 329.698727446 * pow(t - 60, -.1332047592) / 255
    gC = 288.1221695283 * pow(t - 60, -.0755148492) / 255
    bC = 1
  }
  rC = saturate(rC)
  gC = saturate(gC)
  bC = saturate(bC)
}

export function render(index) {
  rgb(rC, gC, bC)
}

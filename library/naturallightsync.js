// name: NaturalLightSync
// Clean-room reimplementation from a prose functional description of the
// community pattern "NaturalLightSync"; original source never consulted.
//
// Circadian lamp: the whole strip is one uniform white whose color
// temperature follows the time of day. Between sunrise and sunset the
// temperature rides a downward-opening parabola — warmest at sunrise and
// sunset, coolest at solar noon. Overnight it holds the warmest setting.
// Requires the device clock to be set (NTP / app); with no wall time the
// clock reads midnight and the strip sits at its warmest white.

// ---- configuration (edit-the-source constants) ----
var sunriseHour = 7    // 24h clock, fractional hours allowed
var sunsetHour = 19
var maxKelvin = 6500   // coolest (solar noon) color temperature
var minKelvin = 2200   // warmest (night / sunrise / sunset)

// watchable for debugging
export var kelvin = minKelvin
export var timeOfDay = 0

var r = 1, g = 1, b = 1

// Standard piecewise blackbody (Kelvin -> RGB) curve fit. Works on
// t = kelvin / 100; channels computed in 0..255 then normalized/clamped.
function kelvinToRgb(k) {
  var t = k / 100
  var red, grn, blu
  if (t <= 66) {
    red = 255
    grn = 99.4708025861 * log(t) - 161.1195681661
    if (t <= 19) {
      blu = 0
    } else {
      blu = 138.5177312231 * log(t - 10) - 305.0447927307
    }
  } else {
    red = 329.698727446 * pow(t - 60, -0.1332047592)
    grn = 288.1221695283 * pow(t - 60, -0.0755148492)
    blu = 255
  }
  r = clamp(red / 255, 0, 1)
  g = clamp(grn / 255, 0, 1)
  b = clamp(blu / 255, 0, 1)
}

export function beforeRender(delta) {
  timeOfDay = clockHour() + clockMinute() / 60 + clockSecond() / 3600

  if (timeOfDay > sunriseHour && timeOfDay < sunsetHour) {
    // Downward parabola: peak (maxKelvin) at solar noon, exactly
    // minKelvin at sunrise and sunset.
    var noon = (sunriseHour + sunsetHour) / 2
    var u = (timeOfDay - noon) / (noon - sunriseHour)  // -1..1 across the day
    kelvin = maxKelvin - (maxKelvin - minKelvin) * u * u
  } else {
    kelvin = minKelvin
  }

  kelvinToRgb(kelvin)
}

export function render(index) {
  rgb(r, g, b)
}

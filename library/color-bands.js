// name: color bands
// Clean-room reimplementation from a prose functional description of the
// community pattern "color bands"; original source never consulted.
//
// A double rainbow along the strip seen through a shifting moire mask:
// short bright bands crawl and interfere, with narrow spots briefly washing
// to white. Purely per-pixel, no state, no randomness; two free-running
// clocks (~10 s and ~15 s) drift the interfering spatial waves.

export function beforeRender(delta) {
  c10 = time(0.153)   // ~10.0 s cycle
  c15 = time(0.229)   // ~15.0 s cycle
}

export function render(index) {
  // hue: normalized position traversing the wheel twice, wrapping past 1
  var h = (index / pixelCount) * 2

  // saturation: short-period wave drifting slowly, ^4 -> narrow white spots
  var sw = wave(index / 3 + c15)
  var s = 1 - sw * sw * sw * sw

  // brightness: three non-harmonic short-period waves; two drift one way and
  // are multiplied, the third drifts the other way and is added, then ^4
  var wa = wave(index / 5 + c10)
  var wb = wave(index / 7 + c10)
  var wc = wave(index / 11 - c10)
  var m = wa * wb + wc
  var v = m * m * m * m     // renderer clamps the >1 plateaus

  hsv(h, s, v)
}

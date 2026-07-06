// name: lightning ZAP!
// Clean-room reimplementation from a prose functional description of the
// community pattern "lightning ZAP!"; original source never consulted.

// White lightning-bolt segments flash on and rapidly fade to black. Each
// flash lights a short contiguous run at full brightness, continuing from
// where the last one ended, so the bolt zaps its way down the strip in
// staccato bursts. After a full pass the strip rests dark for up to about
// a second, then a new pass starts from the beginning.

var bri = array(pixelCount)  // per-pixel brightness, the only render input
var cursor = 0               // where the next flash segment begins
var timer = 0                // seconds until the next flash fires

// segment length bounds scale with the strip (~1/15 .. ~1/6 of it)
var minSeg = max(1, floor(pixelCount / 15))
var maxSeg = max(2, floor(pixelCount / 6))

export function beforeRender(delta) {
  var dt = delta / 1000

  // exponential fade (a full-brightness pixel dies in ~a tenth of a second)
  // plus a tiny constant bleed so values reach true zero
  var keep = max(0, 1 - dt * 12)
  for (var i = 0; i < pixelCount; i++) {
    var v = bri[i] * keep - 0.004
    bri[i] = v > 0 ? v : 0
  }

  timer -= dt
  if (timer <= 0) {
    // fire a flash: a random-length white segment starting at the cursor
    var seg = minSeg + floor(random(maxSeg - minSeg + 1))
    while (seg > 0 && cursor < pixelCount) {
      bri[cursor] = 1
      cursor += 1
      seg -= 1
    }

    if (cursor >= pixelCount) {
      // pass complete: long dark rest, then start over
      cursor = 0
      timer = 0.35 + random(0.75)
    } else {
      // squared-uniform delay: mostly tens of ms, occasionally a few hundred
      var d = 0.07 + random(0.55)
      timer = d * d
    }
  }
}

export function render(index) {
  hsv(0, 0, bri[index])  // zero saturation: pure white at stored brightness
}

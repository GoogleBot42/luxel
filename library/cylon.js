// name: Cylon
// Clean-room reimplementation from a prose functional description of the
// community pattern "Cylon"; original source never consulted.

// Trailing-buffer scanner: one bright head sweeps back and forth, stamping
// full intensity into a persistent per-pixel buffer that decays linearly
// each frame. Cubing the buffer at render time turns the linear tail into
// a short, punchy glow.

var buf = array(pixelCount)
var head = 0
var dir = 1

var hue = 0            // default: red
var sat = 1

// speed in pixels per millisecond; default ~ one pass per second
var speed = pixelCount / 1000
// fade in intensity per millisecond; default full-to-black in ~1 s
var fadeRate = 1 / 1000

export function hsvPickerColor(h, s, v) {
  hue = h
  sat = s
  // picked brightness intentionally unused (matches the original's look)
}

//# min=0 max=1 step=0.01 default=0.45
export function sliderSpeed(v) {
  // small floor so it never fully stops; top end ~2.4 strip-lengths/second
  speed = (0.05 + 2.35 * v) * pixelCount / 1000
}

//# min=0 max=1 step=0.01 default=0.4
export function sliderFade(v) {
  // small floor so the tail always decays; high end = only a few pixels
  fadeRate = (0.2 + 5 * v) / 1000
}

export function beforeRender(delta) {
  // move the head, frame-rate independent
  head += dir * speed * delta
  if (head >= pixelCount - 1) {
    head = pixelCount - 1
    dir = -1
  }
  if (head <= 0) {
    head = 0
    dir = 1
  }

  // decay the whole buffer linearly, then stamp the head at full
  var dec = fadeRate * delta
  for (var i = 0; i < pixelCount; i++) {
    var v = buf[i] - dec
    buf[i] = v > 0 ? v : 0
  }
  buf[floor(head)] = 1
}

export function render(index) {
  var v = buf[index]
  hsv(hue, sat, v * v * v)
}

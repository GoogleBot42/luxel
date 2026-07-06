// Clean-room reimplementation from a prose description of the community
// pattern "Thunderstorm" (no source consulted), with two improvements the
// description recommended: the cloud anchors to the strip's end (not a
// hard-coded index) and timing is delta-based (not frame-counted).
// Layered overwrites, no blending: rain, ember glints, lightning, and the
// cloud painted last so flashes appear to come from behind it.
cloudSize = 8
lightning = 0
nextFlash = 2
edgeT = 0
gap = 6

export function sliderRainDensity(v) {
  gap = 2 + floor((1 - v) * pixelCount / 4)
} //# min=0 max=1 step=0.01 default=0.7

export function beforeRender(delta) {
  dt = delta * 0.001
  edgeT += dt
  if (edgeT > 0.25) {  // the cloud's edge shimmers a few times a second
    edgeT = 0
    cloudSize = floor(pixelCount / 9 + random(pixelCount / 14))
  }
  nextFlash -= dt
  if (lightning > 0) lightning -= dt
  if (nextFlash <= 0) {
    lightning = 0.12
    nextFlash = 0.4 + random(6)  // irregular gaps, constant flash length
  }
  scan = floor(time(0.09) * pixelCount)  // rain marches away from the cloud
  cloudV = 0.35 + 0.65 * min(time(0.06) * 1.5, 1)  // swell, then hold
}

export function render(index) {
  h = 0.66
  s = 1
  v = 0
  if ((index + scan) % gap == 0) v = 0.8  // deep blue drops
  if (random(1) < 0.02) {  // stray ember glints
    h = 0.07
    v = 1
  }
  if (lightning > 0) {  // whole-strip amber flash
    h = 0.09
    s = 0.55
    v = 1
  }
  if (index >= pixelCount - cloudSize) {  // white cloud, always on top
    s = 0
    v = cloudV
  }
  hsv(h, s, v)
}

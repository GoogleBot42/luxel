// name: Iran - Solidarity
// Clean-room reimplementation from a prose functional description of the
// community pattern "Iran - Solidarity"; original source never consulted.

// A solidarity tribute on a 3D map treated as a cylinder: a waving
// green/white/red tricolor wraps the vertical axis, a black glitching
// "void" sector sweeps around it (breathing wider and narrower), and a
// pulsing emblem — circle or heart, morphing from near-black to gold —
// rides the safe zone a quarter-turn behind the void so it is never
// scrubbed out. Everything is dimmed by a somber master "gloom".
// 3D only (azimuth from x/y about the map center, height from z).

// live-settable parameters (variable watcher / websocket API)
export var waveSpeed = 1        // flag cloth ripple speed (0 freezes)
export var voidSpeed = 0.9      // void rotation, rad/s; sign = direction
export var breatheSpeed = 0.5   // void width breathing speed
export var trailLength = 0.2    // ghost-trail length, fraction of a turn (0 = off)
export var gloom = 0.55         // master darkness: 0 bright .. 1 black
export var glitchDensity = 0.85 // probability a void pixel is black
export var voidDim = 0.15       // brightness of surviving void pixels
export var clearWidth = 0.45    // base clear-zone width, fraction of a turn
export var breatheWidth = 0.2   // extra clear width added by breathing
export var emblemMorph = 1      // 0 = dark void emblem .. 1 = golden sun
export var emblemSize = 0.16
export var emblemNudge = 0      // manual angular nudge, turns
export var emblemHeart = 1      // 0 = circle, 1 = heart
export var heartSpeed = 1       // heartbeat base speed
export var heartMult = 3.5      // heartbeat speed multiplier
export var pulseDepth = 0.55    // how far the emblem fades each beat

// phase accumulators: advanced only when their speed is nonzero, so
// zeroing a speed freezes that motion in place instead of snapping
var wfPhase = 0, vrPhase = 0, vbPhase = 0, hbPhase = 0
var voidHalf = 1, safeAz = 0, pulse = 1, dirSign = 1

export function beforeRender(delta) {
  var dt = min(delta / 1000, 0.05)
  if (waveSpeed != 0) wfPhase += waveSpeed * dt * 2
  if (voidSpeed != 0) vrPhase += voidSpeed * dt
  if (breatheSpeed != 0) vbPhase += breatheSpeed * dt
  hbPhase += heartSpeed * heartMult * dt

  var clear = clamp(clearWidth + breatheWidth * (0.5 + 0.5 * sin(vbPhase)), 0.05, 0.95)
  voidHalf = (1 - clear) * PI
  dirSign = voidSpeed >= 0 ? 1 : -1
  // safe zone rides a quarter-turn behind the void, plus the user nudge
  safeAz = vrPhase - PI / 2 + emblemNudge * PI2
  pulse = 1 - pulseDepth * (0.5 + 0.5 * sin(hbPhase))
}

export function render3D(index, x, y, z) {
  var az = atan2(y - 0.5, x - 0.5)
  // two-lobed cloth ripple offsets the height used for the bands
  var h = z + 0.1 * sin(2 * az + wfPhase)

  // 1) waving tricolor: red low, cool white mid, teal-green high
  var tR = smoothstep(0.3, 0.4, h)
  var tG = smoothstep(0.6, 0.7, h)
  var r = mix(1, 0.85, tR)
  var g = mix(0.02, 0.92, tR)
  var b = mix(0.02, 1, tR)
  r = mix(r, 0.03, tG)
  g = mix(g, 0.55, tG)
  b = mix(b, 0.32, tG)

  // 2) emblem, only solidly inside the white band
  var whiteMask = tR * (1 - tG)
  if (whiteMask > 0.85 && emblemSize > 0) {
    var adiff = mod(az - safeAz + PI, PI2) - PI
    var hx = abs(adiff) * 0.22        // angular distance as horizontal dist
    var vy = h - 0.5
    if (emblemHeart) vy = (vy + sqrt(hx) * 0.16) * 1.12  // one-line heart
    var dEm = hypot(hx, vy)
    var a = (1 - smoothstep(emblemSize * 0.8, emblemSize, dEm)) * pulse
    r = mix(r, mix(0.02, 1, emblemMorph), a)
    g = mix(g, mix(0.02, 0.72, emblemMorph), a)
    b = mix(b, mix(0.02, 0.08, emblemMorph), a)
  }

  // 3) void sector: ghost trail near the leading edge, glitchy static after
  var dAz = mod(az - vrPhase + PI, PI2) - PI
  if (abs(dAz) < voidHalf) {
    var fromLead = dirSign > 0 ? voidHalf - dAz : voidHalf + dAz
    var tl = trailLength * PI2
    if (trailLength > 0 && fromLead < tl) {
      var f = 1 - fromLead / tl
      f = f * f
      r *= f
      g *= f
      b *= f
    } else if (random(1) < glitchDensity) {
      r = 0
      g = 0
      b = 0
    } else {
      r *= voidDim
      g *= voidDim
      b *= voidDim
    }
  }

  // 4) gloom
  var m = max(1 - gloom, 0)
  rgb(r * m, g * m, b * m)
}

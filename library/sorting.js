// name: Sorting
// tags: both, ambient
// Curated original (hand-written showcase of the Luxel language/builtins).
// A shuffled rainbow puts itself back in order, live: bubble sort ripples, insertion sort
// slides, shell sort hops, heap sort pulses, and the pair being compared flashes white.
// Sorted, a bright sweep checks it end to end, it holds with a sparkle, then shatters into
// a new shuffle in the next palette. Every algorithm is a resumable step machine paced so a
// sort takes about Sort Time. Strip: one value per pixel, one fillHSV; panel: a bar per
// column, height and hue from the value, one blit each.

var algo = 0            // 0 auto (all four in turn), 1 bubble, 2 insertion, 3 shell, 4 heap
//# min=0 max=4 step=1 default=0
export function sliderAlgorithm(v) {
  var a = clamp(floor(v), 0, 4)
  if (a != algo) { algo = a; phase = 3; t = 0; i = M - 1 }   // re-shuffle into the new choice
}

var sortS = 20          // target seconds per sort
//# min=4 max=90 step=1 default=20
export function sliderSortTimeS(v) { sortS = clamp(v, 1, 600) }

var MAXV = 1024
var val = array(MAXV)   // the values ARE hues: base + rank / M * SPAN
var prof = array(256)   // bar brightness from its top down (panel)
var SPAN = 0.8
var M = 8, panel = 0, gw = 0, gh = 0, seenN = -1, seenW = -1
var cur = 1, base = 0, phase = 3, t = 0, carry = 0, est = 1
var hiA = -1, hiB = -1
// step-machine registers, shared by the algorithms
var i = 0, j = 0, end = 0, gap = 1, flag = 0, root = 0, stage = 0

function layout() {
  seenN = pixelCount
  seenW = gridWidth()
  gw = gridWidth()
  gh = gridHeight()
  panel = has2DMap() && gw >= 8
  M = panel ? min(gw, 128) : clamp(pixelCount, 2, MAXV)
  for (var k = 0; k < 256; k++) prof[k] = 1 - 0.55 * min(k / max(gh, 1), 1)
  for (k = 0; k < M; k++) val[k] = base + k / M * SPAN
  phase = 3
  t = 0
  i = M - 1
}

function swap(a, b) { var x = val[a]; val[a] = val[b]; val[b] = x }

function start() {
  if (algo > 0) cur = algo
  var lg = log2(M)
  i = 0; j = 0; flag = 0; stage = 0
  // est = expected steps per value (M * est would overflow 16.16 on long strips)
  if (cur == 1) { end = M - 1; est = M * 0.47 }
  else if (cur == 2) { i = 1; j = 1; est = M * 0.26 + 1 }
  else if (cur == 3) { gap = max(1, floor(M / 2.2)); i = gap; j = gap; est = lg * 1.2 }
  else { i = floor(M / 2) - 1; root = i; end = M; est = lg * 0.95 + 0.4 }
}

// One comparison (and maybe a swap). Returns 1 when the sort is finished.
function step() {
  if (cur == 1) {                                  // bubble: the largest sinks to the end
    hiA = j; hiB = j + 1
    if (val[j] > val[j + 1]) { swap(j, j + 1); flag = 1 }
    j++
    if (j >= end) {
      if (!flag) return 1
      end--; j = 0; flag = 0
      if (end <= 0) return 1
    }
    return 0
  }
  if (cur == 2 || cur == 3) {                      // insertion, and shell = gapped insertion
    var g = cur == 2 ? 1 : gap
    hiA = j - g; hiB = j
    if (j >= g && val[j - g] > val[j]) { swap(j - g, j); j -= g; return 0 }
    i++; j = i
    if (i >= M) {
      if (g == 1) return 1
      gap = max(1, floor(gap / 2.2)); i = gap; j = gap
    }
    return 0
  }
  // heap: build a max-heap bottom up, then swap the root to the end and sift it down
  var c = 2 * root + 1
  if (c < end) {
    if (c + 1 < end && val[c] < val[c + 1]) c++
    hiA = root; hiB = c
    if (val[root] < val[c]) { swap(root, c); root = c; return 0 }
  }
  if (stage == 0) {
    i--
    if (i >= 0) { root = i; return 0 }
    stage = 1; end = M
  }
  end--
  if (end <= 0) return 1
  swap(0, end); hiA = 0; hiB = end; root = 0
  return 0
}

export function beforeRender(delta) {
  if (pixelCount != seenN || gridWidth() != seenW) layout()
  var dt = min(delta, 100) / 1000
  t += dt
  hiA = -1; hiB = -1
  if (phase == 0) {                                // sorting
    carry += M * min(est / sortS, 24000 / M) * dt
    var n = min(floor(carry), 400)
    carry -= floor(carry)
    for (var k = 0; k < n; k++) if (step()) { phase = 1; t = 0; break }
  } else if (phase == 1) {                         // the check sweep
    if (t > 1.2) { phase = 2; t = 0 }
  } else if (phase == 2) {                         // hold, sparkling
    if (t > 1.6) { phase = 3; t = 0; i = M - 1; recolor(mod(base + 0.27, 1)) }
  } else {                                         // shatter: an animated Fisher-Yates shuffle
    carry += M / 1.2 * dt
    while (carry >= 1 && i > 0) {
      carry -= 1
      var r = floor(random(i + 1))
      swap(i, r); hiA = i; hiB = r
      i--
    }
    if (i <= 0) {
      carry = 0
      phase = 0
      if (algo == 0) cur = cur % 4 + 1
      start()
    }
  }
}

// a new palette keeps every value's rank
function recolor(nb) {
  for (var k = 0; k < M; k++) val[k] = nb + round((val[k] - base) / SPAN * M) / M * SPAN
  base = nb
}

function rankOf(k) { return (val[k] - base) / SPAN }

// brightness lift of value k during the sweep and the hold
function shine(k) {
  if (phase == 1) {
    var head = t / 1.2 * M
    return k < head ? max(0, 1 - (head - k) / (M * 0.25)) : 0
  }
  if (phase == 2) return random(1) < 0.04 ? 1 : 0
  return 0
}

export function render(index) { hsv(0, 0, 0) }   // mapless strips stay strips

export function renderBulk() {
  var k
  if (!panel) {
    if (M >= pixelCount) fillHSV(val, 1, 1)
    else for (k = 0; k < pixelCount; k++) { hsv(val[floor(k * M / pixelCount)], 1, 1); setPixel(k) }
    if (phase >= 1 && phase <= 2) {
      for (k = 0; k < M; k++) {
        var s = shine(k)
        if (s > 0) { hsv(val[k], 1 - s * 0.85, 1); setPixel(k) }
      }
    }
    rgb(1, 1, 1)
    if (hiA >= 0) setPixel(hiA)
    if (hiB >= 0) setPixel(hiB)
    return
  }
  clear()
  var bw = max(1, floor(gw / M)), x0 = floor((gw - bw * M) / 2)
  for (k = 0; k < M; k++) {
    var hgt = max(1, round((rankOf(k) + 1 / M) / (1 + 1 / M) * gh))
    var sat = 1
    if (k == hiA || k == hiB) sat = 0
    else if (phase == 1 || phase == 2) sat = 1 - 0.8 * shine(k)
    for (var c = 0; c < bw; c++) blit(val[k], sat, prof, 1, hgt, x0 + k * bw + c, gh - hgt, 0)
  }
}

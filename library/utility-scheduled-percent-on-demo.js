// name: Utility: Scheduled Percent-On Demo
// Clean-room reimplementation from a prose functional description of the
// community pattern "Utility: Scheduled Percent-On Demo"; original source
// never consulted.

// Tutorial/utility pattern: computes how far the wall clock is through a
// daily on-window and shows that fraction as a solid hue on every pixel.
// Inside the window the display sweeps once through the rainbow (red at
// the window start, back toward red at the end); outside the window the
// fraction is zero, which renders solid red — it demonstrates the
// schedule variable, it does not black out.

var startHour = 0
var endHour = 24

// Exported for inspection in the vars watcher, tutorial-style.
export var currentHour = 0
export var windowHours = 24
export var onFraction = 0

//# min=0 max=1 step=0.04 default=0
export function sliderBeginTime(v) {
  startHour = min(23, floor(v * 24))
}

//# min=0 max=1 step=0.04 default=1
export function sliderEndTime(v) {
  endHour = min(24, floor(v * 24 + 0.5))
}

export function beforeRender(delta) {
  // Schedule math is pixel-independent, so it lives here rather than in
  // render (the original recomputed it per pixel).
  currentHour = clockHour() + clockMinute() / 60 + clockSecond() / 3600

  windowHours = endHour - startHour
  if (windowHours <= 0) windowHours += 24   // window length wraps midnight

  // Faithful port of a documented flaw: this inside-the-window test is a
  // plain start <= h < end comparison, so an overnight window (end before
  // start) never tests true even though the length math above handles it.
  if (currentHour >= startHour && currentHour < endHour) {
    onFraction = (currentHour - startHour) / windowHours
  } else {
    onFraction = 0
  }
}

export function render(index) {
  hsv(onFraction, 1, 1)
}

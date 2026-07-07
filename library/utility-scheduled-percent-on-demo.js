// name: Utility: Scheduled Percent-On Demo
// Clean-room reimplementation from a prose functional description of the
// community pattern "Utility: Scheduled Percent-On Demo"; original source
// never consulted.
//
// A tutorial/utility pattern: two sliders pick a start and end hour; the
// fraction elapsed through that daily window drives the hue of a solid
// color on every pixel — a slow once-per-window rainbow sweep. Outside the
// window the fraction is 0, which renders solid red (it demonstrates the
// schedule variable; it does not black out).

var beginHour = 8
var endHour = 20

// Intermediate values exported for inspection, tutorial-style.
export var currentHour = 0
export var currentMinute = 0
export var currentSecond = 0
export var windowLength = 12
export var onFraction = 0

//# min=0 max=1 step=0.0417 default=0.333
export function sliderBeginTime(v) {
  beginHour = floor(v * 23.99)   // whole hours across the 24-hour day
}

//# min=0 max=1 step=0.0417 default=0.833
export function sliderEndTime(v) {
  endHour = floor(v * 23.99)
}

export function beforeRender(delta) {
  // Clock reads and schedule math are pixel-independent, so they live
  // here rather than in render() (a documented fix over the original).
  currentHour = clockHour()
  currentMinute = clockMinute()
  currentSecond = clockSecond()
  var now = currentHour + currentMinute / 60 + currentSecond / 3600

  windowLength = endHour - beginHour
  if (windowLength <= 0) windowLength += 24  // window length wraps midnight

  // Faithfully ported flaw: the inside-the-window test is a plain
  // start <= hour < end comparison, so an overnight window (end before
  // start) never tests true even though windowLength handles the wrap.
  onFraction = 0
  if (currentHour >= beginHour && currentHour < endHour) {
    onFraction = (now - beginHour) / windowLength
  }
}

export function render(index) {
  hsv(onFraction, 1, 1)
}

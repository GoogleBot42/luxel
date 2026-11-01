// name: Breakout 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Breakout 2D"; original source never consulted.
//
// Self-playing Breakout on a 16x16 virtual canvas: rainbow brick wall up
// top, white ball, white paddle on the bottom row that auto-tracks the
// ball (imperfectly) in demo mode. Fixed 100 ms game ticks decouple game
// speed from render rate.

var W = 16
var H = 16
var brickRows = 5                 // about a third of the height
var bricks = array(W * H)         // row-major 16x16 grid, top rows used
var brickCount = 0

var ballX = 0.5
var ballY = 0.5
var velX = 0
var velY = 0.08
var speed = 0.08                  // unit distance per tick (~12 ticks to cross)

var paddleC = 0.5
var prevPaddleC = 0.5
var paddleMove = 0
var paddleHW = 0.15
var manualC = 0.5
var demoOn = 1

var tickAcc = 0
var shimmer = 0

function resetWall() {
  brickCount = brickRows * W
  var r
  var c
  for (r = 0; r < brickRows; r++) {
    for (c = 0; c < W; c++) {
      bricks[r * W + c] = 1
    }
  }
}

function resetBall() {
  // just below the wall, random x, heading down within a modest cone
  ballX = 0.05 + random(0.9)
  ballY = (brickRows + 1) / H
  var a = (random(1) - 0.5) * 0.9
  velX = speed * sin(a)
  velY = speed * cos(a)
}

resetWall()
resetBall()

function tick() {
  paddleMove = paddleC - prevPaddleC     // recent paddle speed = "spin"
  prevPaddleC = paddleC
  if (demoOn > 0) {
    // track the ball with a little human-like error (up to ~1 pixel)
    paddleC = clamp(ballX - random(1 / W), 0, 1)
  } else {
    paddleC = manualC
  }

  ballX += velX
  ballY += velY

  var row = floor(ballY * H)
  var col = clamp(floor(ballX * W), 0, W - 1)

  if (ballY >= 1) {                      // fell off the bottom
    resetBall()
  } else if (ballY < 0) {                // ceiling
    ballX -= velX
    ballY -= velY
    velY = -velY
  } else if (ballX < 0 || ballX >= 1) {  // side walls
    ballX -= velX
    ballY -= velY
    velX = -velX
  } else if (row >= H - 1) {             // paddle row
    if (abs(ballX - paddleC) <= paddleHW) {
      ballX -= velX
      ballY -= velY
      velY = -velY
      velX = clamp(velX - paddleMove * 0.1, -0.12, 0.12)  // backspin/english
    }
  } else if (row < brickRows) {          // brick region
    var cell = row * W + col
    if (bricks[cell]) {
      bricks[cell] = 0
      brickCount -= 1
      ballX -= velX
      ballY -= velY
      velY = -velY
      if (brickCount <= 0) {             // cleared! start over
        resetWall()
        resetBall()
      }
    }
  }
}

export function beforeRender(delta) {
  shimmer = triangle(time(0.1))          // slow hue shimmer, ~6.5 s period
  tickAcc += delta
  if (tickAcc > 400) tickAcc = 400       // don't spiral after a long stall
  while (tickAcc >= 100) {               // fixed ~0.1 s game tick
    tickAcc -= 100
    tick()
  }
}

export function render2D(index, x, y) {
  var px = floor(x * 15.99)
  var py = floor(y * 15.99)

  if (py == H - 1) {                     // paddle on the bottom row
    var pd = abs(x - paddleC)
    if (pd <= paddleHW) {
      var pv = 1 - 0.7 * pd / paddleHW
      rgb(pv, pv, pv)
    } else {
      rgb(0, 0, 0)
    }
    return
  }

  if (py < brickRows && bricks[py * W + px]) {
    // rainbow banding by row, drifting together with the shimmer
    hsv(py / brickRows * 0.85 + shimmer / 3, 1, 1)
    return
  }

  // anti-aliased white ball everywhere else (including cleared cells)
  var d = hypot((x - ballX) * W, (y - ballY) * H)
  if (d < 1) {
    var v = 1 - d
    rgb(v, v, v)
  } else {
    rgb(0, 0, 0)
  }
}

//# min=0 max=1 step=0.01 default=0.5
export function sliderPaddlePosition(v) {
  manualC = v
}

//# min=0 max=1 step=0.01 default=0.3
export function sliderPaddleWidth(v) {
  paddleHW = 0.1 + v * 0.18       // half-width: ~fifth of display up to a bit over half
}

//# min=0 max=1 step=1 default=1
export function sliderDemoMode(v) {
  demoOn = v                      // anything above zero = auto-play
}

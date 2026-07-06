// name: Breakout 2D
// Clean-room reimplementation from a prose functional description of the
// community pattern "Breakout 2D"; original source never consulted.

// Self-playing Breakout on a 16x16 virtual grid: rainbow brick wall in the
// top third, an anti-aliased white ball, and a white paddle on the bottom
// row that tracks the ball with deliberate error in demo mode. Game logic
// runs on a fixed ~100 ms tick, decoupled from render rate.

var W = 16
var H = 16
var BRICK_ROWS = 5 // ~top third of the display
var bricks = array(W * BRICK_ROWS)
var bricksLeft = 0

var ballX = 0.5, ballY = 0.5
var vx = 0, vy = 0
var SPEED = 0.085 // unit distance per tick: crosses the display in ~12 ticks

var paddleX = 0.5
var prevPaddleX = 0.5
var paddleHalf = 0.15
var demo = 1

var TICK = 100 // ms
var tickAccum = 0
var shimmer = 0
var initialized = 0

function resetBall() {
  ballX = random(1)
  ballY = (BRICK_ROWS + 1) / H // just below the brick wall
  // heading downward within a modest cone around straight down
  var ang = random(0.7) - 0.35
  vx = sin(ang) * SPEED
  vy = cos(ang) * SPEED
}

function resetWall() {
  for (var i = 0; i < W * BRICK_ROWS; i++) bricks[i] = 1
  bricksLeft = W * BRICK_ROWS
  resetBall()
}

function gameTick() {
  // paddle movement since last tick = "spin" / english
  var paddleVel = paddleX - prevPaddleX
  prevPaddleX = paddleX

  if (demo > 0) {
    // imperfect auto-play: track the ball minus up to ~one pixel of error
    paddleX = clamp(ballX - random(1 / W), 0, 1)
  }

  ballX += vx
  ballY += vy

  if (ballY >= 1) {
    // fell off the bottom
    resetBall()
  } else if (ballY < 0) {
    // ceiling
    ballX -= vx; ballY -= vy
    vy = -vy
  } else if (ballX < 0 || ballX >= 1) {
    // side walls
    ballX -= vx; ballY -= vy
    vx = -vx
  } else if (floor(ballY * H) == H - 1) {
    // paddle row
    if (abs(ballX - paddleX) < paddleHalf) {
      ballX -= vx; ballY -= vy
      vy = -vy
      vx -= paddleVel * 0.1 // backspin keeps the demo from looping
    }
  } else if (floor(ballY * H) < BRICK_ROWS) {
    // brick region
    var bi = floor(ballY * H) * W + floor(ballX * W)
    if (bricks[bi]) {
      bricks[bi] = 0
      bricksLeft -= 1
      ballX -= vx; ballY -= vy
      vy = -vy
      if (bricksLeft <= 0) resetWall()
    }
  }
}

export function beforeRender(delta) {
  if (!initialized) {
    initialized = 1
    resetWall()
  }

  // brick shimmer: slow triangle wave, several-second period
  shimmer = triangle(time(0.1))

  // fixed-timestep game logic
  tickAccum += delta
  if (tickAccum >= TICK) {
    tickAccum -= TICK
    if (tickAccum > TICK) tickAccum = 0 // don't spiral after a stall
    gameTick()
  }
}

export function sliderPaddlePosition(v) {
  //# min=0 max=1 step=0.01 default=0.5
  if (demo <= 0) paddleX = v
}

export function sliderPaddleWidth(v) {
  //# min=0 max=1 step=0.01 default=0.3
  paddleHalf = 0.1 + v * 0.18 // full width ~a fifth up to a bit over half
}

export function sliderDemoMode(v) {
  //# min=0 max=1 step=1 default=1
  demo = v > 0
}

export function render2D(index, x, y) {
  var row = floor(y * 15.99)
  var col = floor(x * 15.99)

  // bottom row: the paddle
  if (row == H - 1) {
    var pd = abs(x - paddleX)
    if (pd < paddleHalf) {
      var pv = 1 - pd / paddleHalf
      rgb(pv, pv, pv)
    } else {
      rgb(0, 0, 0)
    }
    return
  }

  // brick wall: rainbow banded by row, drifting with the shimmer
  if (row < BRICK_ROWS) {
    if (bricks[row * W + col]) {
      hsv(row / BRICK_ROWS * 0.7 + shimmer / 3, 1, 1)
      return
    }
    // cleared brick cell: fall through to ball drawing
  }

  // the ball, anti-aliased over about a pixel
  var d = hypot((x - ballX) * W, (y - ballY) * H)
  if (d < 1.2) {
    var bv = 1 - d / 1.2
    rgb(bv, bv, bv)
  } else {
    rgb(0, 0, 0)
  }
}

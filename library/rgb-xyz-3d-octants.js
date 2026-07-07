// name: RGB-XYZ 3D Octants
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGB-XYZ 3D Octants"; original source never consulted.

// Static diagnostic for verifying 3D pixel-map axis orientation. The unit
// cube is split at the midpoint of each axis into eight octants; each color
// channel tracks one axis (red = x high, green = y high, blue = z high).
// The octants therefore show black, blue, green, cyan, red, magenta,
// yellow, and white — the all-high corner is white, the all-low corner
// dark. Note: only meaningful if the device's RGB channel ordering is
// configured correctly, otherwise the octant colors mislead.

// Edit these to dim the diagnostic (not exposed as controls).
const ON = 1
const OFF = 0

export function render3D(index, x, y, z) {
  rgb(x >= 0.5 ? ON : OFF,
      y >= 0.5 ? ON : OFF,
      z >= 0.5 ? ON : OFF)
}

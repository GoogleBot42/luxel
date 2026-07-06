// name: RGB-XYZ 3D Octants
// Clean-room reimplementation from a prose functional description of the
// community pattern "RGB-XYZ 3D Octants"; original source never consulted.

// Static diagnostic for verifying a 3D pixel map's axis orientation. The
// unit cube is split at 0.5 on each axis into eight octants; each color
// channel is tied to one axis (red = x high, green = y high, blue = z
// high). Channels add, so the octants show black, blue, green, cyan, red,
// magenta, yellow, and white — the all-high corner is white, the all-low
// corner is dark.
//
// Note: this check is only meaningful if the device's RGB channel ordering
// is configured correctly; otherwise the octant colors mislead.

// Edit these two constants in source to dim the pattern (not UI controls).
const ON = 1
const OFF = 0

export function render3D(index, x, y, z) {
  rgb(
    x > 0.5 ? ON : OFF,
    y > 0.5 ? ON : OFF,
    z > 0.5 ? ON : OFF
  )
}

# RGB-XYZ 3D Octants
kind: 3D
sensors: no

This is a trivial, completely static diagnostic pattern for verifying a 3D
pixel map's axis orientation. No animation, no state, no controls, no
randomness.

Per pixel: treat the mapped coordinate space as a unit cube split at the
midpoint of each axis into eight octants. Each color channel is tied to one
axis: red channel is on at full when x is in the upper half (else off), green
likewise for y, blue for z. Channels combine additively, so the octants come
out: black, blue, green, teal/cyan, red, purple/magenta, yellow, and white —
letting the user visually confirm each axis points the expected way (the
all-high corner is white, the all-low corner is dark).

The "on" and "off" channel levels are two constants (full and zero) that a
user can edit in source to dim it; not exposed as controls.

Caveat worth carrying into documentation: the check is only meaningful if the
device's RGB channel ordering is configured correctly, otherwise the octant
colors mislead.

# Patternflow idea specs — describer scores (specs in $W/docs/pattern-specs/patternflow/)
# Licence note: presets are CC BY-SA 4.0 art by Seunghun Lee; only prose idea specs exist on our side.

| spec | score | verdict |
|---|---|---|
| hatched-warp | 3 | print-poster plasma, each band a texture (solid/black/dots/hatch) |
| hud-ticker-rows | 4 | lanes of scrolling HUD widgets; ideal as a scene with sprites/text |
| lotus-bloom | 3 | polar-rose mandala with outline/glow/core/aura layers; needs AA and panel scaling |
| sliding-checker | 3 | checkerboard whose rows slide out of phase; wants eased motion |
| glyph-mosaic | 3 | plasma as a grid of glyphs (ASCII-art look) |
| crossing-ribbons | 2 | two opposite gratings; redo as a rotating two-layer scene |
| nodal-lattice | 4 | glowing lines where four waves cancel (caustics-like); cheap, hypnotic |
| spinning-crosses | 3 | counter-rotating crosses reading as meshing gears |
| sliding-strata | 3 | bands of noise shearing sideways like rock strata; glitch look |
| contour-blobs | 4 | moving islands as a topographic contour map; flagship with layered noise |
| glow-tendrils | 4 | bioluminescent filaments; curl warp + trails → one of the best 2D pieces |
| dither-bit-rain | 3 | 1-bit dithered falling streams with a pink accent |
| kaleido-bit-rain | 3 | same, mirrored four ways + warped; ship as a mode of the above |
| normal-map-plexus | 3 | network of moving nodes, links coloured by direction |
| pulse-dot-grid | 3 | halftone dots pulsed by a travelling wave; dot-matrix spectrum base |
| thermal-posterize | 2 | posterized thermal plasma; better as an output palette |
| wave-cell-grid | 2 | square cells gated by a wave + scan line; mode only (merge with pulse-dot-grid) |
| oil-slick-glitch | 4 | iridescent oil-on-water contour bands + slice glitch; calm and glitch modes |
| retro-digital-tapestry | 3 | bitwise cell patterns with colour wash; merge with xorcery-2d-3d |
| vector-field-flow | 2 | generic sine-warp plasma |
| chromatic-vortex | 3 | spiral rendered per R/G/B so it fringes; arm-count + twist controls |
| lissajous-weave | 3 | woven-thread Lissajous; lines + glow + eased morph → 4; merge with lissajous tracer |
| glitch-rings | 3 | bold banded rings with VHS row tearing |
| tile-waves | 2 | mosaic ripple with stepped timing (dup of origin) |
| breakout-arcade | 4 | self-playing Breakout with power-ups/combos/AI; fold into breakout-2d |
| midsummer-sea | 4 | synthwave sunset seascape; strongest scene candidate |
| firefly-hollow | 4 | dusk meadow with blinking fireflies; scene with fireflies layer |
| poincare-sphere | 3 | scrolling grid through a fisheye lens folding into a mirrored rim |
| tri-march | 3 | marching lattice of shimmering triangles; lattice-shapes base with modes |
| warped-wave | 2 | warped ripple in four flat bands; mode only |
| mag-vortex | 4 | particle spiral with fading trails; ideal renderBulk (drawLine + fade) |
| a-big-hit | 3 | neon-tube plasma lit where a warped field crosses zero; → 4 with simplex |
| origin | 3 | gallery of tiles where aliasing makes a different moiré per tile |
| wave-saw | 2 | three-colour stripes with noise warp; palette banded-texture only |

Near-dups: dither-bit-rain+kaleido-bit-rain; pulse-dot-grid+wave-cell-grid; glitch-rings+warped-wave+tile-waves+origin ("rings" family with tile/tear/warp/band modes); vector-field-flow+a-big-hit (keep neon ridge look); hatched-warp/nodal-lattice/contour-blobs/thermal-posterize share a sum-of-sines field.
Overlaps with library: breakout-arcade↔breakout-2d; lissajous-weave↔lissajous-curve-tracer; firefly-hollow↔fireflies/colourful-fireflies; retro-digital-tapestry↔xorcery-2d-3d; midsummer-sea↔ocean/sunset; vector-field-flow↔curl-flow-2d/flow-field-2d.
Portrait note: breakout-arcade, midsummer-sea, firefly-hollow were laid out tall; lay out from the grid's aspect.

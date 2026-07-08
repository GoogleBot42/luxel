# Hardware soak + benchmark — 2026-07-08

*Device 192.168.0.205, firmware v0.1.25, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 195 patterns: **176 clean**, 19 with errors, 29 under 30 fps.
- fps at 300 px: median **69**, p10 21, p90 123.
- lowest heap_free seen while soaking: 30696 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 124 |
| 150 | 124 |
| 300 | 124 |
| 600 | 87 |
| 1024 | 51 |
| 2048 | 26 |

## Errors

| pattern | kind | problem |
|---|---|---|
| amoeba | strip | pattern too large for this device (out of memory) |
| aurorashivers | strip | line 0:0: indexing a non-array value |
| Bouncy Boxes | grid | pattern too large for this device (out of memory) |
| Breakout 2D | grid | line 0:0: array index out of bounds |
| bustle | strip | pattern too large for this device (out of memory) |
| Chasing Rainbows & HSLuv | strip | rejected: invalid bytecode: truncated |
| Continuous Cellular Automata | grid | line 0:0: indexing a non-array value |
| Crosstown Traffic 2D | grid | line 0:0: array index out of bounds |
| DAFTPUNK | grid | rejected: invalid bytecode: truncated |
| DBZBattleFinal | grid | pattern too large for this device (out of memory) |
| Emoji Animation #2 | grid | rejected: invalid bytecode: truncated |
| Frogger 2D | grid | line 0:0: array index out of bounds |
| heatshivers | strip | line 0:0: indexing a non-array value |
| neutronorbit | strip | pattern too large for this device (out of memory) |
| Rainbow Smiley | grid | rejected: invalid bytecode: truncated |
| slowflies | strip | line 0:0: indexing a non-array value |
| StarGen polar 2D | grid | pattern too large for this device (out of memory) |
| tixy | grid | pattern too large for this device (out of memory) |
| Utility: Palettes | strip | rejected: invalid bytecode: truncated |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Animated Asterisks 2D | grid | 7 |
| Mandelbrot 2D | grid | 7 |
| Complements 3D | grid | 9 |
| Iran - Solidarity | cloud | 10 |
| portal | strip | 10 |
| scrolls | strip | 10 |
| wanderedges | strip | 11 |
| Voronoi Mix 2D | grid | 13 |
| Ice Floes 2D | grid | 14 |
| All Lasers Fire | grid | 16 |
| Custom Sequences | strip | 16 |
| Metaballs of Fire 2D | grid | 16 |
| coolaura | strip | 18 |
| fireblobs | strip | 20 |
| novas | strip | 20 |
| Pew-Pew-Pew! | strip | 20 |
| 2D Bouncing Additive Primaries | grid | 21 |
| 2D sinc(theta)/theta | grid | 21 |
| ChristmasPewPew | strip | 21 |
| Twinkling Classic Xmas Strands | strip | 21 |
| 1D Aurora Borealis | strip | 22 |
| 3D Rotation / Spotlights | cloud | 22 |
| Wichmann–Hill PRNG | strip | 22 |
| Bouncer3D | grid | 23 |
| Crossfading | grid | 23 |
| spotlights / rotation 3D | grid | 24 |
| fractal flower | grid | 26 |
| The Grinch | strip | 26 |
| 2D Spiral Twirls | grid | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 59 |
| 1 White Fade | strip | 124 |
| 1-USFLAG_BLINK | strip | 119 |
| 1D Aurora Borealis | strip | 22 |
| 2 Colors | strip | 102 |
| 2 Purple Fade | strip | 124 |
| 2D Bouncing Additive Primaries | grid | 21 |
| 2D canvas example | grid | 79 |
| 2D sinc(theta)/theta | grid | 21 |
| 2D Spiral Twirls | grid | 29 |
| 2D Wandering Fireball | grid | 53 |
| 3 color rotation | strip | 74 |
| 3 Violet Fade | strip | 124 |
| 3D Rotation / Spotlights | cloud | 22 |
| 4 Blue Fade | strip | 123 |
| 4th | strip | 42 |
| 5 Teal Fade | strip | 124 |
| 6 Green Fade | strip | 124 |
| 7 Yelllow Fade | strip | 124 |
| 8 Red Fade | strip | 124 |
| 9 Pink Fade | strip | 124 |
| All Lasers Fire | grid | 16 |
| amoeba | strip | 20 |
| angle and radius from coordinates | grid | 32 |
| Angry Xmass 3D | cloud | 81 |
| Animated Asterisks 2D | grid | 7 |
| aurorashivers | strip | 41 |
| Austin FC | strip | 79 |
| Automap | strip | 124 |
| Autumn Colors | strip | 95 |
| b_lightning_flashes | strip | 120 |
| Bessel Chaos | strip | 55 |
| blink fade | strip | 73 |
| Blinky Eyes 2D | grid | 40 |
| block reflections | strip | 51 |
| Bouncer3D | grid | 23 |
| bouncing balls - hsv | strip | 91 |
| bouncing balls - rgb | strip | 75 |
| Bouncing RGB Balls - 2D | grid | 38 |
| Bouncy Boxes | grid | 20 |
| Breakout 2D | grid | 52 |
| bustle | strip | 20 |
| Cellular Automata 1D | strip | 102 |
| Chasing Rainbows & HSLuv | strip | — |
| chill confetti | strip | 119 |
| Christmas Candy Cane | strip | 61 |
| Christmas Lights | strip | 96 |
| Christmas RG Fade | strip | 74 |
| Christmas string lights | strip | 60 |
| ChristmasLights | strip | 97 |
| ChristmasPewPew | strip | 21 |
| ChristmasStretch | strip | 74 |
| color bands | strip | 47 |
| color bands (buffered) | strip | 37 |
| Color Blend | strip | 51 |
| color fade pulse | strip | 61 |
| Color Pick Fade | strip | 90 |
| color twinkle bounce | strip | 54 |
| color twinkles | strip | 48 |
| colourful fireflies | strip | 75 |
| Complements 3D | grid | 9 |
| Continuous Cellular Automata | grid | 124 |
| coolaura | strip | 18 |
| Crossfading | grid | 23 |
| Crosstown Traffic 2D | grid | 123 |
| Custom Sequences | strip | 16 |
| Cyclic Cellular Automata 2D | grid | 69 |
| Cylon | strip | 71 |
| DAFTPUNK | grid | — |
| DBZBattleFinal | grid | 20 |
| dimbypixel | strip | 124 |
| Doom Fire | grid | 68 |
| Doom Fire (v2.0) 2D | grid | 64 |
| Easing Library v1.0 | grid | 77 |
| Easing Library v1.01 | grid | 71 |
| Edgeburst | strip | 81 |
| Emoji Animation #2 | grid | — |
| Example: color hues | strip | 118 |
| Example: modes and waveforms | strip | 111 |
| Example: Smooth Speed Slider | strip | 122 |
| Example: time and animation | strip | 98 |
| fast pulse | strip | 83 |
| fast pulse 3d | grid | 83 |
| fire - blue | strip | 34 |
| fire - red | strip | 34 |
| fireblobs | strip | 20 |
| FireFlies | strip | 79 |
| firework dust | strip | 124 |
| firework nova | cloud | 41 |
| firework rocket sparks | strip | 58 |
| fractal flower | grid | 26 |
| Frogger 2D | grid | 124 |
| glitch bands | strip | 36 |
| Golden Tix | strip | 77 |
| Gradient blue  purple pink | strip | 64 |
| green ripple reflections | strip | 47 |
| Halloween color twinkles | strip | 45 |
| heart | grid | 39 |
| heatshivers | strip | 41 |
| Holiday_Diagonal_Stripes | grid | 83 |
| Ice Floes 2D | grid | 14 |
| Icicleblaze | grid | 40 |
| index walk | strip | 123 |
| Infinity Flower 2D | grid | 31 |
| Iran - Solidarity | cloud | 10 |
| KITT | strip | 70 |
| KITT (w/ color picker) | strip | 68 |
| lightning ZAP! | strip | 76 |
| Mandelbrot 2D | grid | 7 |
| Map - Concentric | grid | 123 |
| mapped vertical line 2D | grid | 121 |
| marching rainbow | strip | 57 |
| marching rainbow (buffered) | strip | 50 |
| Matrix 2 tone pulse | strip | 37 |
| matrix 2D honeycomb | strip | 42 |
| matrix 2D pulse edit | strip | 52 |
| Matrix Green Waterfall 2D | grid | 96 |
| matrix rain | grid | 62 |
| Metaballs of Fire 2D | grid | 16 |
| Meteor Shower | strip | 111 |
| MidpointDisplacement1D | strip | 119 |
| millipede | strip | 72 |
| millipede 1d/2d controls | grid | 81 |
| neutronorbit | strip | 20 |
| Newfire | strip | 69 |
| novas | strip | 20 |
| Nyan Lights | strip | 53 |
| opposites | strip | 48 |
| Performance test framework | strip | 57 |
| Pew-Pew-Pew! | strip | 20 |
| policeLights | strip | 124 |
| portal | strip | 10 |
| Pride Progress | strip | 64 |
| quiet blinkfade | strip | 86 |
| rainbow | strip | 123 |
| Rainbow Comet | strip | 60 |
| Rainbow Flag | strip | 77 |
| rainbow fonts | strip | 94 |
| rainbow fonts 2 | strip | 83 |
| rainbow melt | strip | 69 |
| rainbow pinwheel | strip | 120 |
| Rainbow rocket sparks | strip | 74 |
| Rainbow Smiley | grid | — |
| Rainbow v2 | strip | 122 |
| Raindrops 2D | grid | 57 |
| Red-Green XY 2D Sweep | grid | 82 |
| regenbogendrogen | strip | 93 |
| RGB Test Pattern | strip | 120 |
| RGB-XYZ 3D Octants | cloud | 110 |
| RGBW Mapping Tester | strip | 116 |
| RGBW Mapping Tester - HSV Version | strip | 113 |
| Scanner | strip | 59 |
| scrolls | strip | 10 |
| Shimmer Crossfade 2D | grid | 41 |
| Sierpinski Rainbow 2D | grid | 68 |
| Single Color Picker - wide or spot | strip | 104 |
| sinpulse 3D | grid | 52 |
| sinus | grid | 58 |
| slow color shift | strip | 53 |
| slowflies | strip | 34 |
| snake | strip | 85 |
| Solid Rainbow | strip | 124 |
| sparkfire | strip | 35 |
| sparks | strip | 81 |
| sparks center | strip | 85 |
| spin cycle | strip | 60 |
| Spinwheel 2D | grid | 41 |
| spotlights / rotation 3D | grid | 24 |
| Spring Colors | strip | 96 |
| Stacker | strip | 92 |
| StarGen polar 2D | grid | 20 |
| Static Christmas Lights - 4 Colors | strip | 108 |
| static random colors | strip | 43 |
| Swirlpool 2D | grid | 67 |
| Synchronized Random Numbers | strip | 33 |
| The Grinch | strip | 26 |
| Three Red Pixels (array) | strip | 82 |
| Three Red Pixels (mathy) | strip | 112 |
| Thunderstorm | strip | 66 |
| Time Flies 2D | grid | 124 |
| tixy | grid | 20 |
| Twinkle | strip | 75 |
| Twinkling Classic Xmas Strands | strip | 21 |
| TwoColorHSVMix | strip | 48 |
| Unstable Orbits | grid | 84 |
| Utility: Palettes | strip | — |
| Utility: Perceptual hue | strip | 106 |
| UtilityColorTemp | strip | 124 |
| Voronoi Mix 2D | grid | 13 |
| wanderedges | strip | 11 |
| wanderers | strip | 69 |
| White Rainbows | strip | 76 |
| Wichmann–Hill PRNG | strip | 22 |
| XmasFlies | strip | 30 |
| xorcery 2D/3D | grid | 34 |

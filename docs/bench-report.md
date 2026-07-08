# Hardware soak + benchmark — 2026-07-08

*Device 192.168.0.205, firmware v0.1.26, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 195 patterns: **189 clean**, 6 with errors, 40 under 30 fps.
- fps at 300 px: median **64**, p10 18, p90 122.
- lowest heap_free seen while soaking: 60808 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 123 |
| 150 | 123 |
| 300 | 123 |
| 600 | 84 |
| 1024 | 49 |
| 2048 | 25 |

## Errors

| pattern | kind | problem |
|---|---|---|
| Breakout 2D | grid | line 0:0: array index out of bounds |
| Crosstown Traffic 2D | grid | line 0:0: array index out of bounds |
| Emoji Animation #2 | grid | pattern too large for this device (out of memory) |
| Frogger 2D | grid | line 0:0: array index out of bounds |
| Rainbow Comet | strip | line 0:0: array index out of bounds |
| Rainbow Smiley | grid | line 0:0: array index out of bounds |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Animated Asterisks 2D | grid | 7 |
| Mandelbrot 2D | grid | 7 |
| Complements 3D | grid | 9 |
| Iran - Solidarity | cloud | 9 |
| portal | strip | 9 |
| wanderedges | strip | 10 |
| scrolls | strip | 11 |
| amoeba | strip | 12 |
| Ice Floes 2D | grid | 12 |
| Voronoi Mix 2D | grid | 12 |
| neutronorbit | strip | 13 |
| Utility: Palettes | strip | 13 |
| DBZBattleFinal | grid | 14 |
| All Lasers Fire | grid | 15 |
| coolaura | strip | 15 |
| Custom Sequences | strip | 15 |
| Metaballs of Fire 2D | grid | 15 |
| bustle | strip | 18 |
| novas | strip | 18 |
| 2D Bouncing Additive Primaries | grid | 19 |
| 2D sinc(theta)/theta | grid | 19 |
| Pew-Pew-Pew! | strip | 19 |
| StarGen polar 2D | grid | 19 |
| 1D Aurora Borealis | strip | 20 |
| slowflies | strip | 20 |
| Twinkling Classic Xmas Strands | strip | 20 |
| aurorashivers | strip | 21 |
| ChristmasPewPew | strip | 21 |
| fireblobs | strip | 21 |
| heatshivers | strip | 21 |
| Wichmann–Hill PRNG | strip | 21 |
| 3D Rotation / Spotlights | cloud | 22 |
| Bouncer3D | grid | 23 |
| Crossfading | grid | 23 |
| spotlights / rotation 3D | grid | 23 |
| fractal flower | grid | 24 |
| The Grinch | strip | 25 |
| 2D Spiral Twirls | grid | 26 |
| angle and radius from coordinates | grid | 27 |
| XmasFlies | strip | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 57 |
| 1 White Fade | strip | 123 |
| 1-USFLAG_BLINK | strip | 112 |
| 1D Aurora Borealis | strip | 20 |
| 2 Colors | strip | 101 |
| 2 Purple Fade | strip | 123 |
| 2D Bouncing Additive Primaries | grid | 19 |
| 2D canvas example | grid | 76 |
| 2D sinc(theta)/theta | grid | 19 |
| 2D Spiral Twirls | grid | 26 |
| 2D Wandering Fireball | grid | 51 |
| 3 color rotation | strip | 71 |
| 3 Violet Fade | strip | 123 |
| 3D Rotation / Spotlights | cloud | 22 |
| 4 Blue Fade | strip | 123 |
| 4th | strip | 40 |
| 5 Teal Fade | strip | 123 |
| 6 Green Fade | strip | 122 |
| 7 Yelllow Fade | strip | 123 |
| 8 Red Fade | strip | 123 |
| 9 Pink Fade | strip | 123 |
| All Lasers Fire | grid | 15 |
| amoeba | strip | 12 |
| angle and radius from coordinates | grid | 27 |
| Angry Xmass 3D | cloud | 78 |
| Animated Asterisks 2D | grid | 7 |
| aurorashivers | strip | 21 |
| Austin FC | strip | 77 |
| Automap | strip | 123 |
| Autumn Colors | strip | 93 |
| b_lightning_flashes | strip | 120 |
| Bessel Chaos | strip | 52 |
| blink fade | strip | 70 |
| Blinky Eyes 2D | grid | 34 |
| block reflections | strip | 49 |
| Bouncer3D | grid | 23 |
| bouncing balls - hsv | strip | 86 |
| bouncing balls - rgb | strip | 72 |
| Bouncing RGB Balls - 2D | grid | 38 |
| Bouncy Boxes | grid | 98 |
| Breakout 2D | grid | 43 |
| bustle | strip | 18 |
| Cellular Automata 1D | strip | 100 |
| Chasing Rainbows & HSLuv | strip | 114 |
| chill confetti | strip | 121 |
| Christmas Candy Cane | strip | 57 |
| Christmas Lights | strip | 93 |
| Christmas RG Fade | strip | 71 |
| Christmas string lights | strip | 58 |
| ChristmasLights | strip | 93 |
| ChristmasPewPew | strip | 21 |
| ChristmasStretch | strip | 71 |
| color bands | strip | 44 |
| color bands (buffered) | strip | 36 |
| Color Blend | strip | 51 |
| color fade pulse | strip | 59 |
| Color Pick Fade | strip | 88 |
| color twinkle bounce | strip | 52 |
| color twinkles | strip | 47 |
| colourful fireflies | strip | 73 |
| Complements 3D | grid | 9 |
| Continuous Cellular Automata | grid | 33 |
| coolaura | strip | 15 |
| Crossfading | grid | 23 |
| Crosstown Traffic 2D | grid | 123 |
| Custom Sequences | strip | 15 |
| Cyclic Cellular Automata 2D | grid | 61 |
| Cylon | strip | 69 |
| DAFTPUNK | grid | 95 |
| DBZBattleFinal | grid | 14 |
| dimbypixel | strip | 123 |
| Doom Fire | grid | 64 |
| Doom Fire (v2.0) 2D | grid | 61 |
| Easing Library v1.0 | grid | 74 |
| Easing Library v1.01 | grid | 68 |
| Edgeburst | strip | 78 |
| Emoji Animation #2 | grid | 20 |
| Example: color hues | strip | 119 |
| Example: modes and waveforms | strip | 108 |
| Example: Smooth Speed Slider | strip | 122 |
| Example: time and animation | strip | 95 |
| fast pulse | strip | 80 |
| fast pulse 3d | grid | 80 |
| fire - blue | strip | 32 |
| fire - red | strip | 33 |
| fireblobs | strip | 21 |
| FireFlies | strip | 76 |
| firework dust | strip | 123 |
| firework nova | cloud | 35 |
| firework rocket sparks | strip | 56 |
| fractal flower | grid | 24 |
| Frogger 2D | grid | 122 |
| glitch bands | strip | 35 |
| Golden Tix | strip | 74 |
| Gradient blue  purple pink | strip | 61 |
| green ripple reflections | strip | 46 |
| Halloween color twinkles | strip | 43 |
| heart | grid | 33 |
| heatshivers | strip | 21 |
| Holiday_Diagonal_Stripes | grid | 79 |
| Ice Floes 2D | grid | 12 |
| Icicleblaze | grid | 38 |
| index walk | strip | 121 |
| Infinity Flower 2D | grid | 30 |
| Iran - Solidarity | cloud | 9 |
| KITT | strip | 68 |
| KITT (w/ color picker) | strip | 66 |
| lightning ZAP! | strip | 72 |
| Mandelbrot 2D | grid | 7 |
| Map - Concentric | grid | 122 |
| mapped vertical line 2D | grid | 119 |
| marching rainbow | strip | 55 |
| marching rainbow (buffered) | strip | 48 |
| Matrix 2 tone pulse | strip | 35 |
| matrix 2D honeycomb | strip | 40 |
| matrix 2D pulse edit | strip | 51 |
| Matrix Green Waterfall 2D | grid | 93 |
| matrix rain | grid | 59 |
| Metaballs of Fire 2D | grid | 15 |
| Meteor Shower | strip | 107 |
| MidpointDisplacement1D | strip | 116 |
| millipede | strip | 70 |
| millipede 1d/2d controls | grid | 79 |
| neutronorbit | strip | 13 |
| Newfire | strip | 66 |
| novas | strip | 18 |
| Nyan Lights | strip | 51 |
| opposites | strip | 46 |
| Performance test framework | strip | 46 |
| Pew-Pew-Pew! | strip | 19 |
| policeLights | strip | 123 |
| portal | strip | 9 |
| Pride Progress | strip | 61 |
| quiet blinkfade | strip | 83 |
| rainbow | strip | 122 |
| Rainbow Comet | strip | 58 |
| Rainbow Flag | strip | 73 |
| rainbow fonts | strip | 91 |
| rainbow fonts 2 | strip | 83 |
| rainbow melt | strip | 66 |
| rainbow pinwheel | strip | 117 |
| Rainbow rocket sparks | strip | 72 |
| Rainbow Smiley | grid | 32 |
| Rainbow v2 | strip | 121 |
| Raindrops 2D | grid | 57 |
| Red-Green XY 2D Sweep | grid | 79 |
| regenbogendrogen | strip | 89 |
| RGB Test Pattern | strip | 116 |
| RGB-XYZ 3D Octants | cloud | 103 |
| RGBW Mapping Tester | strip | 116 |
| RGBW Mapping Tester - HSV Version | strip | 111 |
| Scanner | strip | 57 |
| scrolls | strip | 11 |
| Shimmer Crossfade 2D | grid | 35 |
| Sierpinski Rainbow 2D | grid | 67 |
| Single Color Picker - wide or spot | strip | 101 |
| sinpulse 3D | grid | 50 |
| sinus | grid | 56 |
| slow color shift | strip | 51 |
| slowflies | strip | 20 |
| snake | strip | 82 |
| Solid Rainbow | strip | 123 |
| sparkfire | strip | 33 |
| sparks | strip | 78 |
| sparks center | strip | 81 |
| spin cycle | strip | 58 |
| Spinwheel 2D | grid | 35 |
| spotlights / rotation 3D | grid | 23 |
| Spring Colors | strip | 93 |
| Stacker | strip | 88 |
| StarGen polar 2D | grid | 19 |
| Static Christmas Lights - 4 Colors | strip | 103 |
| static random colors | strip | 41 |
| Swirlpool 2D | grid | 64 |
| Synchronized Random Numbers | strip | 32 |
| The Grinch | strip | 25 |
| Three Red Pixels (array) | strip | 79 |
| Three Red Pixels (mathy) | strip | 108 |
| Thunderstorm | strip | 64 |
| Time Flies 2D | grid | 122 |
| tixy | grid | 67 |
| Twinkle | strip | 69 |
| Twinkling Classic Xmas Strands | strip | 20 |
| TwoColorHSVMix | strip | 46 |
| Unstable Orbits | grid | 81 |
| Utility: Palettes | strip | 13 |
| Utility: Perceptual hue | strip | 102 |
| UtilityColorTemp | strip | 123 |
| Voronoi Mix 2D | grid | 12 |
| wanderedges | strip | 10 |
| wanderers | strip | 67 |
| White Rainbows | strip | 73 |
| Wichmann–Hill PRNG | strip | 21 |
| XmasFlies | strip | 29 |
| xorcery 2D/3D | grid | 33 |

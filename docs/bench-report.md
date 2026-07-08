# Hardware soak + benchmark — 2026-07-08

*Device 192.168.0.205, firmware v0.1.22, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 195 patterns: **134 clean**, 61 with errors, 16 under 30 fps.
- fps at 300 px: median **71**, p10 28, p90 121.
- lowest heap_free seen while soaking: 31912 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 124 |
| 150 | 124 |
| 300 | 106 |
| 600 | 53 |
| 1024 | 32 |
| 2048 | 16 |

## Errors

| pattern | kind | problem |
|---|---|---|
| _Fairies | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| 1D Aurora Borealis | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| 3D Rotation / Spotlights | cloud | push failed: TimeoutError: The operation was aborted due to timeout |
| 4th | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| amoeba | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Animated Asterisks 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| aurorashivers | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Autumn Colors | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| b_lightning_flashes | strip | push failed: TypeError: fetch failed |
| Bouncer3D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| bouncing balls - hsv | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| bouncing balls - rgb | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Bouncy Boxes | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Breakout 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| bustle | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Cellular Automata 1D | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Chasing Rainbows & HSLuv | strip | push failed: SyntaxError: Unexpected token 'b', "body read failed" is not valid JSON |
| ChristmasPewPew | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Complements 3D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Continuous Cellular Automata | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| coolaura | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Crossfading | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Crosstown Traffic 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Custom Sequences | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Cyclic Cellular Automata 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| DAFTPUNK | grid | push failed: SyntaxError: Unexpected token 'b', "body read failed" is not valid JSON |
| DBZBattleFinal | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Doom Fire | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Doom Fire (v2.0) 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Easing Library v1.0 | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Easing Library v1.01 | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Emoji Animation #2 | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| fire - blue | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| fire - red | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| fireblobs | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| fractal flower | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Frogger 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| heatshivers | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Icicleblaze | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Infinity Flower 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Iran - Solidarity | cloud | push failed: TimeoutError: The operation was aborted due to timeout |
| MidpointDisplacement1D | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| neutronorbit | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| novas | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Nyan Lights | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Pew-Pew-Pew! | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| portal | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Raindrops 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| scrolls | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Shimmer Crossfade 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| slowflies | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| spotlights / rotation 3D | grid | push failed: TypeError: fetch failed |
| Spring Colors | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| StarGen polar 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Time Flies 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| tixy | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| Twinkling Classic Xmas Strands | strip | push failed: TypeError: fetch failed |
| Utility: Palettes | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| Voronoi Mix 2D | grid | push failed: TimeoutError: The operation was aborted due to timeout |
| wanderedges | strip | push failed: TimeoutError: The operation was aborted due to timeout |
| XmasFlies | strip | push failed: TimeoutError: The operation was aborted due to timeout |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Mandelbrot 2D | grid | 7 |
| Ice Floes 2D | grid | 13 |
| All Lasers Fire | grid | 14 |
| Metaballs of Fire 2D | grid | 15 |
| 2D sinc(theta)/theta | grid | 18 |
| 2D Bouncing Additive Primaries | grid | 19 |
| angle and radius from coordinates | grid | 19 |
| TwoColorHSVMix | strip | 22 |
| Wichmann–Hill PRNG | strip | 22 |
| 2D Spiral Twirls | grid | 23 |
| Matrix 2 tone pulse | strip | 24 |
| The Grinch | strip | 26 |
| xorcery 2D/3D | grid | 27 |
| Blinky Eyes 2D | grid | 28 |
| matrix 2D honeycomb | strip | 29 |
| Spinwheel 2D | grid | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | — |
| 1 White Fade | strip | 124 |
| 1-USFLAG_BLINK | strip | 114 |
| 1D Aurora Borealis | strip | — |
| 2 Colors | strip | 103 |
| 2 Purple Fade | strip | 99 |
| 2D Bouncing Additive Primaries | grid | 19 |
| 2D canvas example | grid | 61 |
| 2D sinc(theta)/theta | grid | 18 |
| 2D Spiral Twirls | grid | 23 |
| 2D Wandering Fireball | grid | 52 |
| 3 color rotation | strip | 74 |
| 3 Violet Fade | strip | 99 |
| 3D Rotation / Spotlights | cloud | — |
| 4 Blue Fade | strip | 98 |
| 4th | strip | — |
| 5 Teal Fade | strip | 98 |
| 6 Green Fade | strip | 99 |
| 7 Yelllow Fade | strip | 98 |
| 8 Red Fade | strip | 99 |
| 9 Pink Fade | strip | 97 |
| All Lasers Fire | grid | 14 |
| amoeba | strip | — |
| angle and radius from coordinates | grid | 19 |
| Angry Xmass 3D | cloud | 33 |
| Animated Asterisks 2D | grid | — |
| aurorashivers | strip | — |
| Austin FC | strip | 79 |
| Automap | strip | 124 |
| Autumn Colors | strip | — |
| b_lightning_flashes | strip | — |
| Bessel Chaos | strip | 54 |
| blink fade | strip | 72 |
| Blinky Eyes 2D | grid | 28 |
| block reflections | strip | 37 |
| Bouncer3D | grid | — |
| bouncing balls - hsv | strip | — |
| bouncing balls - rgb | strip | — |
| Bouncing RGB Balls - 2D | grid | 33 |
| Bouncy Boxes | grid | — |
| Breakout 2D | grid | — |
| bustle | strip | — |
| Cellular Automata 1D | strip | — |
| Chasing Rainbows & HSLuv | strip | — |
| chill confetti | strip | 120 |
| Christmas Candy Cane | strip | 55 |
| Christmas Lights | strip | 97 |
| Christmas RG Fade | strip | 74 |
| Christmas string lights | strip | 36 |
| ChristmasLights | strip | 97 |
| ChristmasPewPew | strip | — |
| ChristmasStretch | strip | 74 |
| color bands | strip | 40 |
| color bands (buffered) | strip | 37 |
| Color Blend | strip | 44 |
| color fade pulse | strip | 42 |
| Color Pick Fade | strip | 55 |
| color twinkle bounce | strip | 45 |
| color twinkles | strip | 48 |
| colourful fireflies | strip | 75 |
| Complements 3D | grid | — |
| Continuous Cellular Automata | grid | — |
| coolaura | strip | — |
| Crossfading | grid | — |
| Crosstown Traffic 2D | grid | — |
| Custom Sequences | strip | — |
| Cyclic Cellular Automata 2D | grid | — |
| Cylon | strip | 70 |
| DAFTPUNK | grid | — |
| DBZBattleFinal | grid | — |
| dimbypixel | strip | 106 |
| Doom Fire | grid | — |
| Doom Fire (v2.0) 2D | grid | — |
| Easing Library v1.0 | grid | — |
| Easing Library v1.01 | grid | — |
| Edgeburst | strip | 81 |
| Emoji Animation #2 | grid | — |
| Example: color hues | strip | 119 |
| Example: modes and waveforms | strip | 111 |
| Example: Smooth Speed Slider | strip | 123 |
| Example: time and animation | strip | 101 |
| fast pulse | strip | 68 |
| fast pulse 3d | grid | 65 |
| fire - blue | strip | — |
| fire - red | strip | — |
| fireblobs | strip | — |
| FireFlies | strip | 79 |
| firework dust | strip | 121 |
| firework nova | cloud | 32 |
| firework rocket sparks | strip | 51 |
| fractal flower | grid | — |
| Frogger 2D | grid | — |
| glitch bands | strip | 33 |
| Golden Tix | strip | 77 |
| Gradient blue  purple pink | strip | 64 |
| green ripple reflections | strip | 47 |
| Halloween color twinkles | strip | 42 |
| heart | grid | 35 |
| heatshivers | strip | — |
| Holiday_Diagonal_Stripes | grid | 82 |
| Ice Floes 2D | grid | 13 |
| Icicleblaze | grid | — |
| index walk | strip | 123 |
| Infinity Flower 2D | grid | — |
| Iran - Solidarity | cloud | — |
| KITT | strip | 70 |
| KITT (w/ color picker) | strip | 69 |
| lightning ZAP! | strip | 75 |
| Mandelbrot 2D | grid | 7 |
| Map - Concentric | grid | 123 |
| mapped vertical line 2D | grid | 87 |
| marching rainbow | strip | 47 |
| marching rainbow (buffered) | strip | 49 |
| Matrix 2 tone pulse | strip | 24 |
| matrix 2D honeycomb | strip | 29 |
| matrix 2D pulse edit | strip | 44 |
| Matrix Green Waterfall 2D | grid | 71 |
| matrix rain | grid | 51 |
| Metaballs of Fire 2D | grid | 15 |
| Meteor Shower | strip | 109 |
| MidpointDisplacement1D | strip | — |
| millipede | strip | 47 |
| millipede 1d/2d controls | grid | 62 |
| neutronorbit | strip | — |
| Newfire | strip | 68 |
| novas | strip | — |
| Nyan Lights | strip | — |
| opposites | strip | 40 |
| Performance test framework | strip | 49 |
| Pew-Pew-Pew! | strip | — |
| policeLights | strip | 124 |
| portal | strip | — |
| Pride Progress | strip | 59 |
| quiet blinkfade | strip | 84 |
| rainbow | strip | 123 |
| Rainbow Comet | strip | 55 |
| Rainbow Flag | strip | 78 |
| rainbow fonts | strip | 93 |
| rainbow fonts 2 | strip | 86 |
| rainbow melt | strip | 69 |
| rainbow pinwheel | strip | 121 |
| Rainbow rocket sparks | strip | 72 |
| Rainbow Smiley | grid | 124 |
| Rainbow v2 | strip | 122 |
| Raindrops 2D | grid | — |
| Red-Green XY 2D Sweep | grid | 81 |
| regenbogendrogen | strip | 93 |
| RGB Test Pattern | strip | 121 |
| RGB-XYZ 3D Octants | cloud | 110 |
| RGBW Mapping Tester | strip | 118 |
| RGBW Mapping Tester - HSV Version | strip | 87 |
| Scanner | strip | 59 |
| scrolls | strip | — |
| Shimmer Crossfade 2D | grid | — |
| Sierpinski Rainbow 2D | grid | 55 |
| Single Color Picker - wide or spot | strip | 104 |
| sinpulse 3D | grid | 43 |
| sinus | grid | 47 |
| slow color shift | strip | 52 |
| slowflies | strip | — |
| snake | strip | 84 |
| Solid Rainbow | strip | 124 |
| sparkfire | strip | 34 |
| sparks | strip | 80 |
| sparks center | strip | 84 |
| spin cycle | strip | 42 |
| Spinwheel 2D | grid | 29 |
| spotlights / rotation 3D | grid | — |
| Spring Colors | strip | — |
| Stacker | strip | 91 |
| StarGen polar 2D | grid | — |
| Static Christmas Lights - 4 Colors | strip | 108 |
| static random colors | strip | 43 |
| Swirlpool 2D | grid | 53 |
| Synchronized Random Numbers | strip | 33 |
| The Grinch | strip | 26 |
| Three Red Pixels (array) | strip | 82 |
| Three Red Pixels (mathy) | strip | 113 |
| Thunderstorm | strip | 54 |
| Time Flies 2D | grid | — |
| tixy | grid | — |
| Twinkle | strip | 75 |
| Twinkling Classic Xmas Strands | strip | — |
| TwoColorHSVMix | strip | 22 |
| Unstable Orbits | grid | 83 |
| Utility: Palettes | strip | — |
| Utility: Perceptual hue | strip | 76 |
| UtilityColorTemp | strip | 124 |
| Voronoi Mix 2D | grid | — |
| wanderedges | strip | — |
| wanderers | strip | 70 |
| White Rainbows | strip | 76 |
| Wichmann–Hill PRNG | strip | 22 |
| XmasFlies | strip | — |
| xorcery 2D/3D | grid | 27 |

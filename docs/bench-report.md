# Hardware soak + benchmark — 2026-07-08

*Device 192.168.0.205, firmware v0.1.27, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 195 patterns: **192 clean**, 3 with errors, 40 under 30 fps.
- fps at 300 px: median **65**, p10 19, p90 121.
- lowest heap_free seen while soaking: 60616 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 122 |
| 150 | 123 |
| 300 | 123 |
| 600 | 85 |
| 1024 | 50 |
| 2048 | 26 |

## Errors

| pattern | kind | problem |
|---|---|---|
| Breakout 2D | grid | line 0:0: array index out of bounds |
| Crosstown Traffic 2D | grid | line 0:0: array index out of bounds |
| Frogger 2D | grid | line 0:0: array index out of bounds |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Animated Asterisks 2D | grid | 7 |
| Mandelbrot 2D | grid | 7 |
| Complements 3D | grid | 9 |
| portal | strip | 9 |
| Iran - Solidarity | cloud | 10 |
| scrolls | strip | 10 |
| wanderedges | strip | 11 |
| amoeba | strip | 12 |
| Voronoi Mix 2D | grid | 12 |
| Ice Floes 2D | grid | 13 |
| neutronorbit | strip | 13 |
| Utility: Palettes | strip | 13 |
| DBZBattleFinal | grid | 14 |
| All Lasers Fire | grid | 15 |
| coolaura | strip | 16 |
| Custom Sequences | strip | 16 |
| Metaballs of Fire 2D | grid | 16 |
| bustle | strip | 18 |
| novas | strip | 18 |
| Pew-Pew-Pew! | strip | 19 |
| StarGen polar 2D | grid | 19 |
| 2D Bouncing Additive Primaries | grid | 20 |
| aurorashivers | strip | 20 |
| fireblobs | strip | 20 |
| Rainbow Smiley | grid | 20 |
| slowflies | strip | 20 |
| Twinkling Classic Xmas Strands | strip | 20 |
| 1D Aurora Borealis | strip | 21 |
| 2D sinc(theta)/theta | grid | 21 |
| ChristmasPewPew | strip | 21 |
| heatshivers | strip | 21 |
| Wichmann–Hill PRNG | strip | 21 |
| Bouncer3D | grid | 23 |
| Crossfading | grid | 23 |
| 3D Rotation / Spotlights | cloud | 24 |
| spotlights / rotation 3D | grid | 24 |
| fractal flower | grid | 25 |
| The Grinch | strip | 26 |
| 2D Spiral Twirls | grid | 29 |
| XmasFlies | strip | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 58 |
| 1 White Fade | strip | 122 |
| 1-USFLAG_BLINK | strip | 113 |
| 1D Aurora Borealis | strip | 21 |
| 2 Colors | strip | 101 |
| 2 Purple Fade | strip | 122 |
| 2D Bouncing Additive Primaries | grid | 20 |
| 2D canvas example | grid | 77 |
| 2D sinc(theta)/theta | grid | 21 |
| 2D Spiral Twirls | grid | 29 |
| 2D Wandering Fireball | grid | 52 |
| 3 color rotation | strip | 72 |
| 3 Violet Fade | strip | 123 |
| 3D Rotation / Spotlights | cloud | 24 |
| 4 Blue Fade | strip | 122 |
| 4th | strip | 40 |
| 5 Teal Fade | strip | 122 |
| 6 Green Fade | strip | 122 |
| 7 Yelllow Fade | strip | 122 |
| 8 Red Fade | strip | 122 |
| 9 Pink Fade | strip | 121 |
| All Lasers Fire | grid | 15 |
| amoeba | strip | 12 |
| angle and radius from coordinates | grid | 31 |
| Angry Xmass 3D | cloud | 53 |
| Animated Asterisks 2D | grid | 7 |
| aurorashivers | strip | 20 |
| Austin FC | strip | 77 |
| Automap | strip | 122 |
| Autumn Colors | strip | 93 |
| b_lightning_flashes | strip | 119 |
| Bessel Chaos | strip | 53 |
| blink fade | strip | 70 |
| Blinky Eyes 2D | grid | 38 |
| block reflections | strip | 50 |
| Bouncer3D | grid | 23 |
| bouncing balls - hsv | strip | 86 |
| bouncing balls - rgb | strip | 72 |
| Bouncing RGB Balls - 2D | grid | 38 |
| Bouncy Boxes | grid | 101 |
| Breakout 2D | grid | 50 |
| bustle | strip | 18 |
| Cellular Automata 1D | strip | 99 |
| Chasing Rainbows & HSLuv | strip | 116 |
| chill confetti | strip | 120 |
| Christmas Candy Cane | strip | 58 |
| Christmas Lights | strip | 94 |
| Christmas RG Fade | strip | 72 |
| Christmas string lights | strip | 58 |
| ChristmasLights | strip | 94 |
| ChristmasPewPew | strip | 21 |
| ChristmasStretch | strip | 71 |
| color bands | strip | 46 |
| color bands (buffered) | strip | 36 |
| Color Blend | strip | 52 |
| color fade pulse | strip | 60 |
| Color Pick Fade | strip | 88 |
| color twinkle bounce | strip | 53 |
| color twinkles | strip | 47 |
| colourful fireflies | strip | 72 |
| Complements 3D | grid | 9 |
| Continuous Cellular Automata | grid | 33 |
| coolaura | strip | 16 |
| Crossfading | grid | 23 |
| Crosstown Traffic 2D | grid | 123 |
| Custom Sequences | strip | 16 |
| Cyclic Cellular Automata 2D | grid | 65 |
| Cylon | strip | 68 |
| DAFTPUNK | grid | 96 |
| DBZBattleFinal | grid | 14 |
| dimbypixel | strip | 123 |
| Doom Fire | grid | 66 |
| Doom Fire (v2.0) 2D | grid | 62 |
| Easing Library v1.0 | grid | 75 |
| Easing Library v1.01 | grid | 70 |
| Edgeburst | strip | 79 |
| Emoji Animation #2 | grid | 34 |
| Example: color hues | strip | 118 |
| Example: modes and waveforms | strip | 101 |
| Example: Smooth Speed Slider | strip | 122 |
| Example: time and animation | strip | 91 |
| fast pulse | strip | 81 |
| fast pulse 3d | grid | 81 |
| fire - blue | strip | 33 |
| fire - red | strip | 33 |
| fireblobs | strip | 20 |
| FireFlies | strip | 77 |
| firework dust | strip | 123 |
| firework nova | cloud | 40 |
| firework rocket sparks | strip | 57 |
| fractal flower | grid | 25 |
| Frogger 2D | grid | 123 |
| glitch bands | strip | 35 |
| Golden Tix | strip | 75 |
| Gradient blue  purple pink | strip | 62 |
| green ripple reflections | strip | 45 |
| Halloween color twinkles | strip | 44 |
| heart | grid | 37 |
| heatshivers | strip | 21 |
| Holiday_Diagonal_Stripes | grid | 81 |
| Ice Floes 2D | grid | 13 |
| Icicleblaze | grid | 39 |
| index walk | strip | 121 |
| Infinity Flower 2D | grid | 30 |
| Iran - Solidarity | cloud | 10 |
| KITT | strip | 68 |
| KITT (w/ color picker) | strip | 66 |
| lightning ZAP! | strip | 73 |
| Mandelbrot 2D | grid | 7 |
| Map - Concentric | grid | 122 |
| mapped vertical line 2D | grid | 120 |
| marching rainbow | strip | 56 |
| marching rainbow (buffered) | strip | 48 |
| Matrix 2 tone pulse | strip | 35 |
| matrix 2D honeycomb | strip | 41 |
| matrix 2D pulse edit | strip | 52 |
| Matrix Green Waterfall 2D | grid | 93 |
| matrix rain | grid | 60 |
| Metaballs of Fire 2D | grid | 16 |
| Meteor Shower | strip | 106 |
| MidpointDisplacement1D | strip | 118 |
| millipede | strip | 70 |
| millipede 1d/2d controls | grid | 79 |
| neutronorbit | strip | 13 |
| Newfire | strip | 67 |
| novas | strip | 18 |
| Nyan Lights | strip | 51 |
| opposites | strip | 47 |
| Performance test framework | strip | 51 |
| Pew-Pew-Pew! | strip | 19 |
| policeLights | strip | 123 |
| portal | strip | 9 |
| Pride Progress | strip | 61 |
| quiet blinkfade | strip | 83 |
| rainbow | strip | 122 |
| Rainbow Comet | strip | 57 |
| Rainbow Flag | strip | 75 |
| rainbow fonts | strip | 90 |
| rainbow fonts 2 | strip | 83 |
| rainbow melt | strip | 68 |
| rainbow pinwheel | strip | 118 |
| Rainbow rocket sparks | strip | 57 |
| Rainbow Smiley | grid | 20 |
| Rainbow v2 | strip | 121 |
| Raindrops 2D | grid | 57 |
| Red-Green XY 2D Sweep | grid | 79 |
| regenbogendrogen | strip | 91 |
| RGB Test Pattern | strip | 118 |
| RGB-XYZ 3D Octants | cloud | 108 |
| RGBW Mapping Tester | strip | 112 |
| RGBW Mapping Tester - HSV Version | strip | 121 |
| Scanner | strip | 57 |
| scrolls | strip | 10 |
| Shimmer Crossfade 2D | grid | 40 |
| Sierpinski Rainbow 2D | grid | 68 |
| Single Color Picker - wide or spot | strip | 101 |
| sinpulse 3D | grid | 51 |
| sinus | grid | 56 |
| slow color shift | strip | 50 |
| slowflies | strip | 20 |
| snake | strip | 83 |
| Solid Rainbow | strip | 123 |
| sparkfire | strip | 33 |
| sparks | strip | 77 |
| sparks center | strip | 82 |
| spin cycle | strip | 58 |
| Spinwheel 2D | grid | 41 |
| spotlights / rotation 3D | grid | 24 |
| Spring Colors | strip | 93 |
| Stacker | strip | 90 |
| StarGen polar 2D | grid | 19 |
| Static Christmas Lights - 4 Colors | strip | 104 |
| static random colors | strip | 42 |
| Swirlpool 2D | grid | 65 |
| Synchronized Random Numbers | strip | 32 |
| The Grinch | strip | 26 |
| Three Red Pixels (array) | strip | 79 |
| Three Red Pixels (mathy) | strip | 108 |
| Thunderstorm | strip | 65 |
| Time Flies 2D | grid | 123 |
| tixy | grid | 69 |
| Twinkle | strip | 69 |
| Twinkling Classic Xmas Strands | strip | 20 |
| TwoColorHSVMix | strip | 47 |
| Unstable Orbits | grid | 81 |
| Utility: Palettes | strip | 13 |
| Utility: Perceptual hue | strip | 103 |
| UtilityColorTemp | strip | 122 |
| Voronoi Mix 2D | grid | 12 |
| wanderedges | strip | 11 |
| wanderers | strip | 67 |
| White Rainbows | strip | 72 |
| Wichmann–Hill PRNG | strip | 21 |
| XmasFlies | strip | 29 |
| xorcery 2D/3D | grid | 33 |

# Hardware soak + benchmark — 2026-07-08

*Device 192.168.0.205, firmware v0.1.27, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 195 patterns: **191 clean**, 4 with errors, 45 under 30 fps.
- fps at 300 px: median **56**, p10 17, p90 122.
- lowest heap_free seen while soaking: 60616 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 123 |
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
| Swirlpool 2D | grid | line 0:0: array index out of bounds |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Animated Asterisks 2D | grid | 7 |
| portal | strip | 7 |
| scrolls | strip | 8 |
| amoeba | strip | 9 |
| Complements 3D | grid | 9 |
| Iran - Solidarity | cloud | 9 |
| wanderedges | strip | 9 |
| coolaura | strip | 10 |
| Ice Floes 2D | grid | 12 |
| Mandelbrot 2D | grid | 12 |
| Voronoi Mix 2D | grid | 12 |
| DBZBattleFinal | grid | 13 |
| neutronorbit | strip | 13 |
| Utility: Palettes | strip | 13 |
| novas | strip | 14 |
| All Lasers Fire | grid | 15 |
| Metaballs of Fire 2D | grid | 15 |
| angle and radius from coordinates | grid | 16 |
| Custom Sequences | strip | 16 |
| 2D Spiral Twirls | grid | 17 |
| bustle | strip | 18 |
| fireblobs | strip | 19 |
| Pew-Pew-Pew! | strip | 19 |
| StarGen polar 2D | grid | 19 |
| 1D Aurora Borealis | strip | 20 |
| fractal flower | grid | 20 |
| heatshivers | strip | 20 |
| Rainbow Smiley | grid | 20 |
| slowflies | strip | 20 |
| Twinkling Classic Xmas Strands | strip | 20 |
| xorcery 2D/3D | grid | 20 |
| 2D Bouncing Additive Primaries | grid | 21 |
| 2D sinc(theta)/theta | grid | 21 |
| aurorashivers | strip | 21 |
| ChristmasPewPew | strip | 21 |
| Wichmann–Hill PRNG | strip | 21 |
| Bouncer3D | grid | 22 |
| 3D Rotation / Spotlights | cloud | 23 |
| Crossfading | grid | 23 |
| glitch bands | strip | 24 |
| spotlights / rotation 3D | grid | 24 |
| The Grinch | strip | 26 |
| XmasFlies | strip | 26 |
| Emoji Animation #2 | grid | 27 |
| firework nova | cloud | 28 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 57 |
| 1 White Fade | strip | 122 |
| 1-USFLAG_BLINK | strip | 112 |
| 1D Aurora Borealis | strip | 20 |
| 2 Colors | strip | 100 |
| 2 Purple Fade | strip | 123 |
| 2D Bouncing Additive Primaries | grid | 21 |
| 2D canvas example | grid | 61 |
| 2D sinc(theta)/theta | grid | 21 |
| 2D Spiral Twirls | grid | 17 |
| 2D Wandering Fireball | grid | 43 |
| 3 color rotation | strip | 64 |
| 3 Violet Fade | strip | 122 |
| 3D Rotation / Spotlights | cloud | 23 |
| 4 Blue Fade | strip | 123 |
| 4th | strip | 41 |
| 5 Teal Fade | strip | 123 |
| 6 Green Fade | strip | 123 |
| 7 Yelllow Fade | strip | 123 |
| 8 Red Fade | strip | 123 |
| 9 Pink Fade | strip | 123 |
| All Lasers Fire | grid | 15 |
| amoeba | strip | 9 |
| angle and radius from coordinates | grid | 16 |
| Angry Xmass 3D | cloud | 38 |
| Animated Asterisks 2D | grid | 7 |
| aurorashivers | strip | 21 |
| Austin FC | strip | 76 |
| Automap | strip | 123 |
| Autumn Colors | strip | 92 |
| b_lightning_flashes | strip | 118 |
| Bessel Chaos | strip | 53 |
| blink fade | strip | 70 |
| Blinky Eyes 2D | grid | 37 |
| block reflections | strip | 37 |
| Bouncer3D | grid | 22 |
| bouncing balls - hsv | strip | 86 |
| bouncing balls - rgb | strip | 71 |
| Bouncing RGB Balls - 2D | grid | 38 |
| Bouncy Boxes | grid | 101 |
| Breakout 2D | grid | 122 |
| bustle | strip | 18 |
| Cellular Automata 1D | strip | 100 |
| Chasing Rainbows & HSLuv | strip | 115 |
| chill confetti | strip | 121 |
| Christmas Candy Cane | strip | 58 |
| Christmas Lights | strip | 93 |
| Christmas RG Fade | strip | 71 |
| Christmas string lights | strip | 41 |
| ChristmasLights | strip | 94 |
| ChristmasPewPew | strip | 21 |
| ChristmasStretch | strip | 72 |
| color bands | strip | 46 |
| color bands (buffered) | strip | 36 |
| Color Blend | strip | 52 |
| color fade pulse | strip | 49 |
| Color Pick Fade | strip | 67 |
| color twinkle bounce | strip | 53 |
| color twinkles | strip | 47 |
| colourful fireflies | strip | 72 |
| Complements 3D | grid | 9 |
| Continuous Cellular Automata | grid | 30 |
| coolaura | strip | 10 |
| Crossfading | grid | 23 |
| Crosstown Traffic 2D | grid | 123 |
| Custom Sequences | strip | 16 |
| Cyclic Cellular Automata 2D | grid | 51 |
| Cylon | strip | 68 |
| DAFTPUNK | grid | 72 |
| DBZBattleFinal | grid | 13 |
| dimbypixel | strip | 123 |
| Doom Fire | grid | 53 |
| Doom Fire (v2.0) 2D | grid | 48 |
| Easing Library v1.0 | grid | 75 |
| Easing Library v1.01 | grid | 69 |
| Edgeburst | strip | 53 |
| Emoji Animation #2 | grid | 27 |
| Example: color hues | strip | 116 |
| Example: modes and waveforms | strip | 89 |
| Example: Smooth Speed Slider | strip | 122 |
| Example: time and animation | strip | 81 |
| fast pulse | strip | 51 |
| fast pulse 3d | grid | 50 |
| fire - blue | strip | 33 |
| fire - red | strip | 33 |
| fireblobs | strip | 19 |
| FireFlies | strip | 77 |
| firework dust | strip | 123 |
| firework nova | cloud | 28 |
| firework rocket sparks | strip | 57 |
| fractal flower | grid | 20 |
| Frogger 2D | grid | 123 |
| glitch bands | strip | 24 |
| Golden Tix | strip | 75 |
| Gradient blue  purple pink | strip | 62 |
| green ripple reflections | strip | 40 |
| Halloween color twinkles | strip | 34 |
| heart | grid | 38 |
| heatshivers | strip | 20 |
| Holiday_Diagonal_Stripes | grid | 81 |
| Ice Floes 2D | grid | 12 |
| Icicleblaze | grid | 38 |
| index walk | strip | 121 |
| Infinity Flower 2D | grid | 32 |
| Iran - Solidarity | cloud | 9 |
| KITT | strip | 67 |
| KITT (w/ color picker) | strip | 67 |
| lightning ZAP! | strip | 72 |
| Mandelbrot 2D | grid | 12 |
| Map - Concentric | grid | 122 |
| mapped vertical line 2D | grid | 121 |
| marching rainbow | strip | 56 |
| marching rainbow (buffered) | strip | 48 |
| Matrix 2 tone pulse | strip | 31 |
| matrix 2D honeycomb | strip | 32 |
| matrix 2D pulse edit | strip | 52 |
| Matrix Green Waterfall 2D | grid | 71 |
| matrix rain | grid | 51 |
| Metaballs of Fire 2D | grid | 15 |
| Meteor Shower | strip | 107 |
| MidpointDisplacement1D | strip | 117 |
| millipede | strip | 71 |
| millipede 1d/2d controls | grid | 79 |
| neutronorbit | strip | 13 |
| Newfire | strip | 65 |
| novas | strip | 14 |
| Nyan Lights | strip | 51 |
| opposites | strip | 47 |
| Performance test framework | strip | 56 |
| Pew-Pew-Pew! | strip | 19 |
| policeLights | strip | 123 |
| portal | strip | 7 |
| Pride Progress | strip | 50 |
| quiet blinkfade | strip | 81 |
| rainbow | strip | 122 |
| Rainbow Comet | strip | 47 |
| Rainbow Flag | strip | 74 |
| rainbow fonts | strip | 91 |
| rainbow fonts 2 | strip | 84 |
| rainbow melt | strip | 68 |
| rainbow pinwheel | strip | 120 |
| Rainbow rocket sparks | strip | 47 |
| Rainbow Smiley | grid | 20 |
| Rainbow v2 | strip | 119 |
| Raindrops 2D | grid | 49 |
| Red-Green XY 2D Sweep | grid | 80 |
| regenbogendrogen | strip | 91 |
| RGB Test Pattern | strip | 118 |
| RGB-XYZ 3D Octants | cloud | 104 |
| RGBW Mapping Tester | strip | 112 |
| RGBW Mapping Tester - HSV Version | strip | 122 |
| Scanner | strip | 57 |
| scrolls | strip | 8 |
| Shimmer Crossfade 2D | grid | 35 |
| Sierpinski Rainbow 2D | grid | 68 |
| Single Color Picker - wide or spot | strip | 101 |
| sinpulse 3D | grid | 51 |
| sinus | grid | 48 |
| slow color shift | strip | 51 |
| slowflies | strip | 20 |
| snake | strip | 83 |
| Solid Rainbow | strip | 123 |
| sparkfire | strip | 33 |
| sparks | strip | 78 |
| sparks center | strip | 82 |
| spin cycle | strip | 41 |
| Spinwheel 2D | grid | 36 |
| spotlights / rotation 3D | grid | 24 |
| Spring Colors | strip | 93 |
| Stacker | strip | 89 |
| StarGen polar 2D | grid | 19 |
| Static Christmas Lights - 4 Colors | strip | 105 |
| static random colors | strip | 42 |
| Swirlpool 2D | grid | 53 |
| Synchronized Random Numbers | strip | 32 |
| The Grinch | strip | 26 |
| Three Red Pixels (array) | strip | 80 |
| Three Red Pixels (mathy) | strip | 109 |
| Thunderstorm | strip | 64 |
| Time Flies 2D | grid | 123 |
| tixy | grid | 68 |
| Twinkle | strip | 75 |
| Twinkling Classic Xmas Strands | strip | 20 |
| TwoColorHSVMix | strip | 40 |
| Unstable Orbits | grid | 82 |
| Utility: Palettes | strip | 13 |
| Utility: Perceptual hue | strip | 103 |
| UtilityColorTemp | strip | 123 |
| Voronoi Mix 2D | grid | 12 |
| wanderedges | strip | 9 |
| wanderers | strip | 68 |
| White Rainbows | strip | 72 |
| Wichmann–Hill PRNG | strip | 21 |
| XmasFlies | strip | 26 |
| xorcery 2D/3D | grid | 20 |

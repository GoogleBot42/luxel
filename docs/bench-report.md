# Hardware soak + benchmark — 2026-07-20

*Device 192.168.0.205, firmware v0.1.28, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 322 patterns: **321 clean**, 1 with errors, 80 under 30 fps.
- fps at 300 px: median **56**, p10 13, p90 123.
- lowest heap_free seen while soaking: 53140 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 124 |
| 150 | 124 |
| 300 | 124 |
| 600 | 85 |
| 1024 | 50 |
| 2048 | 25 |

## Errors

| pattern | kind | problem |
|---|---|---|
| Music Sequencer - for V3 ONLY | grid | invalid bytecode: not enough memory for this pattern |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Slime mold palette | grid | 3 |
| Crosstown Traffic 2D | grid | 4 |
| Synchronized Random Numbers | strip | 5 |
| Frogger 2D | grid | 6 |
| Mandelbrot 2D | grid | 8 |
| 80s kid show | grid | 9 |
| Bouncy Boxes | grid | 9 |
| Dire Spider 2D | grid | 9 |
| Glittering Jewels | strip | 9 |
| RYB colors | grid | 9 |
| Bubble Column | strip | 10 |
| Butterfly 2D | grid | 10 |
| distance function kaleidoscope 2 | grid | 10 |
| Sun rays through trees | grid | 10 |
| Animated Asterisks 2D | grid | 11 |
| Coral Plasma | grid | 11 |
| Ice Floes 2D | grid | 11 |
| Kaleidoscope 2D | grid | 11 |
| perlin fire wind tunnel | grid | 11 |
| RGBclock 2D | grid | 11 |
| sound - spectroblots - pow fade | grid | 11 |
| Coronal Ejection 2D | grid | 12 |
| Crosshair Pulse 2D | grid | 12 |
| DBZBattleFinal | grid | 12 |
| zoom kaleidoscope | grid | 12 |
| 2D sinc(theta)/theta | grid | 13 |
| All Lasers Fire | grid | 13 |
| Eye of Sauron with movement | grid | 13 |
| Lightning clouds | grid | 13 |
| novas | strip | 13 |
| Radar 2D | grid | 13 |
| Scary Pumpkin | grid | 13 |
| Sunrise Alarm Clock | strip | 13 |
| 1D Aurora Borealis | strip | 14 |
| Eye of Sauron | grid | 14 |
| Iran - Solidarity | cloud | 14 |
| Voronoi 2D | grid | 14 |
| Coronal Mass Ejection | grid | 15 |
| fireblobs | strip | 15 |
| Halloween Wavy Bands | grid | 15 |
| Reaction Diffusion 2D | grid | 15 |
| perlin fire wind | grid | 16 |
| portal | strip | 16 |
| spiral twirls star 2D | grid | 16 |
| Metaballs of Fire 2D | grid | 17 |
| Oasis | strip | 17 |
| Utility: Palettes | strip | 17 |
| Wavy Bands | grid | 17 |
| Blue Holiday Candle 2D | grid | 18 |
| Blue Holiday Star 2D | grid | 18 |
| Ripples 2D | grid | 18 |
| 2d Clock with Hand Color Pickers | grid | 19 |
| Beat Bounce | strip | 19 |
| neutronorbit | strip | 19 |
| Soap 2D | grid | 19 |
| sound - spectromatrix render2D | grid | 19 |
| Wichmann–Hill PRNG | strip | 19 |
| Breathing Gradient | strip | 20 |
| Emoji Animation #2 | grid | 20 |
| Perlin fire | grid | 20 |
| sound - spectrokalidamandala | grid | 20 |
| Carrie's Holiday Star 2D | grid | 21 |
| scrolls | strip | 21 |
| Rainstorm | grid | 22 |
| sound - spectromatrix agc | grid | 22 |
| 2D Bouncing Additive Primaries | grid | 24 |
| Perlin/Simplex Noise 2D | grid | 24 |
| Geometry Morphing Demo 2D | grid | 25 |
| sound - spectro kalidastrip | strip | 25 |
| tixy | grid | 26 |
| Bouncer3D | grid | 27 |
| multimap simpledemo | grid | 27 |
| Twinkling Classic Xmas Strands | strip | 27 |
| xorcery 2D/3D | grid | 27 |
| angle and radius from coordinates | grid | 28 |
| coolaura | strip | 28 |
| fractal flower | grid | 28 |
| DNA Helix 2D | grid | 29 |
| Flash Posterize + Music Sequencer framework | grid | 29 |
| spotlights / rotation 3D | grid | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 74 |
| 1 White Fade | strip | 124 |
| 1-USFLAG_BLINK | strip | 120 |
| 1D Aurora Borealis | strip | 14 |
| 2 Colors | strip | 106 |
| 2 Purple Fade | strip | 123 |
| 2D Bouncing Additive Primaries | grid | 24 |
| 2D canvas example | grid | 96 |
| 2d Clock with Hand Color Pickers | grid | 19 |
| 2D Fireworks Fade | grid | 65 |
| 2D sinc(theta)/theta | grid | 13 |
| 2D Spiral Twirls | grid | 35 |
| 2D Wandering Fireball | grid | 69 |
| 3 color rotation | strip | 119 |
| 3 Violet Fade | strip | 123 |
| 3D Rotation / Spotlights | cloud | 31 |
| 4 Blue Fade | strip | 123 |
| 4th | strip | 49 |
| 5 Teal Fade | strip | 123 |
| 6 Green Fade | strip | 123 |
| 7 Yelllow Fade | strip | 123 |
| 8 Red Fade | strip | 124 |
| 80s kid show | grid | 9 |
| 9 Pink Fade | strip | 123 |
| A Peak Integrator | strip | 102 |
| Accelerometer level example | grid | 97 |
| All Lasers Fire | grid | 13 |
| amoeba | strip | 30 |
| An Intro to Pixelblaze Code | strip | 75 |
| angle and radius from coordinates | grid | 28 |
| Angry Xmass 3D | cloud | 64 |
| Animated Asterisks 2D | grid | 11 |
| Audio Volume Meter | strip | 45 |
| Aurora 2D | grid | 34 |
| aurorashivers | strip | 33 |
| Austin FC | strip | 113 |
| Automap | strip | 109 |
| Autumn Colors | strip | 102 |
| b_lightning_flashes | strip | 121 |
| Beat Bounce | strip | 19 |
| Bessel Chaos | strip | 52 |
| Blink Fade | strip | 66 |
| Blinky Eyes 2D | grid | 38 |
| block reflections | strip | 54 |
| Blue Holiday Candle 2D | grid | 18 |
| Blue Holiday Star 2D | grid | 18 |
| Boids 2D | grid | 68 |
| Bouncer3D | grid | 27 |
| bouncing balls - hsv | strip | 97 |
| bouncing balls - rgb | strip | 59 |
| Bouncing Balls 2D | grid | 49 |
| Bouncing RGB Balls - 2D | grid | 32 |
| Bouncy Boxes | grid | 9 |
| Breakout 2D | grid | 62 |
| Breathing Gradient | strip | 20 |
| Bubble Column | strip | 10 |
| bustle | strip | 39 |
| Butterfly 2D | grid | 10 |
| Carrie's Holiday Star 2D | grid | 21 |
| Cellular Automata 1D | strip | 102 |
| Chasing Rainbows & HSLuv | strip | 117 |
| Chevron 2D | grid | 122 |
| chill confetti | strip | 123 |
| Christmas Candy Cane | strip | 100 |
| Christmas Lights | strip | 116 |
| Christmas Lights (2) | strip | 116 |
| Christmas RG Fade | strip | 78 |
| Christmas string lights | strip | 96 |
| ChristmasLights | strip | 111 |
| ChristmasLights (2) | strip | 101 |
| ChristmasPewPew | strip | 117 |
| ChristmasStretch | strip | 70 |
| color bands | strip | 42 |
| color bands (buffered) | strip | 33 |
| Color Blend | strip | 64 |
| color fade pulse | strip | 63 |
| Color Pick Fade | strip | 97 |
| color twinkle bounce | strip | 72 |
| Color Twinkles | strip | 49 |
| colourful fireflies | strip | 115 |
| Comets | strip | 102 |
| Complements 3D | grid | 49 |
| Continuous Cellular Automata | grid | 34 |
| coolaura | strip | 28 |
| Coral Plasma | grid | 11 |
| Coronal Ejection 2D | grid | 12 |
| Coronal Mass Ejection | grid | 15 |
| Crawling Spider 2D | grid | 30 |
| Crossfading | strip | 38 |
| Crosshair Pulse 2D | grid | 12 |
| Crosstown Traffic 2D | grid | 4 |
| cube fire 3D | grid | 40 |
| Custom Sequences | strip | 33 |
| Cyclic Cellular Automata 2D | grid | 76 |
| Cylon | strip | 78 |
| DAFTPUNK | grid | 87 |
| DBZBattleFinal | grid | 12 |
| Digital Rain 2D | grid | 46 |
| dimbypixel | strip | 123 |
| Dire Spider 2D | grid | 9 |
| distance function kaleidoscope 2 | grid | 10 |
| DNA Helix 2D | grid | 29 |
| Doom Fire | grid | 46 |
| Doom Fire 2D | grid | 46 |
| Drip | strip | 117 |
| Easing Library v1.0 | grid | 87 |
| Easing Library v1.01 | grid | 121 |
| Edgeburst | strip | 94 |
| Emoji Animation #2 | grid | 20 |
| Example - Button w/ debounce | strip | 123 |
| Example: color hues | strip | 111 |
| Example: modes and waveforms | strip | 101 |
| Example: Smooth Speed Slider | strip | 123 |
| Example: time and animation | strip | 82 |
| Eye of Sauron | grid | 14 |
| Eye of Sauron with movement | grid | 13 |
| Falling Sand 2D | grid | 76 |
| Fast Palette Blending | strip | 123 |
| fast pulse | strip | 73 |
| fast pulse 3d | grid | 99 |
| fire - blue | strip | 31 |
| fire - red | strip | 34 |
| fireblobs | strip | 15 |
| Fireflies | strip | 109 |
| firework dust | strip | 123 |
| firework nova | grid | 38 |
| firework rocket sparks | strip | 74 |
| Flash Posterize + Music Sequencer framework | grid | 29 |
| Flow Field 2D | grid | 62 |
| fractal flower | grid | 28 |
| Frogger 2D | grid | 6 |
| Geometry Morphing Demo 2D | grid | 25 |
| glitch bands | strip | 47 |
| Glitter | strip | 78 |
| Glittering Jewels | strip | 9 |
| GlowFlow (3D coord transform API port) | grid | 34 |
| Golden Tix | strip | 92 |
| Gradient blue  purple pink | strip | 86 |
| green ripple reflections | strip | 44 |
| Halloween color twinkles | strip | 46 |
| Halloween Wavy Bands | grid | 15 |
| heart | grid | 47 |
| heatshivers | strip | 38 |
| Holiday_Diagonal_Stripes | grid | 90 |
| Ice Floes 2D | grid | 11 |
| Icicleblaze | grid | 64 |
| index walk | strip | 123 |
| Infinity Flower 2D | grid | 49 |
| Interference 2D | grid | 31 |
| Iran - Solidarity | cloud | 14 |
| Kaleidoscope 2D | grid | 11 |
| KITT | strip | 79 |
| KITT (w/ color picker) | strip | 73 |
| Light Organ - 2.0 | strip | 40 |
| Light Organ -- sensor board | strip | 37 |
| Lightbulb - Crank Hue to Complete | strip | 122 |
| Lightning clouds | grid | 13 |
| lightning ZAP! | strip | 84 |
| Line Dancer 2D | grid | 40 |
| Lissajous curve tracer | grid | 42 |
| M5Stack Hex panels | cloud | 34 |
| Mandelbrot 2D | grid | 8 |
| Map - Concentric | grid | 123 |
| mapped vertical line 2D | grid | 123 |
| Mapping Helper Single and 10x | strip | 101 |
| marching rainbow | strip | 58 |
| marching rainbow (buffered) | strip | 49 |
| Marquee Chase | strip | 117 |
| Matrix 2 tone pulse | strip | 39 |
| matrix 2D honeycomb | grid | 51 |
| matrix 2D pulse edit | strip | 46 |
| Matrix Green Waterfall 2D | grid | 87 |
| matrix rain | grid | 79 |
| Metaballs of Fire 2D | grid | 17 |
| Meteor Shower | strip | 121 |
| MidpointDisplacement1D | strip | 57 |
| millipede | strip | 72 |
| millipede 1d/2d controls | grid | 71 |
| multimap simpledemo | grid | 27 |
| Multisegment Demo | strip | 38 |
| Music Sequencer - for V3 ONLY | grid | 20 |
| Music Sequencer for v2 | grid | 75 |
| Nano Orbital | strip | 123 |
| NaturalLightSync | strip | 123 |
| neutronorbit | strip | 19 |
| Newfire | strip | 51 |
| novas | strip | 13 |
| Nyan Lights | grid | 55 |
| Oasis | strip | 17 |
| Ocean | strip | 43 |
| opposites | strip | 46 |
| Orv - Christmas Tree | grid | 53 |
| Palette Fire 2D | grid | 35 |
| Pendulum Wave | strip | 104 |
| Performance test framework | strip | 88 |
| Perlin fire | grid | 20 |
| perlin fire wind | grid | 16 |
| perlin fire wind tunnel | grid | 11 |
| Perlin/Simplex Noise 1D | strip | 43 |
| Perlin/Simplex Noise 2D | grid | 24 |
| Pew-Pew-Pew! | strip | 69 |
| pixelClock | strip | 84 |
| Polar mapping helper 2D / 3D | grid | 40 |
| policeLights | strip | 122 |
| portal | strip | 16 |
| Pride Progress | strip | 73 |
| quiet blinkfade | strip | 80 |
| Radar 2D | grid | 13 |
| radiant pulse 3 | grid | 32 |
| Rainbow | strip | 123 |
| Rainbow Comet | strip | 73 |
| Rainbow Flag | strip | 111 |
| rainbow fonts | strip | 83 |
| rainbow fonts 2 | strip | 71 |
| Rainbow Melt | strip | 75 |
| rainbow pinwheel | strip | 123 |
| Rainbow rocket sparks | strip | 63 |
| Rainbow Smiley | grid | 123 |
| Rainbow v2 | strip | 123 |
| Raindrops 2D | grid | 50 |
| Rainstorm | grid | 22 |
| Reaction Diffusion 2D | grid | 15 |
| Real World Lights | strip | 32 |
| Red-Green XY 2D Sweep | grid | 66 |
| regenbogendrogen | strip | 97 |
| RGB Test Pattern | strip | 115 |
| RGB-XYZ 3D Octants | cloud | 122 |
| RGB-XYZ 3D Sweep | cloud | 86 |
| RGBclock 2D | grid | 11 |
| RGBW Mapping Tester | strip | 123 |
| RGBW Mapping Tester - HSV Version | strip | 123 |
| Ripples 2D | grid | 18 |
| Rock sparks | grid | 47 |
| Rocket by Tony Hampton | strip | 95 |
| RYB colors | grid | 9 |
| SaberDeploy Tutorial | strip | 123 |
| Scanner | strip | 65 |
| Scary Pumpkin | grid | 13 |
| scrolling text marquee 2D | grid | 73 |
| scrolls | strip | 21 |
| Shimmer Crossfade 2D | grid | 44 |
| Sierpinski Rainbow 2D | grid | 60 |
| Single Color Picker - wide or spot | strip | 84 |
| sinpulse 3D | grid | 53 |
| sinus | grid | 54 |
| SkyPirate's Centered Spectrum | grid | 56 |
| Slime mold palette | grid | 3 |
| slow color shift | strip | 60 |
| slowflies | strip | 37 |
| snake | strip | 89 |
| Snake 2D | grid | 39 |
| Soap 2D | grid | 19 |
| Solid Rainbow | strip | 123 |
| sound - blinkfade | strip | 49 |
| SOUND - lavablob | grid | 37 |
| sound - rays | strip | 83 |
| sound - rays Frequency-BPM Reactive 1 | strip | 77 |
| sound - spectro kalidastrip | strip | 25 |
| sound - spectroblots - pow fade | grid | 11 |
| sound - spectrokalidamandala | grid | 20 |
| sound - spectromatrix agc | grid | 22 |
| sound - spectromatrix render2D | grid | 19 |
| Sound - Spectrum Analyser | grid | 61 |
| sound - Starburst 2 | strip | 57 |
| Sound & Music Spectrum Visualizer | strip | 49 |
| Sound Reactive Color Fade | grid | 124 |
| sparkfire | strip | 33 |
| sparks | strip | 117 |
| sparks center | strip | 97 |
| spin cycle | strip | 60 |
| Spinning Plasma 2D | grid | 43 |
| Spinwheel 2D | grid | 36 |
| Spiral 2D | grid | 38 |
| spiral twirls star 2D | grid | 16 |
| Spirograph 2D | grid | 82 |
| spotlights / rotation 3D | grid | 29 |
| Spring Colors | strip | 109 |
| Stacker | strip | 67 |
| Stairmaster 2D | grid | 59 |
| Starfield 2D | grid | 86 |
| StarGen polar 2D | grid | 30 |
| Static Christmas Lights - 4 Colors | strip | 122 |
| static random colors | strip | 40 |
| Sun rays through trees | grid | 10 |
| Sunrise | strip | 97 |
| Sunrise 2D | grid | 79 |
| Sunrise Alarm Clock | strip | 13 |
| Sunset | strip | 35 |
| Swirlpool 2D | grid | 92 |
| Synchronized Random Numbers | strip | 5 |
| Tetrix 2D | grid | 68 |
| The Grinch | strip | 100 |
| Three Red Pixels (array) | strip | 124 |
| Three Red Pixels (mathy) | strip | 118 |
| Thunderstorm | strip | 80 |
| Time Flies 2D | grid | 123 |
| tixy | grid | 26 |
| Traffic | grid | 38 |
| tree setup pattern | grid | 81 |
| Tunnel of Squares 2D | grid | 32 |
| TV Simulator | strip | 120 |
| Twinkle | strip | 86 |
| twinkle (2) | strip | 83 |
| Twinkling Classic Xmas Strands | strip | 27 |
| twinkly stars | strip | 119 |
| TwoColorHSVMix | strip | 47 |
| Typing Heatmap 2D | grid | 36 |
| Unstable Orbits 2D | grid | 85 |
| Upward waves 3D using accelerometer | cloud | 49 |
| Utility: Palettes | strip | 17 |
| Utility: Perceptual hue | strip | 119 |
| Utility: Scheduled Percent-On Demo | strip | 123 |
| UtilityColorTemp | strip | 123 |
| Voronoi 2D | grid | 14 |
| wanderedges | strip | 44 |
| wanderers | grid | 92 |
| Wavy Bands | grid | 17 |
| White Rainbows | strip | 69 |
| Wichmann–Hill PRNG | strip | 19 |
| XmasFlies | strip | 38 |
| xorcery 2D/3D | grid | 27 |
| zoom kaleidoscope | grid | 12 |

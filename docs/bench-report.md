# Hardware soak + benchmark — 2026-08-16

*Device 192.168.0.183, firmware v0.1.37, 300 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈15 min; runs every gallery pattern on the strip).*

## Summary

- 322 patterns: **322 clean**, 0 with errors, 140 under 30 fps.
- fps at 300 px: median **34**, p10 12, p90 55.
- lowest heap_free seen while soaking: 65840 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 124 |
| 150 | 102 |
| 300 | 53 |
| 600 | 28 |
| 1024 | 16 |
| 2048 | 9 |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Crosstown Traffic 2D | grid | 3 |
| Slime mold palette | grid | 3 |
| Synchronized Random Numbers | strip | 5 |
| Frogger 2D | grid | 6 |
| Glittering Jewels | strip | 6 |
| 80s kid show | grid | 7 |
| Bouncy Boxes | grid | 8 |
| Butterfly 2D | grid | 8 |
| RGBclock 2D | grid | 8 |
| RYB colors | grid | 8 |
| Dire Spider 2D | grid | 9 |
| distance function kaleidoscope 2 | grid | 9 |
| Radar 2D | grid | 9 |
| All Lasers Fire | grid | 10 |
| Animated Asterisks 2D | grid | 10 |
| Crosshair Pulse 2D | grid | 10 |
| Ice Floes 2D | grid | 10 |
| Sun rays through trees | grid | 10 |
| Voronoi 2D | grid | 10 |
| 2d Clock with Hand Color Pickers | grid | 11 |
| 2D sinc(theta)/theta | grid | 11 |
| Bubble Column | strip | 11 |
| DBZBattleFinal | grid | 11 |
| Iran - Solidarity | cloud | 11 |
| Kaleidoscope 2D | grid | 11 |
| Mandelbrot 2D | grid | 11 |
| perlin fire wind tunnel | grid | 11 |
| spiral twirls star 2D | grid | 11 |
| zoom kaleidoscope | grid | 11 |
| 1D Aurora Borealis | strip | 12 |
| Blue Holiday Candle 2D | grid | 12 |
| Blue Holiday Star 2D | grid | 12 |
| Eye of Sauron with movement | grid | 12 |
| Lightning clouds | grid | 12 |
| novas | strip | 12 |
| Oasis | strip | 12 |
| Sunrise Alarm Clock | strip | 12 |
| Beat Bounce | strip | 13 |
| Coral Plasma | grid | 13 |
| Coronal Ejection 2D | grid | 13 |
| Coronal Mass Ejection | grid | 13 |
| fireblobs | strip | 13 |
| Reaction Diffusion 2D | grid | 13 |
| Scary Pumpkin | grid | 13 |
| sound - spectroblots - pow fade | grid | 13 |
| Utility: Palettes | strip | 13 |
| Metaballs of Fire 2D | grid | 14 |
| sound - spectrokalidamandala | grid | 14 |
| Carrie's Holiday Star 2D | grid | 15 |
| Emoji Animation #2 | grid | 15 |
| Eye of Sauron | grid | 15 |
| neutronorbit | strip | 15 |
| Ripples 2D | grid | 15 |
| sound - spectromatrix render2D | grid | 15 |
| Wichmann–Hill PRNG | strip | 15 |
| angle and radius from coordinates | grid | 16 |
| Breathing Gradient | strip | 16 |
| fire - blue | strip | 16 |
| Geometry Morphing Demo 2D | grid | 16 |
| Perlin fire | grid | 16 |
| perlin fire wind | grid | 16 |
| portal | strip | 16 |
| Rainstorm | grid | 16 |
| Perlin/Simplex Noise 2D | grid | 17 |
| 2D Bouncing Additive Primaries | grid | 18 |
| Halloween Wavy Bands | grid | 18 |
| Interference 2D | grid | 18 |
| scrolls | strip | 18 |
| spotlights / rotation 3D | grid | 18 |
| StarGen polar 2D | grid | 18 |
| Twinkling Classic Xmas Strands | strip | 18 |
| firework nova | grid | 19 |
| fractal flower | grid | 19 |
| M5Stack Hex panels | cloud | 19 |
| radiant pulse 3 | grid | 19 |
| sound - spectromatrix agc | grid | 19 |
| Spinwheel 2D | grid | 19 |
| Spiral 2D | grid | 19 |
| tixy | grid | 19 |
| Wavy Bands | grid | 19 |
| DNA Helix 2D | grid | 20 |
| multimap simpledemo | grid | 20 |
| sound - spectro kalidastrip | strip | 20 |
| xorcery 2D/3D | grid | 20 |
| 2D Spiral Twirls | grid | 21 |
| 3D Rotation / Spotlights | cloud | 21 |
| Bouncing RGB Balls - 2D | grid | 21 |
| Crawling Spider 2D | grid | 21 |
| Flash Posterize + Music Sequencer framework | grid | 21 |
| Line Dancer 2D | grid | 21 |
| Bouncer3D | grid | 22 |
| Continuous Cellular Automata | grid | 22 |
| coolaura | strip | 22 |
| Custom Sequences | strip | 22 |
| fire - red | strip | 22 |
| static random colors | strip | 22 |
| Tunnel of Squares 2D | grid | 22 |
| 4th | strip | 23 |
| Blinky Eyes 2D | grid | 23 |
| Doom Fire | grid | 23 |
| Multisegment Demo | strip | 23 |
| Ocean | strip | 23 |
| Real World Lights | strip | 23 |
| Sunset | strip | 23 |
| amoeba | strip | 24 |
| Digital Rain 2D | grid | 24 |
| Matrix 2 tone pulse | strip | 24 |
| Newfire | strip | 24 |
| Rock sparks | grid | 24 |
| Traffic | grid | 24 |
| Shimmer Crossfade 2D | grid | 25 |
| SOUND - lavablob | grid | 25 |
| color bands (buffered) | strip | 26 |
| GlowFlow (3D coord transform API port) | grid | 26 |
| Halloween color twinkles | strip | 26 |
| Light Organ - 2.0 | strip | 26 |
| Palette Fire 2D | grid | 26 |
| sparkfire | strip | 26 |
| Spinning Plasma 2D | grid | 26 |
| aurorashivers | strip | 27 |
| bustle | strip | 27 |
| cube fire 3D | grid | 27 |
| glitch bands | strip | 27 |
| heart | grid | 27 |
| matrix 2D pulse edit | strip | 27 |
| Polar mapping helper 2D / 3D | grid | 27 |
| slowflies | strip | 27 |
| color bands | strip | 28 |
| Crossfading | strip | 28 |
| Doom Fire 2D | grid | 28 |
| green ripple reflections | strip | 28 |
| Light Organ -- sensor board | strip | 28 |
| matrix 2D honeycomb | grid | 28 |
| heatshivers | strip | 29 |
| opposites | strip | 29 |
| sinpulse 3D | grid | 29 |
| Soap 2D | grid | 29 |
| TwoColorHSVMix | strip | 29 |
| Upward waves 3D using accelerometer | cloud | 29 |
| XmasFlies | strip | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 44 |
| 1 White Fade | strip | 74 |
| 1-USFLAG_BLINK | strip | 57 |
| 1D Aurora Borealis | strip | 12 |
| 2 Colors | strip | 43 |
| 2 Purple Fade | strip | 75 |
| 2D Bouncing Additive Primaries | grid | 18 |
| 2D canvas example | grid | 38 |
| 2d Clock with Hand Color Pickers | grid | 11 |
| 2D Fireworks Fade | grid | 36 |
| 2D sinc(theta)/theta | grid | 11 |
| 2D Spiral Twirls | grid | 21 |
| 2D Wandering Fireball | grid | 37 |
| 3 color rotation | strip | 48 |
| 3 Violet Fade | strip | 74 |
| 3D Rotation / Spotlights | cloud | 21 |
| 4 Blue Fade | strip | 75 |
| 4th | strip | 23 |
| 5 Teal Fade | strip | 74 |
| 6 Green Fade | strip | 75 |
| 7 Yelllow Fade | strip | 75 |
| 8 Red Fade | strip | 75 |
| 80s kid show | grid | 7 |
| 9 Pink Fade | strip | 75 |
| A Peak Integrator | strip | 44 |
| Accelerometer level example | grid | 51 |
| All Lasers Fire | grid | 10 |
| amoeba | strip | 24 |
| An Intro to Pixelblaze Code | strip | 45 |
| angle and radius from coordinates | grid | 16 |
| Angry Xmass 3D | cloud | 41 |
| Animated Asterisks 2D | grid | 10 |
| Audio Volume Meter | strip | 32 |
| Aurora 2D | grid | 34 |
| aurorashivers | strip | 27 |
| Austin FC | strip | 56 |
| Automap | strip | 45 |
| Autumn Colors | strip | 52 |
| b_lightning_flashes | strip | 59 |
| Beat Bounce | strip | 13 |
| Bessel Chaos | strip | 31 |
| Blink Fade | strip | 41 |
| Blinky Eyes 2D | grid | 23 |
| block reflections | strip | 32 |
| Blue Holiday Candle 2D | grid | 12 |
| Blue Holiday Star 2D | grid | 12 |
| Boids 2D | grid | 42 |
| Bouncer3D | grid | 22 |
| bouncing balls - hsv | strip | 51 |
| bouncing balls - rgb | strip | 38 |
| Bouncing Balls 2D | grid | 32 |
| Bouncing RGB Balls - 2D | grid | 21 |
| Bouncy Boxes | grid | 8 |
| Breakout 2D | grid | 31 |
| Breathing Gradient | strip | 16 |
| Bubble Column | strip | 11 |
| bustle | strip | 27 |
| Butterfly 2D | grid | 8 |
| Carrie's Holiday Star 2D | grid | 15 |
| Cellular Automata 1D | strip | 43 |
| Chasing Rainbows & HSLuv | strip | 49 |
| Chevron 2D | grid | 60 |
| chill confetti | strip | 69 |
| Christmas Candy Cane | strip | 38 |
| Christmas Lights | strip | 47 |
| Christmas Lights (2) | strip | 47 |
| Christmas RG Fade | strip | 46 |
| Christmas string lights | strip | 41 |
| ChristmasLights | strip | 39 |
| ChristmasLights (2) | strip | 38 |
| ChristmasPewPew | strip | 53 |
| ChristmasStretch | strip | 39 |
| color bands | strip | 28 |
| color bands (buffered) | strip | 26 |
| Color Blend | strip | 36 |
| color fade pulse | strip | 35 |
| Color Pick Fade | strip | 43 |
| color twinkle bounce | strip | 38 |
| Color Twinkles | strip | 34 |
| colourful fireflies | strip | 56 |
| Comets | strip | 48 |
| Complements 3D | grid | 30 |
| Continuous Cellular Automata | grid | 22 |
| coolaura | strip | 22 |
| Coral Plasma | grid | 13 |
| Coronal Ejection 2D | grid | 13 |
| Coronal Mass Ejection | grid | 13 |
| Crawling Spider 2D | grid | 21 |
| Crossfading | strip | 28 |
| Crosshair Pulse 2D | grid | 10 |
| Crosstown Traffic 2D | grid | 3 |
| cube fire 3D | grid | 27 |
| Custom Sequences | strip | 22 |
| Cyclic Cellular Automata 2D | grid | 36 |
| Cylon | strip | 45 |
| DAFTPUNK | grid | 48 |
| DBZBattleFinal | grid | 11 |
| Digital Rain 2D | grid | 24 |
| dimbypixel | strip | 51 |
| Dire Spider 2D | grid | 9 |
| distance function kaleidoscope 2 | grid | 9 |
| DNA Helix 2D | grid | 20 |
| Doom Fire | grid | 23 |
| Doom Fire 2D | grid | 28 |
| Drip | strip | 56 |
| Easing Library v1.0 | grid | 36 |
| Easing Library v1.01 | grid | 49 |
| Edgeburst | strip | 43 |
| Emoji Animation #2 | grid | 15 |
| Example - Button w/ debounce | strip | 57 |
| Example: color hues | strip | 46 |
| Example: modes and waveforms | strip | 44 |
| Example: Smooth Speed Slider | strip | 55 |
| Example: time and animation | strip | 40 |
| Eye of Sauron | grid | 15 |
| Eye of Sauron with movement | grid | 12 |
| Falling Sand 2D | grid | 43 |
| Fast Palette Blending | strip | 53 |
| fast pulse | strip | 38 |
| fast pulse 3d | grid | 40 |
| fire - blue | strip | 16 |
| fire - red | strip | 22 |
| fireblobs | strip | 13 |
| Fireflies | strip | 54 |
| firework dust | strip | 61 |
| firework nova | grid | 19 |
| firework rocket sparks | strip | 37 |
| Flash Posterize + Music Sequencer framework | grid | 21 |
| Flow Field 2D | grid | 42 |
| fractal flower | grid | 19 |
| Frogger 2D | grid | 6 |
| Geometry Morphing Demo 2D | grid | 16 |
| glitch bands | strip | 27 |
| Glitter | strip | 46 |
| Glittering Jewels | strip | 6 |
| GlowFlow (3D coord transform API port) | grid | 26 |
| Golden Tix | strip | 44 |
| Gradient blue  purple pink | strip | 41 |
| green ripple reflections | strip | 28 |
| Halloween color twinkles | strip | 26 |
| Halloween Wavy Bands | grid | 18 |
| heart | grid | 27 |
| heatshivers | strip | 29 |
| Holiday_Diagonal_Stripes | grid | 42 |
| Ice Floes 2D | grid | 10 |
| Icicleblaze | grid | 36 |
| index walk | strip | 53 |
| Infinity Flower 2D | grid | 30 |
| Interference 2D | grid | 18 |
| Iran - Solidarity | cloud | 11 |
| Kaleidoscope 2D | grid | 11 |
| KITT | strip | 46 |
| KITT (w/ color picker) | strip | 43 |
| Light Organ - 2.0 | strip | 26 |
| Light Organ -- sensor board | strip | 28 |
| Lightbulb - Crank Hue to Complete | strip | 50 |
| Lightning clouds | grid | 12 |
| lightning ZAP! | strip | 47 |
| Line Dancer 2D | grid | 21 |
| Lissajous curve tracer | grid | 30 |
| M5Stack Hex panels | cloud | 19 |
| Mandelbrot 2D | grid | 11 |
| Map - Concentric | grid | 55 |
| mapped vertical line 2D | grid | 50 |
| Mapping Helper Single and 10x | strip | 38 |
| marching rainbow | strip | 34 |
| marching rainbow (buffered) | strip | 34 |
| Marquee Chase | strip | 47 |
| Matrix 2 tone pulse | strip | 24 |
| matrix 2D honeycomb | grid | 28 |
| matrix 2D pulse edit | strip | 27 |
| Matrix Green Waterfall 2D | grid | 48 |
| matrix rain | grid | 39 |
| Metaballs of Fire 2D | grid | 14 |
| Meteor Shower | strip | 59 |
| MidpointDisplacement1D | strip | 33 |
| millipede | strip | 38 |
| millipede 1d/2d controls | grid | 37 |
| multimap simpledemo | grid | 20 |
| Multisegment Demo | strip | 23 |
| Music Sequencer - for V3 ONLY | grid | 64 |
| Music Sequencer for v2 | grid | 40 |
| Nano Orbital | strip | 52 |
| NaturalLightSync | strip | 73 |
| neutronorbit | strip | 15 |
| Newfire | strip | 24 |
| novas | strip | 12 |
| Nyan Lights | grid | 32 |
| Oasis | strip | 12 |
| Ocean | strip | 23 |
| opposites | strip | 29 |
| Orv - Christmas Tree | grid | 36 |
| Palette Fire 2D | grid | 26 |
| Pendulum Wave | strip | 44 |
| Performance test framework | strip | 41 |
| Perlin fire | grid | 16 |
| perlin fire wind | grid | 16 |
| perlin fire wind tunnel | grid | 11 |
| Perlin/Simplex Noise 1D | strip | 30 |
| Perlin/Simplex Noise 2D | grid | 17 |
| Pew-Pew-Pew! | strip | 42 |
| pixelClock | strip | 35 |
| Polar mapping helper 2D / 3D | grid | 27 |
| policeLights | strip | 49 |
| portal | strip | 16 |
| Pride Progress | strip | 41 |
| quiet blinkfade | strip | 46 |
| Radar 2D | grid | 9 |
| radiant pulse 3 | grid | 19 |
| Rainbow | strip | 53 |
| Rainbow Comet | strip | 44 |
| Rainbow Flag | strip | 46 |
| rainbow fonts | strip | 40 |
| rainbow fonts 2 | strip | 37 |
| Rainbow Melt | strip | 38 |
| rainbow pinwheel | strip | 49 |
| Rainbow rocket sparks | strip | 34 |
| Rainbow Smiley | grid | 56 |
| Rainbow v2 | strip | 56 |
| Raindrops 2D | grid | 32 |
| Rainstorm | grid | 16 |
| Reaction Diffusion 2D | grid | 13 |
| Real World Lights | strip | 23 |
| Red-Green XY 2D Sweep | grid | 36 |
| regenbogendrogen | strip | 43 |
| RGB Test Pattern | strip | 55 |
| RGB-XYZ 3D Octants | cloud | 61 |
| RGB-XYZ 3D Sweep | cloud | 40 |
| RGBclock 2D | grid | 8 |
| RGBW Mapping Tester | strip | 50 |
| RGBW Mapping Tester - HSV Version | strip | 51 |
| Ripples 2D | grid | 15 |
| Rock sparks | grid | 24 |
| Rocket by Tony Hampton | strip | 50 |
| RYB colors | grid | 8 |
| SaberDeploy Tutorial | strip | 64 |
| Scanner | strip | 36 |
| Scary Pumpkin | grid | 13 |
| scrolling text marquee 2D | grid | 40 |
| scrolls | strip | 18 |
| Shimmer Crossfade 2D | grid | 25 |
| Sierpinski Rainbow 2D | grid | 34 |
| Single Color Picker - wide or spot | strip | 35 |
| sinpulse 3D | grid | 29 |
| sinus | grid | 36 |
| SkyPirate's Centered Spectrum | grid | 33 |
| Slime mold palette | grid | 3 |
| slow color shift | strip | 34 |
| slowflies | strip | 27 |
| snake | strip | 42 |
| Snake 2D | grid | 30 |
| Soap 2D | grid | 29 |
| Solid Rainbow | strip | 55 |
| sound - blinkfade | strip | 34 |
| SOUND - lavablob | grid | 25 |
| sound - rays | strip | 40 |
| sound - rays Frequency-BPM Reactive 1 | strip | 45 |
| sound - spectro kalidastrip | strip | 20 |
| sound - spectroblots - pow fade | grid | 13 |
| sound - spectrokalidamandala | grid | 14 |
| sound - spectromatrix agc | grid | 19 |
| sound - spectromatrix render2D | grid | 15 |
| Sound - Spectrum Analyser | grid | 34 |
| sound - Starburst 2 | strip | 33 |
| Sound & Music Spectrum Visualizer | strip | 30 |
| Sound Reactive Color Fade | grid | 73 |
| sparkfire | strip | 26 |
| sparks | strip | 56 |
| sparks center | strip | 51 |
| spin cycle | strip | 34 |
| Spinning Plasma 2D | grid | 26 |
| Spinwheel 2D | grid | 19 |
| Spiral 2D | grid | 19 |
| spiral twirls star 2D | grid | 11 |
| Spirograph 2D | grid | 47 |
| spotlights / rotation 3D | grid | 18 |
| Spring Colors | strip | 54 |
| Stacker | strip | 36 |
| Stairmaster 2D | grid | 30 |
| Starfield 2D | grid | 48 |
| StarGen polar 2D | grid | 18 |
| Static Christmas Lights - 4 Colors | strip | 49 |
| static random colors | strip | 22 |
| Sun rays through trees | grid | 10 |
| Sunrise | strip | 45 |
| Sunrise 2D | grid | 36 |
| Sunrise Alarm Clock | strip | 12 |
| Sunset | strip | 23 |
| Swirlpool 2D | grid | 49 |
| Synchronized Random Numbers | strip | 5 |
| Tetrix 2D | grid | 42 |
| The Grinch | strip | 44 |
| Three Red Pixels (array) | strip | 71 |
| Three Red Pixels (mathy) | strip | 40 |
| Thunderstorm | strip | 40 |
| Time Flies 2D | grid | 73 |
| tixy | grid | 19 |
| Traffic | grid | 24 |
| tree setup pattern | grid | 43 |
| Tunnel of Squares 2D | grid | 22 |
| TV Simulator | strip | 49 |
| Twinkle | strip | 48 |
| twinkle (2) | strip | 48 |
| Twinkling Classic Xmas Strands | strip | 18 |
| twinkly stars | strip | 48 |
| TwoColorHSVMix | strip | 29 |
| Typing Heatmap 2D | grid | 48 |
| Unstable Orbits 2D | grid | 47 |
| Upward waves 3D using accelerometer | cloud | 29 |
| Utility: Palettes | strip | 13 |
| Utility: Perceptual hue | strip | 48 |
| Utility: Scheduled Percent-On Demo | strip | 73 |
| UtilityColorTemp | strip | 74 |
| Voronoi 2D | grid | 10 |
| wanderedges | strip | 30 |
| wanderers | grid | 47 |
| Wavy Bands | grid | 19 |
| White Rainbows | strip | 38 |
| Wichmann–Hill PRNG | strip | 15 |
| XmasFlies | strip | 29 |
| xorcery 2D/3D | grid | 20 |
| zoom kaleidoscope | grid | 11 |

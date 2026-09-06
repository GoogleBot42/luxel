# Hardware soak + benchmark — 2026-09-06

*Device 192.168.0.238, firmware v0.1.40, 4096 px ws2812, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 299 patterns: **184 clean**, 115 with errors, 184 under 30 fps.
- fps at 4096 px (the count the sweep ran at): median **7**, p10 2, p90 17.
- lowest heap_free seen while soaking: 17984 bytes.
- **1 device crash** (unreachable after a push): after "Synchronized Random Numbers" (back in 106s, needed the reset cmd).

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 125 |
| 150 | 125 |
| 300 | 125 |
| 600 | 116 |
| 1024 | 68 |
| 2048 | 35 |

## Errors

| pattern | kind | problem |
|---|---|---|
| _Fairies | strip | indexing a non-array value |
| 2D Bouncing Additive Primaries | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| 2D canvas example | grid | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| 2d Clock with Hand Color Pickers | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| 2D Spiral Twirls | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| 2D Wandering Fireball | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| 3D Rotation / Spotlights | cloud | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| 4th | strip | feedback of a non-array |
| 80s kid show | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| amoeba | strip | indexing a non-array value |
| Angry Xmass 3D | cloud | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Animated Asterisks 2D | grid | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Audio Volume Meter | strip | indexing a non-array value |
| aurorashivers | strip | indexing a non-array value |
| Autumn Colors | strip | indexing a non-array value |
| Blink Fade | strip | indexing a non-array value |
| Blinky Eyes 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Blue Holiday Candle 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Blue Holiday Star 2D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Boids 2D | grid | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| Bouncer3D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| bouncing balls - hsv | strip | feedback of a non-array |
| bouncing balls - rgb | strip | indexing a non-array value |
| Breakout 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| bustle | strip | indexing a non-array value |
| Carrie's Holiday Star 2D | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Cellular Automata 1D | strip | indexing a non-array value |
| Chasing Rainbows & HSLuv | strip | array memory budget exceeded (pattern too large for this device) |
| Chevron 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| chill confetti | strip | feedback of a non-array |
| Christmas RG Fade | strip | indexing a non-array value |
| ChristmasPewPew | strip | feedback of a non-array |
| color bands (buffered) | strip | indexing a non-array value |
| colourful fireflies | strip | indexing a non-array value |
| Continuous Cellular Automata | grid | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| coolaura | strip | indexing a non-array value |
| Coronal Ejection 2D | grid | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| Coronal Mass Ejection | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Crawling Spider 2D | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Crosshair Pulse 2D | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Crosstown Traffic 2D | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| DBZBattleFinal | grid | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| Digital Rain 2D | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Dire Spider 2D | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| distance function kaleidoscope 2 | grid | indexing a non-array value |
| Doom Fire | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| Doom Fire 2D | grid | pattern too large for this device — it left only 8 KB of heap free (the firmware needs 20 KB to keep running) |
| Emoji Animation #2 | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Eye of Sauron | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Eye of Sauron with movement | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Falling Sand 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| fireblobs | strip | array index out of bounds |
| firework dust | strip | indexing a non-array value |
| Fireworks Finale | strip | feedback of a non-array |
| Flash Posterize + Music Sequencer framework | grid | call of a non-function value |
| heart | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| heatshivers | strip | feedback of a non-array |
| Holiday_Diagonal_Stripes | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Ice Floes 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Interference 2D | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Kaleidoscope 2D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Lightning clouds | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| Lightning Strike | strip | indexing a non-array value |
| Line Dancer 2D | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Lissajous curve tracer | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| M5Stack Hex panels | cloud | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Main Stage | grid | indexing a non-array value |
| Mandelbrot 2D | grid | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| marching rainbow (buffered) | strip | indexing a non-array value |
| Matrix Green Waterfall 2D | grid | pattern too large for this device — it left only 7 KB of heap free (the firmware needs 20 KB to keep running) |
| Metaballs of Fire 2D | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Meteor Shower | strip | indexing a non-array value |
| neutronorbit | strip | indexing a non-array value |
| novas | strip | indexing a non-array value |
| Nyan Lights | grid | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| Opening Act | grid | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Orv - Christmas Tree | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| Pew-Pew-Pew! | strip | feedback of a non-array |
| Radar 2D | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| radiant pulse 3 | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| Rainbow Comet | strip | indexing a non-array value |
| Raindrops 2D | grid | pattern too large for this device — it left only 7 KB of heap free (the firmware needs 20 KB to keep running) |
| Rainstorm | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| Reaction Diffusion 2D | grid | pattern too large for this device — it left only 6 KB of heap free (the firmware needs 20 KB to keep running) |
| RGB-XYZ 3D Sweep | cloud | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| RGBclock 2D | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Ripples 2D | grid | pattern too large for this device — it left only 8 KB of heap free (the firmware needs 20 KB to keep running) |
| Rock sparks | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Rocket by Tony Hampton | strip | feedback of a non-array |
| sinus | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| slowflies | strip | indexing a non-array value |
| Soap 2D | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - blinkfade | strip | indexing a non-array value |
| sound - rays | strip | indexing a non-array value |
| sound - rays Frequency-BPM Reactive 1 | strip | indexing a non-array value |
| sound - Starburst 2 | strip | indexing a non-array value |
| Sound & Music Spectrum Visualizer | strip | indexing a non-array value |
| Spinning Plasma 2D | grid | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Spinwheel 2D | grid | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Spirograph 2D | grid | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| Spring Colors | strip | indexing a non-array value |
| Stairmaster 2D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Starfield 2D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Sun rays through trees | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Sunrise 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Swirlpool 2D | grid | pattern too large for this device — it left only 8 KB of heap free (the firmware needs 20 KB to keep running) |
| Synchronized Random Numbers | strip | crashed (device back after 106s) |
| Twinkle | strip | indexing a non-array value |
| twinkle (2) | strip | indexing a non-array value |
| Twinkling Classic Xmas Strands | strip | array memory budget exceeded (pattern too large for this device) |
| wanderedges | strip | indexing a non-array value |
| White Rainbows | strip | indexing a non-array value |
| Wichmann–Hill PRNG | strip | indexing a non-array value |
| XmasFlies | strip | feedback of a non-array |
| zoom kaleidoscope | grid | indexing a non-array value |

## Slowest (< 30 fps at 4096 px)

| pattern | kind | fps |
|---|---|---:|
| Butterfly 2D | grid | 1 |
| fire - blue | strip | 1 |
| Glittering Jewels | strip | 1 |
| RYB colors | grid | 1 |
| 1D Aurora Borealis | strip | 2 |
| All Lasers Fire | grid | 2 |
| Bouncing Balls RGB 2D | grid | 2 |
| Bubble Column | strip | 2 |
| Coral Plasma | grid | 2 |
| fire - red | strip | 2 |
| Geometry Morphing Demo 2D | grid | 2 |
| millipede 1d/2d controls | grid | 2 |
| Newfire | strip | 2 |
| Oasis | strip | 2 |
| Perlin fire | grid | 2 |
| perlin fire wind | grid | 2 |
| portal | strip | 2 |
| sound - spectromatrix render2D | grid | 2 |
| spiral twirls star 2D | grid | 2 |
| Sunrise Alarm Clock | strip | 2 |
| Utility: Palettes | strip | 2 |
| Voronoi 2D | grid | 2 |
| angle and radius from coordinates | grid | 3 |
| Beat Bounce | strip | 3 |
| Bouncing RGB Balls - 2D | grid | 3 |
| Breathing Gradient | strip | 3 |
| DNA Helix 2D | grid | 3 |
| Frogger 2D | grid | 3 |
| multimap simpledemo | grid | 3 |
| perlin fire wind tunnel | grid | 3 |
| Post-Process Chain | strip | 3 |
| Real World Lights | strip | 3 |
| scrolls | strip | 3 |
| Slime mold palette | grid | 3 |
| sound - spectro kalidastrip | strip | 3 |
| sound - spectroblots - pow fade | grid | 3 |
| sound - spectrokalidamandala | grid | 3 |
| spotlights / rotation 3D | grid | 3 |
| StarGen polar 2D | grid | 3 |
| tixy | grid | 3 |
| US Flag 2D | grid | 3 |
| xorcery 2D/3D | grid | 3 |
| color bands | strip | 4 |
| Crossfading | strip | 4 |
| cube fire 3D | grid | 4 |
| Custom Sequences | strip | 4 |
| firework nova | grid | 4 |
| GlowFlow (3D coord transform API port) | grid | 4 |
| green ripple reflections | strip | 4 |
| Halloween Wavy Bands | grid | 4 |
| Infinity Flower 2D | grid | 4 |
| Light Organ -- sensor board | strip | 4 |
| Matrix 2 tone pulse | strip | 4 |
| Multisegment Demo | strip | 4 |
| Perlin/Simplex Noise 2D | grid | 4 |
| Polar mapping helper 2D / 3D | grid | 4 |
| SOUND - lavablob | grid | 4 |
| sparkfire | strip | 4 |
| spin cycle | strip | 4 |
| static random colors | strip | 4 |
| Sunset | strip | 4 |
| Traffic | grid | 4 |
| Tunnel of Squares 2D | grid | 4 |
| US Flag | strip | 4 |
| Wavy Bands | grid | 4 |
| Aurora 2D | grid | 5 |
| Bessel Chaos | strip | 5 |
| block reflections | strip | 5 |
| Color Twinkles | strip | 5 |
| Complements 3D | grid | 5 |
| fractal flower | grid | 5 |
| glitch bands | strip | 5 |
| Halloween color twinkles | strip | 5 |
| Light Organ - 2.0 | strip | 5 |
| matrix 2D honeycomb | grid | 5 |
| matrix 2D pulse edit | strip | 5 |
| Ocean | strip | 5 |
| opposites | strip | 5 |
| Perlin/Simplex Noise 1D | strip | 5 |
| sinpulse 3D | grid | 5 |
| Spiral 2D | grid | 5 |
| TwoColorHSVMix | strip | 5 |
| color fade pulse | strip | 6 |
| Ember Diffusion | strip | 6 |
| Grinch's Heist | grid | 6 |
| marching rainbow | strip | 6 |
| MidpointDisplacement1D | strip | 6 |
| Sierpinski Rainbow 2D | grid | 6 |
| slow color shift | strip | 6 |
| Upward waves 3D using accelerometer | cloud | 6 |
| ChristmasStretch | strip | 7 |
| Color Blend | strip | 7 |
| color twinkle bounce | strip | 7 |
| Curl Flow 2D | grid | 7 |
| millipede | strip | 7 |
| Palette Fire 2D | grid | 7 |
| quiet blinkfade | strip | 7 |
| Rainbow Melt | strip | 7 |
| Rainbow rocket sparks | strip | 7 |
| Scanner | strip | 7 |
| Sound - Spectrum Analyser | grid | 7 |
| Tetrix 2D | grid | 7 |
| Christmas Lights | strip | 8 |
| Color Pick Fade | strip | 8 |
| Cylon | strip | 8 |
| Example: time and animation | strip | 8 |
| fast pulse | strip | 8 |
| firework rocket sparks | strip | 8 |
| Glitter | strip | 8 |
| Infinite Snake | grid | 8 |
| KITT (w/ color picker) | strip | 8 |
| rainbow fonts | strip | 8 |
| regenbogendrogen | strip | 8 |
| scrolling text marquee 2D | grid | 8 |
| snake | strip | 8 |
| sound - spectromatrix agc | grid | 8 |
| sparks center | strip | 8 |
| Stacker | strip | 8 |
| Thunderstorm | strip | 8 |
| Bouncing Balls 2D | grid | 9 |
| Bouncy Boxes | grid | 9 |
| Cyclic Cellular Automata 2D | grid | 9 |
| Flow Field 2D | grid | 9 |
| Gradient blue  purple pink | strip | 9 |
| KITT | strip | 9 |
| pixelClock | strip | 9 |
| Single Color Picker - wide or spot | strip | 9 |
| Accelerometer level example | grid | 10 |
| Edgeburst | strip | 10 |
| Example: modes and waveforms | strip | 10 |
| fast pulse 3d | grid | 10 |
| Golden Tix | strip | 10 |
| Sunrise | strip | 10 |
| tree setup pattern | grid | 10 |
| Unstable Orbits 2D | grid | 10 |
| 2 Colors | strip | 11 |
| Christmas Candy Cane | strip | 11 |
| Mapping Helper Single and 10x | strip | 11 |
| Matrix Green Waterfall 1D | strip | 11 |
| Pendulum Wave | strip | 11 |
| Performance test framework | strip | 11 |
| wanderers | grid | 11 |
| ChristmasLights | strip | 12 |
| Example: color hues | strip | 12 |
| Fireflies | strip | 12 |
| Marquee Chase | strip | 12 |
| Rainbow Flag | strip | 12 |
| rainbow pinwheel | strip | 12 |
| Three Red Pixels (array) | strip | 12 |
| twinkly stars | strip | 12 |
| 3 color rotation | strip | 13 |
| Drip | strip | 13 |
| Easing Library v1.0 | grid | 13 |
| policeLights | strip | 14 |
| RGBW Mapping Tester | strip | 14 |
| Static Christmas Lights - 4 Colors | strip | 14 |
| TV Simulator | strip | 14 |
| Typing Heatmap 2D | grid | 14 |
| Marching Dots | strip | 15 |
| sparks | strip | 15 |
| b_lightning_flashes | strip | 16 |
| Fast Palette Blending | strip | 16 |
| Lightbulb - Crank Hue to Complete | strip | 16 |
| RGBW Mapping Tester - HSV Version | strip | 16 |
| Nano Orbital | strip | 17 |
| SaberDeploy Tutorial | strip | 17 |
| Comets | strip | 18 |
| Rainbow | strip | 18 |
| Rainbow v2 | strip | 19 |
| 2D Fireworks Fade | grid | 20 |
| 2D sinc(theta)/theta | grid | 20 |
| matrix rain | grid | 20 |
| Scary Pumpkin | grid | 20 |
| Shimmer Crossfade 2D | grid | 20 |
| A Peak Integrator | strip | 21 |
| Example: Smooth Speed Slider | strip | 21 |
| Rainbow Smiley | grid | 21 |
| Example - Button w/ debounce | strip | 24 |
| 2 Purple Fade | strip | 27 |
| 1 White Fade | strip | 28 |
| Sound Reactive Color Fade | grid | 28 |
| Time Flies 2D | grid | 28 |
| NaturalLightSync | strip | 29 |
| UtilityColorTemp | strip | 29 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 29 |
| 1 White Fade | strip | 28 |
| 1D Aurora Borealis | strip | 2 |
| 2 Colors | strip | 11 |
| 2 Purple Fade | strip | 27 |
| 2D Bouncing Additive Primaries | grid | 20 |
| 2D canvas example | grid | 20 |
| 2d Clock with Hand Color Pickers | grid | 20 |
| 2D Fireworks Fade | grid | 20 |
| 2D sinc(theta)/theta | grid | 20 |
| 2D Spiral Twirls | grid | 20 |
| 2D Wandering Fireball | grid | 20 |
| 3 color rotation | strip | 13 |
| 3D Rotation / Spotlights | cloud | 20 |
| 4th | strip | 6 |
| 80s kid show | grid | 20 |
| A Peak Integrator | strip | 21 |
| Accelerometer level example | grid | 10 |
| All Lasers Fire | grid | 2 |
| amoeba | strip | 7 |
| angle and radius from coordinates | grid | 3 |
| Angry Xmass 3D | cloud | 20 |
| Animated Asterisks 2D | grid | 20 |
| Audio Volume Meter | strip | 18 |
| Aurora 2D | grid | 5 |
| aurorashivers | strip | 20 |
| Autumn Colors | strip | 29 |
| b_lightning_flashes | strip | 16 |
| Beat Bounce | strip | 3 |
| Bessel Chaos | strip | 5 |
| Blink Fade | strip | 21 |
| Blinky Eyes 2D | grid | 20 |
| block reflections | strip | 5 |
| Blue Holiday Candle 2D | grid | 20 |
| Blue Holiday Star 2D | grid | 20 |
| Boids 2D | grid | 20 |
| Bouncer3D | grid | 20 |
| bouncing balls - hsv | strip | 11 |
| bouncing balls - rgb | strip | 10 |
| Bouncing Balls 2D | grid | 9 |
| Bouncing Balls RGB 2D | grid | 2 |
| Bouncing RGB Balls - 2D | grid | 3 |
| Bouncy Boxes | grid | 9 |
| Breakout 2D | grid | 20 |
| Breathing Gradient | strip | 3 |
| Bubble Column | strip | 2 |
| bustle | strip | 22 |
| Butterfly 2D | grid | 1 |
| Carrie's Holiday Star 2D | grid | 20 |
| Cellular Automata 1D | strip | 28 |
| Chasing Rainbows & HSLuv | strip | 12 |
| Chevron 2D | grid | 20 |
| chill confetti | strip | 22 |
| Christmas Candy Cane | strip | 11 |
| Christmas Lights | strip | 8 |
| Christmas RG Fade | strip | 20 |
| ChristmasLights | strip | 12 |
| ChristmasPewPew | strip | 18 |
| ChristmasStretch | strip | 7 |
| color bands | strip | 4 |
| color bands (buffered) | strip | 23 |
| Color Blend | strip | 7 |
| color fade pulse | strip | 6 |
| Color Pick Fade | strip | 8 |
| color twinkle bounce | strip | 7 |
| Color Twinkles | strip | 5 |
| colourful fireflies | strip | 20 |
| Comets | strip | 18 |
| Complements 3D | grid | 5 |
| Continuous Cellular Automata | grid | 20 |
| coolaura | strip | 6 |
| Coral Plasma | grid | 2 |
| Coronal Ejection 2D | grid | 20 |
| Coronal Mass Ejection | grid | 20 |
| Crawling Spider 2D | grid | 20 |
| Crossfading | strip | 4 |
| Crosshair Pulse 2D | grid | 20 |
| Crosstown Traffic 2D | grid | 20 |
| cube fire 3D | grid | 4 |
| Curl Flow 2D | grid | 7 |
| Custom Sequences | strip | 4 |
| Cyclic Cellular Automata 2D | grid | 9 |
| Cylon | strip | 8 |
| DBZBattleFinal | grid | 20 |
| Digital Rain 2D | grid | 20 |
| Dire Spider 2D | grid | 20 |
| distance function kaleidoscope 2 | grid | 3 |
| DNA Helix 2D | grid | 3 |
| Doom Fire | grid | 20 |
| Doom Fire 2D | grid | 20 |
| Drip | strip | 13 |
| Easing Library v1.0 | grid | 13 |
| Edgeburst | strip | 10 |
| Ember Diffusion | strip | 6 |
| Emoji Animation #2 | grid | 20 |
| Example - Button w/ debounce | strip | 24 |
| Example: color hues | strip | 12 |
| Example: modes and waveforms | strip | 10 |
| Example: Smooth Speed Slider | strip | 21 |
| Example: time and animation | strip | 8 |
| Eye of Sauron | grid | 20 |
| Eye of Sauron with movement | grid | 20 |
| Falling Sand 2D | grid | 20 |
| Fast Palette Blending | strip | 16 |
| fast pulse | strip | 8 |
| fast pulse 3d | grid | 10 |
| fire - blue | strip | 1 |
| fire - red | strip | 2 |
| fireblobs | strip | 26 |
| Fireflies | strip | 12 |
| firework dust | strip | 9 |
| firework nova | grid | 4 |
| firework rocket sparks | strip | 8 |
| Fireworks Finale | strip | 21 |
| Flash Posterize + Music Sequencer framework | grid | 13 |
| Flow Field 2D | grid | 9 |
| fractal flower | grid | 5 |
| Frogger 2D | grid | 3 |
| Geometry Morphing Demo 2D | grid | 2 |
| glitch bands | strip | 5 |
| Glitter | strip | 8 |
| Glittering Jewels | strip | 1 |
| GlowFlow (3D coord transform API port) | grid | 4 |
| Golden Tix | strip | 10 |
| Gradient blue  purple pink | strip | 9 |
| green ripple reflections | strip | 4 |
| Grinch's Heist | grid | 6 |
| Halloween color twinkles | strip | 5 |
| Halloween Wavy Bands | grid | 4 |
| heart | grid | 20 |
| heatshivers | strip | 27 |
| Holiday_Diagonal_Stripes | grid | 20 |
| Ice Floes 2D | grid | 20 |
| Infinite Snake | grid | 8 |
| Infinity Flower 2D | grid | 4 |
| Interference 2D | grid | 20 |
| Kaleidoscope 2D | grid | 20 |
| KITT | strip | 9 |
| KITT (w/ color picker) | strip | 8 |
| Light Organ - 2.0 | strip | 5 |
| Light Organ -- sensor board | strip | 4 |
| Lightbulb - Crank Hue to Complete | strip | 16 |
| Lightning clouds | grid | 20 |
| Lightning Strike | strip | 28 |
| Line Dancer 2D | grid | 20 |
| Lissajous curve tracer | grid | 20 |
| M5Stack Hex panels | cloud | 20 |
| Main Stage | grid | 29 |
| Mandelbrot 2D | grid | 20 |
| Mapping Helper Single and 10x | strip | 11 |
| Marching Dots | strip | 15 |
| marching rainbow | strip | 6 |
| marching rainbow (buffered) | strip | 21 |
| Marquee Chase | strip | 12 |
| Matrix 2 tone pulse | strip | 4 |
| matrix 2D honeycomb | grid | 5 |
| matrix 2D pulse edit | strip | 5 |
| Matrix Green Waterfall 1D | strip | 11 |
| Matrix Green Waterfall 2D | grid | 20 |
| matrix rain | grid | 20 |
| Metaballs of Fire 2D | grid | 20 |
| Meteor Shower | strip | 14 |
| MidpointDisplacement1D | strip | 6 |
| millipede | strip | 7 |
| millipede 1d/2d controls | grid | 2 |
| multimap simpledemo | grid | 3 |
| Multisegment Demo | strip | 4 |
| Nano Orbital | strip | 17 |
| NaturalLightSync | strip | 29 |
| neutronorbit | strip | 19 |
| Newfire | strip | 2 |
| novas | strip | 28 |
| Nyan Lights | grid | 20 |
| Oasis | strip | 2 |
| Ocean | strip | 5 |
| Opening Act | grid | 20 |
| opposites | strip | 5 |
| Orv - Christmas Tree | grid | 20 |
| Palette Fire 2D | grid | 7 |
| Pendulum Wave | strip | 11 |
| Performance test framework | strip | 11 |
| Perlin fire | grid | 2 |
| perlin fire wind | grid | 2 |
| perlin fire wind tunnel | grid | 3 |
| Perlin/Simplex Noise 1D | strip | 5 |
| Perlin/Simplex Noise 2D | grid | 4 |
| Pew-Pew-Pew! | strip | 15 |
| pixelClock | strip | 9 |
| Polar mapping helper 2D / 3D | grid | 4 |
| policeLights | strip | 14 |
| portal | strip | 2 |
| Post-Process Chain | strip | 3 |
| quiet blinkfade | strip | 7 |
| Radar 2D | grid | 20 |
| radiant pulse 3 | grid | 20 |
| Rainbow | strip | 18 |
| Rainbow Comet | strip | 21 |
| Rainbow Flag | strip | 12 |
| rainbow fonts | strip | 8 |
| Rainbow Melt | strip | 7 |
| rainbow pinwheel | strip | 12 |
| Rainbow rocket sparks | strip | 7 |
| Rainbow Smiley | grid | 21 |
| Rainbow v2 | strip | 19 |
| Raindrops 2D | grid | 20 |
| Rainstorm | grid | 20 |
| Reaction Diffusion 2D | grid | 20 |
| Real World Lights | strip | 3 |
| regenbogendrogen | strip | 8 |
| RGB-XYZ 3D Sweep | cloud | 20 |
| RGBclock 2D | grid | 20 |
| RGBW Mapping Tester | strip | 14 |
| RGBW Mapping Tester - HSV Version | strip | 16 |
| Ripples 2D | grid | 20 |
| Rock sparks | grid | 20 |
| Rocket by Tony Hampton | strip | 18 |
| RYB colors | grid | 1 |
| SaberDeploy Tutorial | strip | 17 |
| Scanner | strip | 7 |
| Scary Pumpkin | grid | 20 |
| scrolling text marquee 2D | grid | 8 |
| scrolls | strip | 3 |
| Shimmer Crossfade 2D | grid | 20 |
| Sierpinski Rainbow 2D | grid | 6 |
| Single Color Picker - wide or spot | strip | 9 |
| sinpulse 3D | grid | 5 |
| sinus | grid | 20 |
| Slime mold palette | grid | 3 |
| slow color shift | strip | 6 |
| slowflies | strip | 17 |
| snake | strip | 8 |
| Soap 2D | grid | 20 |
| sound - blinkfade | strip | 12 |
| SOUND - lavablob | grid | 4 |
| sound - rays | strip | 12 |
| sound - rays Frequency-BPM Reactive 1 | strip | 12 |
| sound - spectro kalidastrip | strip | 3 |
| sound - spectroblots - pow fade | grid | 3 |
| sound - spectrokalidamandala | grid | 3 |
| sound - spectromatrix agc | grid | 8 |
| sound - spectromatrix render2D | grid | 2 |
| Sound - Spectrum Analyser | grid | 7 |
| sound - Starburst 2 | strip | 12 |
| Sound & Music Spectrum Visualizer | strip | 13 |
| Sound Reactive Color Fade | grid | 28 |
| sparkfire | strip | 4 |
| sparks | strip | 15 |
| sparks center | strip | 8 |
| spin cycle | strip | 4 |
| Spinning Plasma 2D | grid | 20 |
| Spinwheel 2D | grid | 20 |
| Spiral 2D | grid | 5 |
| spiral twirls star 2D | grid | 2 |
| Spirograph 2D | grid | 20 |
| spotlights / rotation 3D | grid | 3 |
| Spring Colors | strip | 21 |
| Stacker | strip | 8 |
| Stairmaster 2D | grid | 20 |
| Starfield 2D | grid | 20 |
| StarGen polar 2D | grid | 3 |
| Static Christmas Lights - 4 Colors | strip | 14 |
| static random colors | strip | 4 |
| Sun rays through trees | grid | 20 |
| Sunrise | strip | 10 |
| Sunrise 2D | grid | 20 |
| Sunrise Alarm Clock | strip | 2 |
| Sunset | strip | 4 |
| Swirlpool 2D | grid | 20 |
| Synchronized Random Numbers | strip | — |
| Tetrix 2D | grid | 7 |
| Three Red Pixels (array) | strip | 12 |
| Thunderstorm | strip | 8 |
| Time Flies 2D | grid | 28 |
| tixy | grid | 3 |
| Traffic | grid | 4 |
| tree setup pattern | grid | 10 |
| Tunnel of Squares 2D | grid | 4 |
| TV Simulator | strip | 14 |
| Twinkle | strip | 24 |
| twinkle (2) | strip | 18 |
| Twinkling Classic Xmas Strands | strip | 4 |
| twinkly stars | strip | 12 |
| TwoColorHSVMix | strip | 5 |
| Typing Heatmap 2D | grid | 14 |
| Unstable Orbits 2D | grid | 10 |
| Upward waves 3D using accelerometer | cloud | 6 |
| US Flag | strip | 4 |
| US Flag 2D | grid | 3 |
| Utility: Palettes | strip | 2 |
| UtilityColorTemp | strip | 29 |
| Voronoi 2D | grid | 2 |
| wanderedges | strip | 8 |
| wanderers | grid | 11 |
| Wavy Bands | grid | 4 |
| White Rainbows | strip | 12 |
| Wichmann–Hill PRNG | strip | 2 |
| XmasFlies | strip | 25 |
| xorcery 2D/3D | grid | 3 |
| zoom kaleidoscope | grid | 7 |

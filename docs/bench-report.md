# Hardware soak + benchmark — 2026-08-31

*Device 192.168.0.183, firmware v0.1.39, 60 px SK9822, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 303 patterns: **302 clean**, 1 with errors, 7 under 30 fps.
- fps at 300 px: median **120**, p10 55, p90 123.
- lowest heap_free seen while soaking: 83012 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 123 |
| 150 | 120 |
| 300 | 64 |
| 600 | 33 |
| 1024 | 20 |
| 2048 | 10 |

## Errors

| pattern | kind | problem |
|---|---|---|
| sound - spectromatrix render2D | grid | array index out of bounds |

## Slowest (< 30 fps at 300 px)

| pattern | kind | fps |
|---|---|---:|
| Slime mold palette | grid | 3 |
| Bouncy Boxes | grid | 9 |
| Ice Floes 2D | grid | 11 |
| Crosstown Traffic 2D | grid | 14 |
| Reaction Diffusion 2D | grid | 16 |
| Synchronized Random Numbers | strip | 21 |
| sound - spectromatrix agc | grid | 26 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 123 |
| 1 White Fade | strip | 123 |
| 1D Aurora Borealis | strip | 51 |
| 2 Colors | strip | 123 |
| 2 Purple Fade | strip | 123 |
| 2D Bouncing Additive Primaries | grid | 69 |
| 2D canvas example | grid | 122 |
| 2d Clock with Hand Color Pickers | grid | 60 |
| 2D Fireworks Fade | grid | 118 |
| 2D sinc(theta)/theta | grid | 49 |
| 2D Spiral Twirls | grid | 90 |
| 2D Wandering Fireball | grid | 122 |
| 3 color rotation | strip | 122 |
| 3D Rotation / Spotlights | cloud | 84 |
| 4th | strip | 118 |
| 80s kid show | grid | 39 |
| A Peak Integrator | strip | 122 |
| Accelerometer level example | grid | 123 |
| All Lasers Fire | grid | 46 |
| amoeba | strip | 88 |
| angle and radius from coordinates | grid | 72 |
| Angry Xmass 3D | cloud | 122 |
| Animated Asterisks 2D | grid | 46 |
| Audio Volume Meter | strip | 108 |
| Aurora 2D | grid | 102 |
| aurorashivers | strip | 107 |
| Autumn Colors | strip | 122 |
| b_lightning_flashes | strip | 122 |
| Beat Bounce | strip | 61 |
| Bessel Chaos | strip | 122 |
| Blink Fade | strip | 122 |
| Blinky Eyes 2D | grid | 100 |
| block reflections | strip | 122 |
| Blue Holiday Candle 2D | grid | 53 |
| Blue Holiday Star 2D | grid | 69 |
| Boids 2D | grid | 116 |
| Bouncer3D | grid | 94 |
| bouncing balls - hsv | strip | 123 |
| bouncing balls - rgb | strip | 122 |
| Bouncing Balls 2D | grid | 66 |
| Bouncing Balls RGB 2D | grid | 58 |
| Bouncing RGB Balls - 2D | grid | 101 |
| Bouncy Boxes | grid | 9 |
| Breakout 2D | grid | 120 |
| Breathing Gradient | strip | 68 |
| Bubble Column | strip | 57 |
| bustle | strip | 108 |
| Butterfly 2D | grid | 49 |
| Carrie's Holiday Star 2D | grid | 76 |
| Cellular Automata 1D | strip | 122 |
| Chasing Rainbows & HSLuv | strip | 122 |
| Chevron 2D | grid | 123 |
| chill confetti | strip | 123 |
| Christmas Candy Cane | strip | 122 |
| Christmas Lights | strip | 123 |
| Christmas Lights (2) | strip | 123 |
| Christmas RG Fade | strip | 123 |
| ChristmasLights | strip | 122 |
| ChristmasPewPew | strip | 123 |
| ChristmasStretch | strip | 123 |
| color bands | strip | 121 |
| color bands (buffered) | strip | 112 |
| Color Blend | strip | 123 |
| color fade pulse | strip | 122 |
| Color Pick Fade | strip | 123 |
| color twinkle bounce | strip | 122 |
| Color Twinkles | strip | 122 |
| colourful fireflies | strip | 122 |
| Comets | strip | 123 |
| Complements 3D | grid | 119 |
| Continuous Cellular Automata | grid | 110 |
| coolaura | strip | 90 |
| Coral Plasma | grid | 78 |
| Coronal Ejection 2D | grid | 66 |
| Coronal Mass Ejection | grid | 74 |
| Crawling Spider 2D | grid | 91 |
| Crossfading | strip | 73 |
| Crosshair Pulse 2D | grid | 53 |
| Crosstown Traffic 2D | grid | 14 |
| cube fire 3D | grid | 118 |
| Custom Sequences | strip | 114 |
| Cyclic Cellular Automata 2D | grid | 106 |
| Cylon | strip | 123 |
| DBZBattleFinal | grid | 44 |
| Digital Rain 2D | grid | 118 |
| dimbypixel | strip | 123 |
| Dire Spider 2D | grid | 52 |
| distance function kaleidoscope 2 | grid | 64 |
| DNA Helix 2D | grid | 93 |
| Doom Fire | grid | 79 |
| Doom Fire 2D | grid | 89 |
| Drip | strip | 123 |
| Easing Library v1.0 | grid | 122 |
| Edgeburst | strip | 123 |
| Emoji Animation #2 | grid | 55 |
| Example - Button w/ debounce | strip | 123 |
| Example: color hues | strip | 123 |
| Example: modes and waveforms | strip | 123 |
| Example: Smooth Speed Slider | strip | 123 |
| Example: time and animation | strip | 122 |
| Eye of Sauron | grid | 65 |
| Eye of Sauron with movement | grid | 62 |
| Falling Sand 2D | grid | 115 |
| Fast Palette Blending | strip | 123 |
| fast pulse | strip | 123 |
| fast pulse 3d | grid | 123 |
| fire - blue | strip | 121 |
| fire - red | strip | 117 |
| fireblobs | strip | 59 |
| Fireflies | strip | 123 |
| firework dust | strip | 123 |
| firework nova | grid | 74 |
| firework rocket sparks | strip | 123 |
| Fireworks Finale | strip | 122 |
| Flash Posterize + Music Sequencer framework | grid | 87 |
| Flow Field 2D | grid | 116 |
| fractal flower | grid | 37 |
| Frogger 2D | grid | 54 |
| Geometry Morphing Demo 2D | grid | 62 |
| glitch bands | strip | 121 |
| Glitter | strip | 123 |
| Glittering Jewels | strip | 30 |
| GlowFlow (3D coord transform API port) | grid | 111 |
| Golden Tix | strip | 123 |
| Gradient blue  purple pink | strip | 123 |
| green ripple reflections | strip | 120 |
| Grinch's Heist | grid | 89 |
| Halloween color twinkles | strip | 121 |
| Halloween Wavy Bands | grid | 84 |
| heart | grid | 119 |
| heatshivers | strip | 106 |
| Holiday_Diagonal_Stripes | grid | 123 |
| Ice Floes 2D | grid | 11 |
| Icicleblaze | grid | 122 |
| Infinite Snake | grid | 117 |
| Infinity Flower 2D | grid | 114 |
| Interference 2D | grid | 58 |
| Iran - Solidarity | cloud | 48 |
| Kaleidoscope 2D | grid | 58 |
| KITT | strip | 123 |
| KITT (w/ color picker) | strip | 123 |
| Light Organ - 2.0 | strip | 106 |
| Light Organ -- sensor board | strip | 105 |
| Lightbulb - Crank Hue to Complete | strip | 123 |
| Lightning clouds | grid | 88 |
| Lightning Strike | strip | 118 |
| lightning ZAP! | strip | 123 |
| Line Dancer 2D | grid | 97 |
| Lissajous curve tracer | grid | 51 |
| M5Stack Hex panels | cloud | 88 |
| Mandelbrot 2D | grid | 41 |
| Mapping Helper Single and 10x | strip | 122 |
| Marching Dots | strip | 123 |
| marching rainbow | strip | 122 |
| marching rainbow (buffered) | strip | 121 |
| Marquee Chase | strip | 123 |
| Matrix 2 tone pulse | strip | 118 |
| matrix 2D honeycomb | grid | 122 |
| matrix 2D pulse edit | strip | 118 |
| Matrix Green Waterfall 1D | strip | 122 |
| Matrix Green Waterfall 2D | grid | 122 |
| matrix rain | grid | 123 |
| Metaballs of Fire 2D | grid | 55 |
| Meteor Shower | strip | 123 |
| MidpointDisplacement1D | strip | 122 |
| millipede | strip | 123 |
| millipede 1d/2d controls | grid | 123 |
| multimap simpledemo | grid | 93 |
| Multisegment Demo | strip | 117 |
| Music Sequencer - for V3 ONLY | grid | 122 |
| Music Sequencer for v2 | grid | 122 |
| Nano Orbital | strip | 123 |
| NaturalLightSync | strip | 123 |
| neutronorbit | strip | 72 |
| Newfire | strip | 122 |
| novas | strip | 62 |
| Nyan Lights | grid | 122 |
| Oasis | strip | 64 |
| Ocean | strip | 119 |
| opposites | strip | 121 |
| Orv - Christmas Tree | grid | 120 |
| Palette Fire 2D | grid | 120 |
| Pendulum Wave | strip | 123 |
| Performance test framework | strip | 122 |
| Perlin fire | grid | 93 |
| perlin fire wind | grid | 82 |
| perlin fire wind tunnel | grid | 67 |
| Perlin/Simplex Noise 1D | strip | 121 |
| Perlin/Simplex Noise 2D | grid | 43 |
| Pew-Pew-Pew! | strip | 121 |
| pixelClock | strip | 123 |
| Polar mapping helper 2D / 3D | grid | 119 |
| policeLights | strip | 122 |
| portal | strip | 63 |
| Post-Process Chain | strip | 80 |
| Pride Progress | strip | 122 |
| quiet blinkfade | strip | 123 |
| Radar 2D | grid | 42 |
| radiant pulse 3 | grid | 88 |
| Rainbow | strip | 123 |
| Rainbow Comet | strip | 121 |
| Rainbow Flag | strip | 123 |
| rainbow fonts | strip | 123 |
| Rainbow Melt | strip | 123 |
| rainbow pinwheel | strip | 123 |
| Rainbow rocket sparks | strip | 123 |
| Rainbow Smiley | grid | 123 |
| Rainbow v2 | strip | 123 |
| Raindrops 2D | grid | 101 |
| Rainstorm | grid | 65 |
| Reaction Diffusion 2D | grid | 16 |
| Real World Lights | strip | 111 |
| regenbogendrogen | strip | 123 |
| RGB-XYZ 3D Sweep | cloud | 123 |
| RGBclock 2D | grid | 40 |
| RGBW Mapping Tester | strip | 123 |
| RGBW Mapping Tester - HSV Version | strip | 123 |
| Ripples 2D | grid | 60 |
| Rock sparks | grid | 118 |
| Rocket by Tony Hampton | strip | 122 |
| RYB colors | grid | 36 |
| SaberDeploy Tutorial | strip | 123 |
| Scanner | strip | 122 |
| Scary Pumpkin | grid | 74 |
| scrolling text marquee 2D | grid | 122 |
| scrolls | strip | 77 |
| Shimmer Crossfade 2D | grid | 78 |
| Sierpinski Rainbow 2D | grid | 123 |
| Single Color Picker - wide or spot | strip | 122 |
| sinpulse 3D | grid | 122 |
| sinus | grid | 122 |
| Slime mold palette | grid | 3 |
| slow color shift | strip | 123 |
| slowflies | strip | 112 |
| snake | strip | 122 |
| Soap 2D | grid | 111 |
| sound - blinkfade | strip | 121 |
| SOUND - lavablob | grid | 118 |
| sound - rays | strip | 122 |
| sound - rays Frequency-BPM Reactive 1 | strip | 122 |
| sound - spectro kalidastrip | strip | 77 |
| sound - spectroblots - pow fade | grid | 64 |
| sound - spectrokalidamandala | grid | 52 |
| sound - spectromatrix agc | grid | 26 |
| sound - spectromatrix render2D | grid | 61 |
| Sound - Spectrum Analyser | grid | 121 |
| sound - Starburst 2 | strip | 114 |
| Sound & Music Spectrum Visualizer | strip | 121 |
| Sound Reactive Color Fade | grid | 122 |
| sparkfire | strip | 104 |
| sparks | strip | 123 |
| sparks center | strip | 122 |
| spin cycle | strip | 119 |
| Spinning Plasma 2D | grid | 123 |
| Spinwheel 2D | grid | 91 |
| Spiral 2D | grid | 90 |
| spiral twirls star 2D | grid | 56 |
| Spirograph 2D | grid | 122 |
| spotlights / rotation 3D | grid | 80 |
| Spring Colors | strip | 123 |
| Stacker | strip | 121 |
| Stairmaster 2D | grid | 118 |
| Starfield 2D | grid | 121 |
| StarGen polar 2D | grid | 82 |
| Static Christmas Lights - 4 Colors | strip | 122 |
| static random colors | strip | 116 |
| Sun rays through trees | grid | 59 |
| Sunrise | strip | 122 |
| Sunrise 2D | grid | 111 |
| Sunrise Alarm Clock | strip | 49 |
| Sunset | strip | 111 |
| Swirlpool 2D | grid | 122 |
| Synchronized Random Numbers | strip | 21 |
| Tetrix 2D | grid | 122 |
| Three Red Pixels (array) | strip | 123 |
| Thunderstorm | strip | 122 |
| Time Flies 2D | grid | 123 |
| tixy | grid | 91 |
| Traffic | grid | 118 |
| tree setup pattern | grid | 123 |
| Tunnel of Squares 2D | grid | 117 |
| TV Simulator | strip | 122 |
| Twinkle | strip | 122 |
| twinkle (2) | strip | 123 |
| Twinkling Classic Xmas Strands | strip | 63 |
| twinkly stars | strip | 123 |
| TwoColorHSVMix | strip | 121 |
| Typing Heatmap 2D | grid | 121 |
| Unstable Orbits 2D | grid | 121 |
| Upward waves 3D using accelerometer | cloud | 106 |
| US Flag | strip | 113 |
| US Flag 2D | grid | 103 |
| Utility: Palettes | strip | 65 |
| UtilityColorTemp | strip | 123 |
| Voronoi 2D | grid | 47 |
| wanderedges | strip | 119 |
| wanderers | grid | 122 |
| Wavy Bands | grid | 88 |
| White Rainbows | strip | 120 |
| Wichmann–Hill PRNG | strip | 76 |
| XmasFlies | strip | 117 |
| xorcery 2D/3D | grid | 102 |
| zoom kaleidoscope | grid | 74 |

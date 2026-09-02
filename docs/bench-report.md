# Hardware soak + benchmark — 2026-09-02

*Device 192.168.0.183, firmware v0.1.40, 60 px ws2812, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 299 patterns: **299 clean**, 0 with errors, 12 under 30 fps.
- fps at 60 px (the count the sweep ran at): median **118**, p10 52, p90 122.
- lowest heap_free seen while soaking: 83448 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 122 |
| 150 | 99 |
| 300 | 52 |
| 600 | 27 |
| 1024 | 16 |
| 2048 | 8 |

## Slowest (< 30 fps at 60 px)

| pattern | kind | fps |
|---|---|---:|
| Slime mold palette | grid | 3 |
| Bouncy Boxes | grid | 10 |
| fractal flower | grid | 11 |
| Ice Floes 2D | grid | 11 |
| Crosstown Traffic 2D | grid | 14 |
| Dire Spider 2D | grid | 16 |
| Reaction Diffusion 2D | grid | 16 |
| Crawling Spider 2D | grid | 21 |
| Synchronized Random Numbers | strip | 21 |
| Butterfly 2D | grid | 25 |
| sound - spectromatrix agc | grid | 25 |
| Glittering Jewels | strip | 28 |

## All results

| pattern | kind | fps |
|---|---|---:|
| _Fairies | strip | 122 |
| 1 White Fade | strip | 123 |
| 1D Aurora Borealis | strip | 56 |
| 2 Colors | strip | 122 |
| 2 Purple Fade | strip | 123 |
| 2D Bouncing Additive Primaries | grid | 83 |
| 2D canvas example | grid | 123 |
| 2d Clock with Hand Color Pickers | grid | 74 |
| 2D Fireworks Fade | grid | 121 |
| 2D sinc(theta)/theta | grid | 57 |
| 2D Spiral Twirls | grid | 98 |
| 2D Wandering Fireball | grid | 120 |
| 3 color rotation | strip | 122 |
| 3D Rotation / Spotlights | cloud | 98 |
| 4th | strip | 104 |
| 80s kid show | grid | 41 |
| A Peak Integrator | strip | 123 |
| Accelerometer level example | grid | 122 |
| All Lasers Fire | grid | 54 |
| amoeba | strip | 96 |
| angle and radius from coordinates | grid | 88 |
| Angry Xmass 3D | cloud | 120 |
| Animated Asterisks 2D | grid | 74 |
| Audio Volume Meter | strip | 114 |
| Aurora 2D | grid | 89 |
| aurorashivers | strip | 85 |
| Autumn Colors | strip | 122 |
| b_lightning_flashes | strip | 122 |
| Beat Bounce | strip | 61 |
| Bessel Chaos | strip | 120 |
| Blink Fade | strip | 122 |
| Blinky Eyes 2D | grid | 109 |
| block reflections | strip | 119 |
| Blue Holiday Candle 2D | grid | 47 |
| Blue Holiday Star 2D | grid | 60 |
| Boids 2D | grid | 114 |
| Bouncer3D | grid | 94 |
| bouncing balls - hsv | strip | 121 |
| bouncing balls - rgb | strip | 121 |
| Bouncing Balls 2D | grid | 65 |
| Bouncing Balls RGB 2D | grid | 63 |
| Bouncing RGB Balls - 2D | grid | 109 |
| Bouncy Boxes | grid | 10 |
| Breakout 2D | grid | 121 |
| Breathing Gradient | strip | 69 |
| Bubble Column | strip | 55 |
| bustle | strip | 116 |
| Butterfly 2D | grid | 25 |
| Carrie's Holiday Star 2D | grid | 70 |
| Cellular Automata 1D | strip | 122 |
| Chasing Rainbows & HSLuv | strip | 122 |
| Chevron 2D | grid | 123 |
| chill confetti | strip | 122 |
| Christmas Candy Cane | strip | 122 |
| Christmas Lights | strip | 122 |
| Christmas RG Fade | strip | 122 |
| ChristmasLights | strip | 122 |
| ChristmasPewPew | strip | 122 |
| ChristmasStretch | strip | 122 |
| color bands | strip | 118 |
| color bands (buffered) | strip | 105 |
| Color Blend | strip | 122 |
| color fade pulse | strip | 121 |
| Color Pick Fade | strip | 121 |
| color twinkle bounce | strip | 122 |
| Color Twinkles | strip | 120 |
| colourful fireflies | strip | 122 |
| Comets | strip | 122 |
| Complements 3D | grid | 120 |
| Continuous Cellular Automata | grid | 96 |
| coolaura | strip | 91 |
| Coral Plasma | grid | 64 |
| Coronal Ejection 2D | grid | 50 |
| Coronal Mass Ejection | grid | 52 |
| Crawling Spider 2D | grid | 21 |
| Crossfading | strip | 108 |
| Crosshair Pulse 2D | grid | 35 |
| Crosstown Traffic 2D | grid | 14 |
| cube fire 3D | grid | 116 |
| Curl Flow 2D | grid | 96 |
| Custom Sequences | strip | 104 |
| Cyclic Cellular Automata 2D | grid | 104 |
| Cylon | strip | 122 |
| DBZBattleFinal | grid | 48 |
| Digital Rain 2D | grid | 118 |
| Dire Spider 2D | grid | 16 |
| distance function kaleidoscope 2 | grid | 47 |
| DNA Helix 2D | grid | 90 |
| Doom Fire | grid | 62 |
| Doom Fire 2D | grid | 81 |
| Drip | strip | 122 |
| Easing Library v1.0 | grid | 122 |
| Edgeburst | strip | 122 |
| Ember Diffusion | strip | 121 |
| Emoji Animation #2 | grid | 66 |
| Example - Button w/ debounce | strip | 122 |
| Example: color hues | strip | 121 |
| Example: modes and waveforms | strip | 121 |
| Example: Smooth Speed Slider | strip | 122 |
| Example: time and animation | strip | 121 |
| Eye of Sauron | grid | 60 |
| Eye of Sauron with movement | grid | 54 |
| Falling Sand 2D | grid | 110 |
| Fast Palette Blending | strip | 122 |
| fast pulse | strip | 121 |
| fast pulse 3d | grid | 122 |
| fire - blue | strip | 121 |
| fire - red | strip | 120 |
| fireblobs | strip | 60 |
| Fireflies | strip | 122 |
| firework dust | strip | 122 |
| firework nova | grid | 101 |
| firework rocket sparks | strip | 121 |
| Fireworks Finale | strip | 121 |
| Flash Posterize + Music Sequencer framework | grid | 89 |
| Flow Field 2D | grid | 117 |
| fractal flower | grid | 11 |
| Frogger 2D | grid | 75 |
| Geometry Morphing Demo 2D | grid | 55 |
| glitch bands | strip | 117 |
| Glitter | strip | 122 |
| Glittering Jewels | strip | 28 |
| GlowFlow (3D coord transform API port) | grid | 103 |
| Golden Tix | strip | 122 |
| Gradient blue  purple pink | strip | 123 |
| green ripple reflections | strip | 119 |
| Grinch's Heist | grid | 88 |
| Halloween color twinkles | strip | 119 |
| Halloween Wavy Bands | grid | 79 |
| heart | grid | 118 |
| heatshivers | strip | 109 |
| Holiday_Diagonal_Stripes | grid | 122 |
| Ice Floes 2D | grid | 11 |
| Infinite Snake | grid | 116 |
| Infinity Flower 2D | grid | 118 |
| Interference 2D | grid | 87 |
| Kaleidoscope 2D | grid | 44 |
| KITT | strip | 123 |
| KITT (w/ color picker) | strip | 121 |
| Light Organ - 2.0 | strip | 92 |
| Light Organ -- sensor board | strip | 98 |
| Lightbulb - Crank Hue to Complete | strip | 122 |
| Lightning clouds | grid | 82 |
| Lightning Strike | strip | 119 |
| Line Dancer 2D | grid | 116 |
| Lissajous curve tracer | grid | 43 |
| M5Stack Hex panels | cloud | 104 |
| Main Stage | grid | 122 |
| Mandelbrot 2D | grid | 64 |
| Mapping Helper Single and 10x | strip | 123 |
| Marching Dots | strip | 123 |
| marching rainbow | strip | 120 |
| marching rainbow (buffered) | strip | 120 |
| Marquee Chase | strip | 122 |
| Matrix 2 tone pulse | strip | 114 |
| matrix 2D honeycomb | grid | 111 |
| matrix 2D pulse edit | strip | 119 |
| Matrix Green Waterfall 1D | strip | 121 |
| Matrix Green Waterfall 2D | grid | 121 |
| matrix rain | grid | 121 |
| Metaballs of Fire 2D | grid | 65 |
| Meteor Shower | strip | 122 |
| MidpointDisplacement1D | strip | 121 |
| millipede | strip | 122 |
| millipede 1d/2d controls | grid | 59 |
| multimap simpledemo | grid | 91 |
| Multisegment Demo | strip | 110 |
| Nano Orbital | strip | 122 |
| NaturalLightSync | strip | 122 |
| neutronorbit | strip | 58 |
| Newfire | strip | 119 |
| novas | strip | 60 |
| Nyan Lights | grid | 121 |
| Oasis | strip | 63 |
| Ocean | strip | 101 |
| Opening Act | grid | 120 |
| opposites | strip | 114 |
| Orv - Christmas Tree | grid | 49 |
| Palette Fire 2D | grid | 91 |
| Pendulum Wave | strip | 123 |
| Performance test framework | strip | 121 |
| Perlin fire | grid | 45 |
| perlin fire wind | grid | 43 |
| perlin fire wind tunnel | grid | 71 |
| Perlin/Simplex Noise 1D | strip | 120 |
| Perlin/Simplex Noise 2D | grid | 41 |
| Pew-Pew-Pew! | strip | 122 |
| pixelClock | strip | 122 |
| Polar mapping helper 2D / 3D | grid | 107 |
| policeLights | strip | 122 |
| portal | strip | 66 |
| Post-Process Chain | strip | 81 |
| quiet blinkfade | strip | 122 |
| Radar 2D | grid | 44 |
| radiant pulse 3 | grid | 106 |
| Rainbow | strip | 123 |
| Rainbow Comet | strip | 122 |
| Rainbow Flag | strip | 122 |
| rainbow fonts | strip | 122 |
| Rainbow Melt | strip | 121 |
| rainbow pinwheel | strip | 122 |
| Rainbow rocket sparks | strip | 120 |
| Rainbow Smiley | grid | 123 |
| Rainbow v2 | strip | 123 |
| Raindrops 2D | grid | 41 |
| Rainstorm | grid | 73 |
| Reaction Diffusion 2D | grid | 16 |
| Real World Lights | strip | 96 |
| regenbogendrogen | strip | 122 |
| RGB-XYZ 3D Sweep | cloud | 122 |
| RGBclock 2D | grid | 43 |
| RGBW Mapping Tester | strip | 123 |
| RGBW Mapping Tester - HSV Version | strip | 123 |
| Ripples 2D | grid | 68 |
| Rock sparks | grid | 119 |
| Rocket by Tony Hampton | strip | 121 |
| RYB colors | grid | 37 |
| SaberDeploy Tutorial | strip | 123 |
| Scanner | strip | 120 |
| Scary Pumpkin | grid | 73 |
| scrolling text marquee 2D | grid | 122 |
| scrolls | strip | 86 |
| Shimmer Crossfade 2D | grid | 97 |
| Sierpinski Rainbow 2D | grid | 119 |
| Single Color Picker - wide or spot | strip | 122 |
| sinpulse 3D | grid | 117 |
| sinus | grid | 121 |
| Slime mold palette | grid | 3 |
| slow color shift | strip | 122 |
| slowflies | strip | 113 |
| snake | strip | 122 |
| Soap 2D | grid | 118 |
| sound - blinkfade | strip | 119 |
| SOUND - lavablob | grid | 112 |
| sound - rays | strip | 123 |
| sound - rays Frequency-BPM Reactive 1 | strip | 121 |
| sound - spectro kalidastrip | strip | 80 |
| sound - spectroblots - pow fade | grid | 59 |
| sound - spectrokalidamandala | grid | 65 |
| sound - spectromatrix agc | grid | 25 |
| sound - spectromatrix render2D | grid | 59 |
| Sound - Spectrum Analyser | grid | 121 |
| sound - Starburst 2 | strip | 120 |
| Sound & Music Spectrum Visualizer | strip | 121 |
| Sound Reactive Color Fade | grid | 123 |
| sparkfire | strip | 91 |
| sparks | strip | 121 |
| sparks center | strip | 121 |
| spin cycle | strip | 106 |
| Spinning Plasma 2D | grid | 121 |
| Spinwheel 2D | grid | 112 |
| Spiral 2D | grid | 118 |
| spiral twirls star 2D | grid | 58 |
| Spirograph 2D | grid | 122 |
| spotlights / rotation 3D | grid | 87 |
| Spring Colors | strip | 122 |
| Stacker | strip | 121 |
| Stairmaster 2D | grid | 119 |
| Starfield 2D | grid | 121 |
| StarGen polar 2D | grid | 78 |
| Static Christmas Lights - 4 Colors | strip | 123 |
| static random colors | strip | 101 |
| Sun rays through trees | grid | 74 |
| Sunrise | strip | 122 |
| Sunrise 2D | grid | 108 |
| Sunrise Alarm Clock | strip | 65 |
| Sunset | strip | 112 |
| Swirlpool 2D | grid | 122 |
| Synchronized Random Numbers | strip | 21 |
| Tetrix 2D | grid | 121 |
| Three Red Pixels (array) | strip | 122 |
| Thunderstorm | strip | 121 |
| Time Flies 2D | grid | 123 |
| tixy | grid | 79 |
| Traffic | grid | 114 |
| tree setup pattern | grid | 121 |
| Tunnel of Squares 2D | grid | 101 |
| TV Simulator | strip | 122 |
| Twinkle | strip | 121 |
| twinkle (2) | strip | 122 |
| Twinkling Classic Xmas Strands | strip | 74 |
| twinkly stars | strip | 122 |
| TwoColorHSVMix | strip | 98 |
| Typing Heatmap 2D | grid | 117 |
| Unstable Orbits 2D | grid | 121 |
| Upward waves 3D using accelerometer | cloud | 119 |
| US Flag | strip | 102 |
| US Flag 2D | grid | 96 |
| Utility: Palettes | strip | 57 |
| UtilityColorTemp | strip | 121 |
| Voronoi 2D | grid | 54 |
| wanderedges | strip | 117 |
| wanderers | grid | 92 |
| Wavy Bands | grid | 89 |
| White Rainbows | strip | 122 |
| Wichmann–Hill PRNG | strip | 51 |
| XmasFlies | strip | 115 |
| xorcery 2D/3D | grid | 77 |
| zoom kaleidoscope | grid | 84 |

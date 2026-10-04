<!-- The Seengreat 2x2 (16384 px) full-library soak, 2026-10-04 — the third of
three runs that night, on the image that merged (dcache.rs write-back fix +
the live upload in PSRAM). The runs, same device, same 308 patterns,
brightness 4 (the board resets on panel current above ~10 with a bright
pattern — Gitea #931):

| run | image | clean | reboots | panics on serial | lowest heap_free |
|---|---|---:|---:|---|---:|
| 1 | master dfb60162 | 297 | 7 | 5× `Illegal`, 1× `InstrProhibited PC 0` in fresh JIT code, 1 unread | 4,996 B |
| 2 | + dcache.rs (S3 write-back erratum) | 300 | 4 | 2× `memory allocation failed` (272 B, 4096 B), 1× `StoreProhibited`, 1× `Illegal` | 5,860 B |
| 3 | + live upload/program out of internal DRAM | 303 | 1 | 1× `LoadStoreError` | 44,920 B |

The four `array index out of bounds` rows below are 16.16 wraparounds in
the patterns themselves (Gitea #934), fixed in the same PR and replayed
clean on the device afterwards; this run pushed the pre-fix gallery. The one
reboot followed a row whose output stage had stopped taking frames (`pipe 0
out 0`, 13 hand-off drops, ring late/torn/dma_restarts in the thousands) —
the residual is Gitea #936. -->

# Hardware soak + benchmark — 2026-10-04

*Device 192.168.0.238, firmware v0.1.40, 16384 px ws2812, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 308 patterns: **303 clean**, 5 with errors, 288 under 30 fps.
- fps at 16384 px (the count the sweep ran at): median **12**, p10 4, p90 26.
- lowest heap_free seen while soaking: 44920 bytes.
- **1 device crash** (0 unreachable after a push, **1 reboot** caught by the fences/slot oracle).
  - rebooted: at "perlin fire wind" (since "Perlin fire"; reset Some(CoreSw), slot ota_0).

## fps vs pixel count (rainbow reference)

skipped (--no-curve).

## Errors

| pattern | kind | problem |
|---|---|---|
| Audio Volume Meter | strip | array index out of bounds |
| Drip | strip | array index out of bounds |
| perlin fire wind | grid | rebooted (reset Some(CoreSw), slot ota_0) |
| Sound & Music Spectrum Visualizer | strip | array index out of bounds |
| XmasFlies | strip | array index out of bounds |

## Slowest (< 30 fps at 16384 px)

| pattern | kind | fps |
|---|---|---:|
| Dire Spider 2D | grid | 1 |
| 2D sinc(theta)/theta | grid | 2 |
| All Lasers Fire | grid | 2 |
| Beat Bounce | strip | 2 |
| Blue Holiday Star 2D | grid | 2 |
| Breathing Gradient | strip | 2 |
| Butterfly 2D | grid | 2 |
| Crawling Spider 2D | grid | 2 |
| Crosstown Traffic 2D | grid | 2 |
| distance function kaleidoscope 2 | grid | 2 |
| Eye of Sauron | grid | 2 |
| Eye of Sauron with movement | grid | 2 |
| Radar 2D | grid | 2 |
| Carrie's Holiday Star 2D | grid | 3 |
| Coral Plasma | grid | 3 |
| Coronal Ejection 2D | grid | 3 |
| Coronal Mass Ejection | grid | 3 |
| fire - blue | strip | 3 |
| Glittering Jewels | strip | 3 |
| Interference 2D | grid | 3 |
| Kaleidoscope 2D | grid | 3 |
| millipede 1d/2d controls | grid | 3 |
| RGBclock 2D | grid | 3 |
| Scary Pumpkin | grid | 3 |
| Sun rays through trees | grid | 3 |
| Sunrise Alarm Clock | strip | 3 |
| zoom kaleidoscope | grid | 3 |
| 2d Clock with Hand Color Pickers | grid | 4 |
| Crosshair Pulse 2D | grid | 4 |
| fire - red | strip | 4 |
| fireblobs | strip | 4 |
| Metaballs of Fire 2D | grid | 4 |
| novas | strip | 4 |
| perlin fire wind tunnel | grid | 4 |
| Ripples 2D | grid | 4 |
| sound - spectroblots - pow fade | grid | 4 |
| Synchronized Random Numbers | strip | 4 |
| Voronoi 2D | grid | 4 |
| 1D Aurora Borealis | strip | 5 |
| 2D Bouncing Additive Primaries | grid | 5 |
| 2D Fireworks Fade | grid | 5 |
| 80s kid show | grid | 5 |
| Blue Holiday Candle 2D | grid | 5 |
| Bouncing RGB Balls - 2D | grid | 5 |
| Bubble Column | strip | 5 |
| DBZBattleFinal | grid | 5 |
| Halloween Wavy Bands | grid | 5 |
| Lightning clouds | grid | 5 |
| Palette Fire 2D | grid | 5 |
| Rainstorm | grid | 5 |
| RYB colors | grid | 5 |
| Soap 2D | grid | 5 |
| sound - spectrokalidamandala | grid | 5 |
| Wavy Bands | grid | 5 |
| angle and radius from coordinates | grid | 6 |
| Curl Flow 2D | grid | 6 |
| Easing Library v1.0 | grid | 6 |
| Emoji Animation #2 | grid | 6 |
| Line Dancer 2D | grid | 6 |
| Newfire | strip | 6 |
| radiant pulse 3 | grid | 6 |
| Real World Lights | strip | 6 |
| StarGen polar 2D | grid | 6 |
| Tunnel of Squares 2D | grid | 6 |
| 2D canvas example | grid | 7 |
| 2D Spiral Twirls | grid | 7 |
| aurorashivers | strip | 7 |
| Blinky Eyes 2D | grid | 7 |
| Bouncing Balls RGB 2D | grid | 7 |
| Crossfading | strip | 7 |
| DNA Helix 2D | grid | 7 |
| Ember Diffusion | strip | 7 |
| Geometry Morphing Demo 2D | grid | 7 |
| glitch bands | strip | 7 |
| heatshivers | strip | 7 |
| M5Stack Hex panels | cloud | 7 |
| Oasis | strip | 7 |
| portal | strip | 7 |
| Post-Process Chain | strip | 7 |
| scrolls | strip | 7 |
| Spinwheel 2D | grid | 7 |
| Spiral 2D | grid | 7 |
| spiral twirls star 2D | grid | 7 |
| Wichmann–Hill PRNG | strip | 7 |
| 4th | any | 8 |
| bustle | strip | 8 |
| ChristmasPewPew | strip | 8 |
| color bands (buffered) | any | 8 |
| Color Twinkles | strip | 8 |
| Flash Posterize + Music Sequencer framework | grid | 8 |
| Halloween color twinkles | strip | 8 |
| neutronorbit | strip | 8 |
| Orv - Christmas Tree | grid | 8 |
| Pew-Pew-Pew! | any | 8 |
| sinpulse 3D | grid | 8 |
| sound - spectromatrix render2D | grid | 8 |
| Spinning Plasma 2D | grid | 8 |
| Time Flies 2D | grid | 8 |
| Twinkling Classic Xmas Strands | strip | 8 |
| Utility: Palettes | strip | 8 |
| amoeba | strip | 9 |
| coolaura | strip | 9 |
| cube fire 3D | grid | 9 |
| fast pulse 3d | grid | 9 |
| firework nova | grid | 9 |
| green ripple reflections | strip | 9 |
| Infinity Flower 2D | grid | 9 |
| Lightning Strike | strip | 9 |
| Matrix 2 tone pulse | strip | 9 |
| Shimmer Crossfade 2D | grid | 9 |
| slowflies | strip | 9 |
| sound - spectro kalidastrip | strip | 9 |
| spotlights / rotation 3D | grid | 9 |
| Spring Colors | strip | 9 |
| Sunset | strip | 9 |
| tree setup pattern | grid | 9 |
| Typing Heatmap 2D | grid | 9 |
| US Flag | strip | 9 |
| US Flag 2D | grid | 9 |
| xorcery 2D/3D | grid | 9 |
| _Fairies | any | 10 |
| 3D Rotation / Spotlights | cloud | 10 |
| Blink Fade | strip | 10 |
| Cellular Automata 1D | strip | 10 |
| Chasing Rainbows & HSLuv | strip | 10 |
| Continuous Cellular Automata | grid | 10 |
| Frogger 2D | grid | 10 |
| GlowFlow (3D coord transform API port) | grid | 10 |
| matrix 2D honeycomb | grid | 10 |
| matrix 2D pulse edit | strip | 10 |
| sound - blinkfade | strip | 10 |
| SOUND - lavablob | grid | 10 |
| sound - Starburst 2 | strip | 10 |
| sparkfire | strip | 10 |
| Stairmaster 2D | grid | 10 |
| tixy | grid | 10 |
| Traffic | grid | 10 |
| Upward waves 3D using accelerometer | cloud | 10 |
| wanderedges | strip | 10 |
| Animated Asterisks 2D | grid | 11 |
| Aurora 2D | grid | 11 |
| Bessel Chaos | strip | 11 |
| color bands | strip | 11 |
| Digital Rain 2D | grid | 11 |
| heart | grid | 11 |
| Mandelbrot 2D | grid | 11 |
| marching rainbow (buffered) | any | 11 |
| multimap simpledemo | grid | 11 |
| Multisegment Demo | strip | 11 |
| Single Color Picker - wide or spot | strip | 11 |
| Bouncer3D | grid | 12 |
| Breakout 2D | grid | 12 |
| color twinkle bounce | strip | 12 |
| colourful fireflies | strip | 12 |
| Comets | strip | 12 |
| marching rainbow | strip | 12 |
| Ocean | strip | 12 |
| Perlin/Simplex Noise 2D | grid | 12 |
| Polar mapping helper 2D / 3D | grid | 12 |
| slow color shift | strip | 12 |
| Autumn Colors | strip | 13 |
| bouncing balls - rgb | any | 13 |
| Christmas RG Fade | strip | 13 |
| Complements 3D | grid | 13 |
| Doom Fire | grid | 13 |
| firework dust | strip | 13 |
| Fireworks Finale | any | 13 |
| Glitter | strip | 13 |
| Light Organ -- sensor board | strip | 13 |
| Rocket by Tony Hampton | any | 13 |
| Sierpinski Rainbow 2D | grid | 13 |
| sinus | grid | 13 |
| White Rainbows | strip | 13 |
| bouncing balls - hsv | strip | 14 |
| color fade pulse | strip | 14 |
| Custom Sequences | strip | 14 |
| Grinch's Heist | grid | 14 |
| KITT (w/ color picker) | strip | 14 |
| opposites | strip | 14 |
| Rainbow Comet | any | 14 |
| Rainbow Melt | strip | 14 |
| Scanner | strip | 14 |
| spin cycle | strip | 14 |
| Tetrix 2D | grid | 14 |
| Cyclic Cellular Automata 2D | grid | 15 |
| Cylon | strip | 15 |
| Doom Fire 2D | grid | 15 |
| Golden Tix | strip | 15 |
| Infinite Snake | grid | 15 |
| Light Organ - 2.0 | strip | 15 |
| Meteor Shower | strip | 15 |
| Opening Act | grid | 15 |
| Perlin/Simplex Noise 1D | strip | 15 |
| quiet blinkfade | strip | 15 |
| rainbow fonts | strip | 15 |
| 2D Wandering Fireball | grid | 16 |
| Boids 2D | grid | 16 |
| Color Blend | strip | 16 |
| Fireflies | strip | 16 |
| KITT | strip | 16 |
| Lissajous curve tracer | grid | 16 |
| millipede | strip | 16 |
| Nano Orbital | strip | 16 |
| Pendulum Wave | strip | 16 |
| Reaction Diffusion 2D | grid | 16 |
| Rock sparks | grid | 16 |
| Slime mold palette | grid | 16 |
| sound - rays | strip | 16 |
| sound - rays Frequency-BPM Reactive 1 | strip | 16 |
| Sound - Spectrum Analyser | grid | 16 |
| sparks center | strip | 16 |
| static random colors | strip | 16 |
| Sunrise 2D | grid | 16 |
| TwoColorHSVMix | strip | 16 |
| Unstable Orbits 2D | grid | 16 |
| Angry Xmass 3D | cloud | 17 |
| Example: time and animation | strip | 17 |
| Falling Sand 2D | grid | 17 |
| firework rocket sparks | strip | 17 |
| Main Stage | grid | 17 |
| Matrix Green Waterfall 1D | strip | 17 |
| Matrix Green Waterfall 2D | grid | 17 |
| MidpointDisplacement1D | strip | 17 |
| scrolling text marquee 2D | grid | 17 |
| sparks | strip | 17 |
| Spirograph 2D | grid | 17 |
| Starfield 2D | grid | 17 |
| Three Red Pixels (array) | strip | 17 |
| Twinkle | strip | 17 |
| Accelerometer level example | grid | 18 |
| block reflections | strip | 18 |
| Bouncing Balls 2D | grid | 18 |
| Example: modes and waveforms | strip | 18 |
| Holiday_Diagonal_Stripes | grid | 18 |
| Rainbow Smiley | grid | 18 |
| regenbogendrogen | strip | 18 |
| twinkle (2) | any | 18 |
| twinkly stars | strip | 18 |
| 2 Colors | strip | 19 |
| Chevron 2D | grid | 19 |
| Christmas Lights | strip | 19 |
| fast pulse | strip | 19 |
| matrix rain | grid | 19 |
| Rainbow Flag | strip | 19 |
| RGB-XYZ 3D Sweep | cloud | 19 |
| wanderers | grid | 19 |
| Example: color hues | strip | 20 |
| Fast Palette Blending | strip | 20 |
| rainbow pinwheel | strip | 20 |
| Rainbow rocket sparks | strip | 20 |
| Color Pick Fade | strip | 21 |
| Gradient blue  purple pink | strip | 21 |
| Lightbulb - Crank Hue to Complete | strip | 21 |
| snake | strip | 21 |
| Sound Reactive Color Fade | grid | 21 |
| Thunderstorm | strip | 21 |
| chill confetti | any | 22 |
| ChristmasLights | strip | 22 |
| ChristmasStretch | strip | 22 |
| Edgeburst | strip | 22 |
| pixelClock | strip | 22 |
| Static Christmas Lights - 4 Colors | strip | 22 |
| Sunrise | strip | 22 |
| 3 color rotation | strip | 23 |
| Christmas Candy Cane | strip | 23 |
| Marquee Chase | strip | 23 |
| Performance test framework | strip | 23 |
| Perlin fire | grid | 23 |
| Stacker | strip | 23 |
| TV Simulator | strip | 23 |
| policeLights | strip | 24 |
| Rainbow | strip | 25 |
| Bulk Bouncing Balls 2D | grid | 26 |
| Example - Button w/ debounce | strip | 26 |
| Example: Smooth Speed Slider | strip | 26 |
| Mapping Helper Single and 10x | strip | 26 |
| Marching Dots | strip | 26 |
| Rainbow v2 | strip | 26 |
| RGBW Mapping Tester - HSV Version | strip | 26 |
| SaberDeploy Tutorial | strip | 27 |
| 1 White Fade | strip | 28 |
| 2 Purple Fade | strip | 28 |
| b_lightning_flashes | strip | 29 |
| Bouncy Boxes | grid | 29 |
| Bulk Canvas Ripples 2D | grid | 29 |
| fractal flower | grid | 29 |
| Raindrops 2D | grid | 29 |
| RGBW Mapping Tester | strip | 29 |

## All results

| pattern | kind | fps | heap largest | jit | drops | frame µs | vm µs | pipe µs | out µs |
|---|---|---:|---:|---|---|---:|---:|---:|---:|
| _Fairies | any | 10 | 49076 | native | 0/0/0 (late 31 torn 57 dma_rs 364 lcd_rs 0 dma_err 0) | 107135 | 101240 | 67 | 34833 |
| 1 White Fade | strip | 28 | 50152 | native | 0/0/0 (late 57 torn 71 dma_rs 385 lcd_rs 0 dma_err 0) | 35545 | 29908 | 43 | 23172 |
| 1D Aurora Borealis | strip | 5 | 48436 | native | 0/0/0 (late 76 torn 99 dma_rs 411 lcd_rs 0 dma_err 0) | 233481 | 229296 | 109 | 23817 |
| 2 Colors | strip | 19 | 49608 | native | 0/0/0 (late 100 torn 117 dma_rs 432 lcd_rs 0 dma_err 0) | 53337 | 48808 | 44 | 22586 |
| 2 Purple Fade | strip | 28 | 50108 | native | 0/0/0 (late 117 torn 138 dma_rs 451 lcd_rs 0 dma_err 0) | 36217 | 30596 | 47 | 23347 |
| 2D Bouncing Additive Primaries | grid | 5 | 48344 | native | 0/0/0 (late 153 torn 169 dma_rs 476 lcd_rs 0 dma_err 0) | 224202 | 220105 | 78 | 23181 |
| 2D canvas example | grid | 7 | 50020 | native | 0/0/0 (late 172 torn 196 dma_rs 510 lcd_rs 0 dma_err 0) | 160834 | 156250 | 87 | 23464 |
| 2d Clock with Hand Color Pickers | grid | 4 | 45496 | native | 0/0/0 (late 235 torn 244 dma_rs 534 lcd_rs 0 dma_err 0) | 324599 | 320034 | 117 | 25007 |
| 2D Fireworks Fade | grid | 5 | 45968 | native | 0/0/0 (late 302 torn 331 dma_rs 564 lcd_rs 0 dma_err 0) | 215192 | 210561 | 91 | 25821 |
| 2D sinc(theta)/theta | grid | 2 | 48688 | native | 0/0/0 (late 325 torn 360 dma_rs 589 lcd_rs 0 dma_err 0) | 594410 | 590063 | 108 | 24355 |
| 2D Spiral Twirls | grid | 7 | 48756 | native | 0/0/0 (late 337 torn 378 dma_rs 614 lcd_rs 0 dma_err 0) | 148174 | 144104 | 81 | 23791 |
| 2D Wandering Fireball | grid | 16 | 49016 | native | 0/0/0 (late 358 torn 411 dma_rs 639 lcd_rs 0 dma_err 0) | 65606 | 61257 | 62 | 22514 |
| 3 color rotation | strip | 23 | 49512 | native | 0/0/0 (late 382 torn 426 dma_rs 659 lcd_rs 0 dma_err 0) | 44261 | 39477 | 44 | 22863 |
| 3D Rotation / Spotlights | cloud | 10 | 49696 | native | 0/0/0 (late 413 torn 437 dma_rs 679 lcd_rs 0 dma_err 0) | 99706 | 94695 | 86 | 23177 |
| 4th | any | 8 | 47708 | native | 0/0/0 (late 438 torn 494 dma_rs 703 lcd_rs 0 dma_err 0) | 124359 | 118787 | 66 | 44359 |
| 80s kid show | grid | 5 | 46804 | native | 0/0/0 (late 476 torn 553 dma_rs 728 lcd_rs 0 dma_err 0) | 244520 | 240346 | 110 | 24039 |
| A Peak Integrator | strip | 30 | 48320 | native | 0/0/0 (late 520 torn 608 dma_rs 755 lcd_rs 0 dma_err 0) | 33234 | 27713 | 46 | 23935 |
| Accelerometer level example | grid | 18 | 49812 | native | 1/0/0 (late 553 torn 623 dma_rs 778 lcd_rs 0 dma_err 0) | 55817 | 51418 | 67 | 23397 |
| All Lasers Fire | grid | 2 | 49236 | native | 1/0/0 (late 575 torn 638 dma_rs 808 lcd_rs 0 dma_err 0) | 518959 | 515030 | 116 | 23977 |
| amoeba | strip | 9 | 48484 | native | 1/0/0 (late 595 torn 677 dma_rs 845 lcd_rs 0 dma_err 0) | 110147 | 104352 | 98 | 37249 |
| angle and radius from coordinates | grid | 6 | 48920 | native | 1/0/0 (late 604 torn 698 dma_rs 868 lcd_rs 0 dma_err 0) | 174010 | 169963 | 80 | 23549 |
| Angry Xmass 3D | cloud | 17 | 50032 | native | 1/0/0 (late 616 torn 710 dma_rs 891 lcd_rs 0 dma_err 0) | 59499 | 53217 | 57 | 22692 |
| Animated Asterisks 2D | grid | 11 | 48956 | native | 1/0/0 (late 650 torn 728 dma_rs 917 lcd_rs 0 dma_err 0) | 91575 | 87395 | 68 | 23213 |
| Audio Volume Meter | strip | 11 | 46164 | native | 1/0/0 (late 671 torn 795 dma_rs 939 lcd_rs 0 dma_err 0) | 92296 | 86354 | 121 | 27162 |
| Aurora 2D | grid | 11 | 48736 | native | 1/0/0 (late 725 torn 861 dma_rs 963 lcd_rs 0 dma_err 0) | 92618 | 87932 | 64 | 24129 |
| aurorashivers | strip | 7 | 49068 | native | 1/0/0 (late 735 torn 897 dma_rs 998 lcd_rs 0 dma_err 0) | 143919 | 137996 | 82 | 29680 |
| Autumn Colors | strip | 13 | 48828 | native | 1/0/0 (late 758 torn 930 dma_rs 1027 lcd_rs 0 dma_err 0) | 81805 | 76384 | 61 | 27529 |
| b_lightning_flashes | strip | 29 | 49060 | native | 1/0/0 (late 767 torn 965 dma_rs 1047 lcd_rs 0 dma_err 0) | 34238 | 28679 | 41 | 23471 |
| Beat Bounce | strip | 2 | 49324 | native | 2/0/0 (late 789 torn 987 dma_rs 1083 lcd_rs 0 dma_err 0) | 587555 | 583507 | 104 | 28314 |
| Bessel Chaos | strip | 11 | 49528 | native | 2/0/0 (late 815 torn 1010 dma_rs 1115 lcd_rs 0 dma_err 0) | 98761 | 94462 | 86 | 23209 |
| Blink Fade | strip | 10 | 48980 | native | 2/0/0 (late 823 torn 1024 dma_rs 1143 lcd_rs 0 dma_err 0) | 106496 | 100843 | 75 | 26568 |
| Blinky Eyes 2D | grid | 7 | 48428 | native | 2/0/0 (late 844 torn 1069 dma_rs 1157 lcd_rs 0 dma_err 0) | 148899 | 144077 | 81 | 23244 |
| block reflections | strip | 18 | 49096 | native | 2/0/0 (late 866 torn 1079 dma_rs 1187 lcd_rs 0 dma_err 0) | 57114 | 50713 | 56 | 23187 |
| Blue Holiday Candle 2D | grid | 5 | 49028 | native | 2/0/0 (late 893 torn 1125 dma_rs 1210 lcd_rs 0 dma_err 0) | 217796 | 213730 | 125 | 23874 |
| Blue Holiday Star 2D | grid | 2 | 49380 | native | 2/0/0 (late 908 torn 1170 dma_rs 1238 lcd_rs 0 dma_err 0) | 524242 | 520306 | 112 | 23620 |
| Boids 2D | grid | 16 | 49280 | native | 2/0/0 (late 925 torn 1193 dma_rs 1275 lcd_rs 0 dma_err 0) | 62247 | 57503 | 93 | 24620 |
| Bouncer3D | grid | 12 | 48788 | native | 2/0/0 (late 958 torn 1218 dma_rs 1298 lcd_rs 0 dma_err 0) | 89434 | 85140 | 67 | 22984 |
| bouncing balls - hsv | strip | 14 | 49540 | native | 2/0/0 (late 974 torn 1254 dma_rs 1320 lcd_rs 0 dma_err 0) | 71580 | 65911 | 70 | 33719 |
| bouncing balls - rgb | any | 13 | 48244 | native | 2/0/0 (late 997 torn 1295 dma_rs 1345 lcd_rs 0 dma_err 0) | 77775 | 72201 | 57 | 44092 |
| Bouncing Balls 2D | grid | 18 | 48668 | native | 2/0/0 (late 1026 torn 1343 dma_rs 1364 lcd_rs 0 dma_err 0) | 56664 | 51791 | 71 | 23693 |
| Bouncing Balls RGB 2D | grid | 7 | 48248 | native | 2/0/0 (late 1089 torn 1365 dma_rs 1393 lcd_rs 0 dma_err 0) | 142094 | 137924 | 81 | 23373 |
| Bouncing RGB Balls - 2D | grid | 5 | 49620 | native | 2/0/0 (late 1127 torn 1383 dma_rs 1414 lcd_rs 0 dma_err 0) | 209309 | 205454 | 93 | 23004 |
| Bouncy Boxes | grid | 29 | 48280 | native | 2/0/0 (late 1155 torn 1423 dma_rs 1458 lcd_rs 0 dma_err 0) | 20028 | 14426 | 37 | 26233 |
| Breakout 2D | grid | 12 | 48516 | native | 3/0/0 (late 1191 torn 1457 dma_rs 1490 lcd_rs 0 dma_err 0) | 87464 | 82827 | 86 | 23623 |
| Breathing Gradient | strip | 2 | 49736 | native | 3/0/0 (late 1208 torn 1478 dma_rs 1514 lcd_rs 0 dma_err 0) | 671518 | 666417 | 112 | 24529 |
| Bubble Column | strip | 5 | 49232 | native | 3/0/0 (late 1229 torn 1497 dma_rs 1548 lcd_rs 0 dma_err 0) | 217017 | 211150 | 100 | 23558 |
| Bulk Bouncing Balls 2D | grid | 26 | 49084 | native | 3/0/0 (late 1248 torn 1518 dma_rs 1579 lcd_rs 0 dma_err 0) | 38980 | 33672 | 60 | 26339 |
| Bulk Canvas Ripples 2D | grid | 29 | 48644 | native | 3/0/0 (late 1280 torn 1551 dma_rs 1609 lcd_rs 0 dma_err 0) | 17484 | 11657 | 41 | 25653 |
| Bulk Comet Trails | any | 31 | 49592 | native | 4/0/0 (late 1308 torn 1568 dma_rs 1632 lcd_rs 0 dma_err 0) | 10920 | 5153 | 30 | 24119 |
| Bulk Rainbow | strip | 31 | 49708 | native | 5/0/0 (late 1332 torn 1581 dma_rs 1658 lcd_rs 0 dma_err 0) | 21295 | 15843 | 26 | 24365 |
| Bulk Sprite Scroll 2D | grid | 30 | 49540 | native | 6/0/0 (late 1355 torn 1595 dma_rs 1688 lcd_rs 0 dma_err 0) | 33092 | 27412 | 47 | 23724 |
| bustle | strip | 8 | 49508 | native | 7/0/0 (late 1383 torn 1640 dma_rs 1710 lcd_rs 0 dma_err 0) | 134707 | 129042 | 178 | 40002 |
| Butterfly 2D | grid | 2 | 48484 | native | 7/0/0 (late 1409 torn 1693 dma_rs 1772 lcd_rs 0 dma_err 0) | 624565 | 619866 | 132 | 25631 |
| Carrie's Holiday Star 2D | grid | 3 | 49204 | native | 7/0/0 (late 1436 torn 1728 dma_rs 1815 lcd_rs 0 dma_err 0) | 399182 | 394659 | 113 | 23846 |
| Cellular Automata 1D | strip | 10 | 48176 | native | 7/0/0 (late 1446 torn 1758 dma_rs 1838 lcd_rs 0 dma_err 0) | 102259 | 96338 | 77 | 27360 |
| Chasing Rainbows & HSLuv | strip | 10 | 46992 | none | 7/0/0 (late 1479 torn 1791 dma_rs 1863 lcd_rs 0 dma_err 0) | 103018 | 97087 | 0 | 0 |
| Chevron 2D | grid | 19 | 50148 | native | 7/0/0 (late 1494 torn 1801 dma_rs 1887 lcd_rs 0 dma_err 0) | 52790 | 48063 | 54 | 22876 |
| chill confetti | any | 22 | 48804 | native | 7/0/0 (late 1525 torn 1822 dma_rs 1903 lcd_rs 0 dma_err 0) | 45505 | 39955 | 42 | 34427 |
| Christmas Candy Cane | strip | 23 | 49180 | native | 7/0/0 (late 1542 torn 1847 dma_rs 1931 lcd_rs 0 dma_err 0) | 43351 | 38542 | 43 | 22935 |
| Christmas Lights | strip | 19 | 48792 | native | 7/0/0 (late 1563 torn 1881 dma_rs 1954 lcd_rs 0 dma_err 0) | 53729 | 49394 | 41 | 22272 |
| Christmas RG Fade | strip | 13 | 49916 | native | 7/0/0 (late 1577 torn 1890 dma_rs 1982 lcd_rs 0 dma_err 0) | 79288 | 73424 | 47 | 28566 |
| ChristmasLights | strip | 22 | 49144 | native | 7/0/0 (late 1586 torn 1906 dma_rs 2006 lcd_rs 0 dma_err 0) | 45351 | 40728 | 53 | 22722 |
| ChristmasPewPew | strip | 8 | 49316 | native | 7/0/0 (late 1606 torn 1933 dma_rs 2025 lcd_rs 0 dma_err 0) | 133715 | 126888 | 78 | 39761 |
| ChristmasStretch | strip | 22 | 50004 | native | 7/0/0 (late 1627 torn 1952 dma_rs 2053 lcd_rs 0 dma_err 0) | 46878 | 40609 | 44 | 22857 |
| color bands | strip | 11 | 49028 | native | 7/0/0 (late 1637 torn 1984 dma_rs 2086 lcd_rs 0 dma_err 0) | 93084 | 87947 | 81 | 22732 |
| color bands (buffered) | any | 8 | 48980 | native | 7/0/0 (late 1667 torn 2007 dma_rs 2125 lcd_rs 0 dma_err 0) | 137313 | 131612 | 70 | 26269 |
| Color Blend | strip | 16 | 48384 | native | 7/0/0 (late 1674 torn 2039 dma_rs 2153 lcd_rs 0 dma_err 0) | 65664 | 59815 | 59 | 22753 |
| color fade pulse | strip | 14 | 49196 | native | 7/0/0 (late 1698 torn 2061 dma_rs 2178 lcd_rs 0 dma_err 0) | 73962 | 68504 | 78 | 23163 |
| Color Pick Fade | strip | 21 | 48956 | native | 7/0/0 (late 1723 torn 2078 dma_rs 2200 lcd_rs 0 dma_err 0) | 48587 | 42595 | 46 | 22885 |
| color twinkle bounce | strip | 12 | 48688 | native | 7/0/0 (late 1731 torn 2094 dma_rs 2231 lcd_rs 0 dma_err 0) | 87930 | 82802 | 71 | 22691 |
| Color Twinkles | strip | 8 | 48836 | native | 7/0/0 (late 1741 torn 2111 dma_rs 2256 lcd_rs 0 dma_err 0) | 133442 | 128689 | 85 | 22761 |
| colourful fireflies | strip | 12 | 48308 | native | 7/0/0 (late 1760 torn 2138 dma_rs 2277 lcd_rs 0 dma_err 0) | 83269 | 77092 | 71 | 38946 |
| Comets | strip | 12 | 49700 | native | 7/0/0 (late 1787 torn 2148 dma_rs 2294 lcd_rs 0 dma_err 0) | 85077 | 79455 | 81 | 35737 |
| Complements 3D | grid | 13 | 48992 | native | 7/0/0 (late 1793 torn 2182 dma_rs 2318 lcd_rs 0 dma_err 0) | 81339 | 77084 | 63 | 22910 |
| Continuous Cellular Automata | grid | 10 | 48084 | native | 7/0/0 (late 1828 torn 2240 dma_rs 2344 lcd_rs 0 dma_err 0) | 107425 | 102959 | 87 | 23743 |
| coolaura | strip | 9 | 48524 | native | 7/0/0 (late 1841 torn 2274 dma_rs 2374 lcd_rs 0 dma_err 0) | 113090 | 107192 | 70 | 32038 |
| Coral Plasma | grid | 3 | 49472 | native | 7/0/0 (late 1851 torn 2308 dma_rs 2412 lcd_rs 0 dma_err 0) | 395526 | 391477 | 94 | 24685 |
| Coronal Ejection 2D | grid | 3 | 46780 | native | 7/0/0 (late 1904 torn 2361 dma_rs 2440 lcd_rs 0 dma_err 0) | 404282 | 400240 | 105 | 25670 |
| Coronal Mass Ejection | grid | 3 | 47624 | native | 7/0/0 (late 1928 torn 2390 dma_rs 2476 lcd_rs 0 dma_err 0) | 401290 | 397194 | 116 | 26348 |
| Crawling Spider 2D | grid | 2 | 48620 | native | 7/0/0 (late 1948 torn 2432 dma_rs 2516 lcd_rs 0 dma_err 0) | 526545 | 522428 | 153 | 25339 |
| Crossfading | strip | 7 | 47952 | native | 7/0/0 (late 1978 torn 2458 dma_rs 2539 lcd_rs 0 dma_err 0) | 138673 | 134573 | 99 | 23447 |
| Crosshair Pulse 2D | grid | 4 | 49124 | native | 7/0/0 (late 1997 torn 2479 dma_rs 2575 lcd_rs 0 dma_err 0) | 305708 | 301703 | 95 | 23741 |
| Crosstown Traffic 2D | grid | 2 | 48972 | native | 7/0/0 (late 2027 torn 2511 dma_rs 2596 lcd_rs 0 dma_err 0) | 1094379 | 1090189 | 0 | 0 |
| cube fire 3D | grid | 9 | 48424 | native | 7/0/0 (late 2048 torn 2540 dma_rs 2623 lcd_rs 0 dma_err 0) | 110110 | 105922 | 70 | 23139 |
| Curl Flow 2D | grid | 6 | 48360 | native | 7/0/0 (late 2067 torn 2576 dma_rs 2656 lcd_rs 0 dma_err 0) | 170233 | 165680 | 99 | 25319 |
| Custom Sequences | strip | 14 | 44972 | native | 7/0/0 (late 2095 torn 2635 dma_rs 2684 lcd_rs 0 dma_err 0) | 73386 | 69133 | 66 | 22510 |
| Cyclic Cellular Automata 2D | grid | 15 | 48744 | native | 7/0/0 (late 2110 torn 2668 dma_rs 2722 lcd_rs 0 dma_err 0) | 67021 | 62681 | 61 | 23225 |
| Cylon | strip | 15 | 49324 | native | 7/0/0 (late 2123 torn 2686 dma_rs 2751 lcd_rs 0 dma_err 0) | 66065 | 60308 | 53 | 28772 |
| DBZBattleFinal | grid | 5 | 47620 | native | 7/0/0 (late 2171 torn 2731 dma_rs 2772 lcd_rs 0 dma_err 0) | 216956 | 212800 | 92 | 24141 |
| Digital Rain 2D | grid | 11 | 49728 | native | 7/0/0 (late 2181 torn 2747 dma_rs 2794 lcd_rs 0 dma_err 0) | 97473 | 93239 | 75 | 22935 |
| Dire Spider 2D | grid | 1 | 47708 | native | 7/0/0 (late 2221 torn 2785 dma_rs 2842 lcd_rs 0 dma_err 0) | 1042288 | 1037994 | 105 | 26471 |
| distance function kaleidoscope 2 | grid | 2 | 49508 | native | 7/0/0 (late 2242 torn 2813 dma_rs 2923 lcd_rs 0 dma_err 0) | 469244 | 461882 | 108 | 75183 |
| DNA Helix 2D | grid | 7 | 49812 | native | 7/0/0 (late 2249 torn 2816 dma_rs 3002 lcd_rs 0 dma_err 0) | 145150 | 140309 | 91 | 23158 |
| Doom Fire | grid | 13 | 48680 | native | 7/0/0 (late 2286 torn 2842 dma_rs 3023 lcd_rs 0 dma_err 0) | 76709 | 72071 | 79 | 23256 |
| Doom Fire 2D | grid | 15 | 48036 | native | 7/0/0 (late 2310 torn 2888 dma_rs 3057 lcd_rs 0 dma_err 0) | 68417 | 63831 | 70 | 23392 |
| Drip | strip | 16 | 49788 | native | 7/0/0 (late 2342 torn 2900 dma_rs 3081 lcd_rs 0 dma_err 0) | 62371 | 56457 | 62 | 33158 |
| Easing Library v1.0 | grid | 6 | 49112 | native | 7/0/0 (late 2416 torn 2920 dma_rs 3106 lcd_rs 0 dma_err 0) | 169290 | 165177 | 86 | 24072 |
| Edgeburst | strip | 22 | 50108 | native | 7/0/0 (late 2421 torn 2936 dma_rs 3141 lcd_rs 0 dma_err 0) | 45038 | 40402 | 42 | 22881 |
| Ember Diffusion | strip | 7 | 49752 | native | 7/0/0 (late 2427 torn 2941 dma_rs 3169 lcd_rs 0 dma_err 0) | 151114 | 145315 | 74 | 23407 |
| Emoji Animation #2 | grid | 6 | 47612 | native | 7/0/0 (late 2446 torn 2975 dma_rs 3198 lcd_rs 0 dma_err 0) | 184744 | 180618 | 82 | 23618 |
| Example - Button w/ debounce | strip | 26 | 49180 | native | 7/0/0 (late 2465 torn 2992 dma_rs 3218 lcd_rs 0 dma_err 0) | 38230 | 32440 | 48 | 23237 |
| Example: color hues | strip | 20 | 47832 | native | 7/0/0 (late 2486 torn 3034 dma_rs 3238 lcd_rs 0 dma_err 0) | 51485 | 47052 | 51 | 22522 |
| Example: modes and waveforms | strip | 18 | 47472 | native | 7/0/0 (late 2504 torn 3038 dma_rs 3260 lcd_rs 0 dma_err 0) | 57298 | 52883 | 66 | 22632 |
| Example: Smooth Speed Slider | strip | 26 | 49820 | native | 7/0/0 (late 2532 torn 3045 dma_rs 3282 lcd_rs 0 dma_err 0) | 37948 | 32863 | 39 | 22950 |
| Example: time and animation | strip | 17 | 48496 | native | 8/0/0 (late 2562 torn 3057 dma_rs 3305 lcd_rs 0 dma_err 0) | 61224 | 56752 | 61 | 22805 |
| Eye of Sauron | grid | 2 | 48708 | native | 8/0/0 (late 2594 torn 3100 dma_rs 3332 lcd_rs 0 dma_err 0) | 563739 | 559180 | 104 | 25318 |
| Eye of Sauron with movement | grid | 2 | 48692 | native | 8/0/0 (late 2615 torn 3129 dma_rs 3366 lcd_rs 0 dma_err 0) | 590358 | 585599 | 145 | 25387 |
| Falling Sand 2D | grid | 17 | 49788 | native | 8/0/0 (late 2636 torn 3157 dma_rs 3407 lcd_rs 0 dma_err 0) | 59268 | 54767 | 71 | 23111 |
| Fast Palette Blending | strip | 20 | 47576 | native | 8/0/0 (late 2676 torn 3181 dma_rs 3429 lcd_rs 0 dma_err 0) | 50562 | 45843 | 49 | 22990 |
| fast pulse | strip | 19 | 50184 | native | 8/0/0 (late 2697 torn 3199 dma_rs 3449 lcd_rs 0 dma_err 0) | 53900 | 49240 | 54 | 22851 |
| fast pulse 3d | grid | 9 | 48408 | native | 8/0/0 (late 2737 torn 3224 dma_rs 3481 lcd_rs 0 dma_err 0) | 115064 | 110939 | 74 | 23284 |
| fire - blue | strip | 3 | 48976 | native | 8/0/0 (late 2759 torn 3245 dma_rs 3508 lcd_rs 0 dma_err 0) | 392731 | 386709 | 87 | 28100 |
| fire - red | strip | 4 | 49688 | native | 8/0/0 (late 2786 torn 3257 dma_rs 3534 lcd_rs 0 dma_err 0) | 289399 | 281752 | 82 | 27943 |
| fireblobs | strip | 4 | 48060 | native | 8/0/0 (late 2799 torn 3281 dma_rs 3558 lcd_rs 0 dma_err 0) | 259547 | 253580 | 85 | 44268 |
| Fireflies | strip | 16 | 48808 | native | 8/0/0 (late 2830 torn 3327 dma_rs 3579 lcd_rs 0 dma_err 0) | 63230 | 57351 | 61 | 33143 |
| firework dust | strip | 13 | 49008 | native | 8/0/0 (late 2851 torn 3339 dma_rs 3604 lcd_rs 0 dma_err 0) | 80775 | 75113 | 61 | 32239 |
| firework nova | grid | 9 | 48764 | native | 8/0/0 (late 2877 torn 3384 dma_rs 3629 lcd_rs 0 dma_err 0) | 115203 | 110878 | 74 | 23053 |
| firework rocket sparks | strip | 17 | 48816 | native | 8/0/0 (late 2898 torn 3394 dma_rs 3654 lcd_rs 0 dma_err 0) | 58173 | 53848 | 46 | 22416 |
| Fireworks Finale | any | 13 | 47132 | native | 8/0/0 (late 2941 torn 3456 dma_rs 3694 lcd_rs 0 dma_err 0) | 77290 | 71460 | 60 | 44315 |
| Flash Posterize + Music Sequencer framework | grid | 8 | 44040 | native | 8/0/0 (late 2988 torn 3531 dma_rs 3720 lcd_rs 0 dma_err 0) | 138466 | 131604 | 93 | 23788 |
| Flow Field 2D | grid | 30 | 49568 | native | 8/0/0 (late 3008 torn 3548 dma_rs 3747 lcd_rs 0 dma_err 0) | 15463 | 10061 | 43 | 25581 |
| fractal flower | grid | 29 | 46828 | native | 9/0/0 (late 3052 torn 3588 dma_rs 3782 lcd_rs 0 dma_err 0) | 20372 | 14900 | 49 | 26522 |
| Frame Rate Scan | grid | 31 | 49336 | native | 10/0/0 (late 3109 torn 3608 dma_rs 3823 lcd_rs 0 dma_err 0) | 10559 | 5221 | 30 | 24321 |
| Frame Rate Scan (fast) | grid | 36 | 50008 | native | 12/0/0 (late 3144 torn 3617 dma_rs 3846 lcd_rs 0 dma_err 0) | 6227 | 811 | 33 | 22032 |
| Frame Rate Test | grid | 30 | 48992 | native | 14/0/0 (late 3173 torn 3646 dma_rs 3880 lcd_rs 0 dma_err 0) | 13015 | 7568 | 32 | 25866 |
| Frogger 2D | grid | 10 | 44336 | native | 16/0/0 (late 3220 torn 3741 dma_rs 3930 lcd_rs 0 dma_err 0) | 105691 | 101029 | 88 | 24110 |
| Geometry Morphing Demo 2D | grid | 7 | 48444 | native | 16/0/0 (late 3236 torn 3782 dma_rs 3963 lcd_rs 0 dma_err 0) | 146292 | 142043 | 92 | 24269 |
| glitch bands | strip | 7 | 49980 | native | 16/0/0 (late 3257 torn 3791 dma_rs 3987 lcd_rs 0 dma_err 0) | 157773 | 153678 | 72 | 23528 |
| Glitter | strip | 13 | 49624 | native | 16/0/0 (late 3270 torn 3811 dma_rs 4004 lcd_rs 0 dma_err 0) | 82022 | 77637 | 63 | 22975 |
| Glittering Jewels | strip | 3 | 47764 | native | 16/0/0 (late 3291 torn 3841 dma_rs 4026 lcd_rs 0 dma_err 0) | 458681 | 454602 | 98 | 24588 |
| GlowFlow (3D coord transform API port) | grid | 10 | 48588 | native | 16/0/0 (late 3308 torn 3866 dma_rs 4060 lcd_rs 0 dma_err 0) | 101880 | 95821 | 110 | 28677 |
| Golden Tix | strip | 15 | 49072 | native | 16/0/0 (late 3325 torn 3884 dma_rs 4083 lcd_rs 0 dma_err 0) | 66559 | 60512 | 68 | 22792 |
| Gradient blue  purple pink | strip | 21 | 49228 | native | 16/0/0 (late 3331 torn 3900 dma_rs 4099 lcd_rs 0 dma_err 0) | 49219 | 42871 | 44 | 22562 |
| green ripple reflections | strip | 9 | 49020 | native | 16/0/0 (late 3361 torn 3919 dma_rs 4127 lcd_rs 0 dma_err 0) | 121929 | 117121 | 77 | 23350 |
| Grinch's Heist | grid | 14 | 47344 | native | 16/0/0 (late 3410 torn 3978 dma_rs 4167 lcd_rs 0 dma_err 0) | 70792 | 66382 | 109 | 24127 |
| Halloween color twinkles | strip | 8 | 49904 | native | 16/0/0 (late 3449 torn 3995 dma_rs 4191 lcd_rs 0 dma_err 0) | 135594 | 131538 | 93 | 23162 |
| Halloween Wavy Bands | grid | 5 | 48904 | native | 16/0/0 (late 3483 torn 4007 dma_rs 4223 lcd_rs 0 dma_err 0) | 221226 | 216799 | 95 | 24726 |
| heart | grid | 11 | 49320 | native | 16/0/0 (late 3503 torn 4033 dma_rs 4251 lcd_rs 0 dma_err 0) | 96803 | 92432 | 75 | 23821 |
| heatshivers | strip | 7 | 49360 | native | 16/0/0 (late 3534 torn 4079 dma_rs 4283 lcd_rs 0 dma_err 0) | 144019 | 137889 | 83 | 40285 |
| Holiday_Diagonal_Stripes | grid | 18 | 49860 | native | 16/0/0 (late 3549 torn 4091 dma_rs 4312 lcd_rs 0 dma_err 0) | 56247 | 50312 | 54 | 22991 |
| Ice Floes 2D | grid | 30 | 48488 | native | 16/0/0 (late 3582 torn 4120 dma_rs 4352 lcd_rs 0 dma_err 0) | 19629 | 13967 | 42 | 25396 |
| Infinite Snake | grid | 15 | 46628 | native | 17/0/0 (late 3629 torn 4181 dma_rs 4387 lcd_rs 0 dma_err 0) | 67580 | 62035 | 78 | 23489 |
| Infinite Snake v2 | grid | 31 | 46540 | native | 17/0/0 (late 3667 torn 4259 dma_rs 4414 lcd_rs 0 dma_err 0) | 13522 | 8182 | 44 | 24969 |
| Infinity Flower 2D | grid | 9 | 49244 | native | 18/0/0 (late 3693 torn 4279 dma_rs 4449 lcd_rs 0 dma_err 0) | 112065 | 107600 | 95 | 23992 |
| Interference 2D | grid | 3 | 49572 | native | 18/0/0 (late 3713 torn 4292 dma_rs 4473 lcd_rs 0 dma_err 0) | 366124 | 362042 | 91 | 25099 |
| Kaleidoscope 2D | grid | 3 | 47588 | native | 18/0/0 (late 3767 torn 4318 dma_rs 4528 lcd_rs 0 dma_err 0) | 473893 | 469810 | 121 | 27241 |
| KITT | strip | 16 | 50028 | native | 18/0/0 (late 3768 torn 4323 dma_rs 4573 lcd_rs 0 dma_err 0) | 63122 | 57240 | 58 | 30185 |
| KITT (w/ color picker) | strip | 14 | 49648 | native | 18/0/0 (late 3805 torn 4346 dma_rs 4594 lcd_rs 0 dma_err 0) | 74689 | 68904 | 56 | 26493 |
| Light Organ - 2.0 | strip | 15 | 48504 | native | 18/0/0 (late 3835 torn 4385 dma_rs 4624 lcd_rs 0 dma_err 0) | 68027 | 63572 | 98 | 24185 |
| Light Organ -- sensor board | strip | 13 | 48720 | native | 18/0/0 (late 3855 torn 4441 dma_rs 4650 lcd_rs 0 dma_err 0) | 79831 | 75616 | 68 | 23113 |
| Lightbulb - Crank Hue to Complete | strip | 21 | 48696 | native | 18/0/0 (late 3879 torn 4470 dma_rs 4678 lcd_rs 0 dma_err 0) | 47754 | 42791 | 74 | 23405 |
| Lightning clouds | grid | 5 | 48556 | native | 18/0/0 (late 3898 torn 4507 dma_rs 4703 lcd_rs 0 dma_err 0) | 246544 | 241964 | 92 | 23678 |
| Lightning Strike | strip | 9 | 48008 | native | 18/0/0 (late 3949 torn 4554 dma_rs 4740 lcd_rs 0 dma_err 0) | 117548 | 111794 | 73 | 28763 |
| Line Dancer 2D | grid | 6 | 49296 | native | 18/0/0 (late 3975 torn 4567 dma_rs 4767 lcd_rs 0 dma_err 0) | 182787 | 178129 | 110 | 24320 |
| Lissajous curve tracer | grid | 16 | 47120 | native | 18/0/0 (late 4010 torn 4624 dma_rs 4797 lcd_rs 0 dma_err 0) | 63087 | 58362 | 88 | 24157 |
| M5Stack Hex panels | cloud | 7 | 49484 | native | 18/0/0 (late 4031 torn 4643 dma_rs 4822 lcd_rs 0 dma_err 0) | 163558 | 159446 | 89 | 23821 |
| Main Stage | grid | 17 | 41940 | native | 18/0/0 (late 4098 torn 4730 dma_rs 4862 lcd_rs 0 dma_err 0) | 57990 | 52248 | 86 | 39064 |
| Mandelbrot 2D | grid | 11 | 49720 | native | 18/0/0 (late 4117 torn 4772 dma_rs 4883 lcd_rs 0 dma_err 0) | 90351 | 85019 | 67 | 23170 |
| Mapping Helper Single and 10x | strip | 26 | 49844 | native | 18/0/0 (late 4134 torn 4790 dma_rs 4915 lcd_rs 0 dma_err 0) | 38945 | 33766 | 37 | 22932 |
| Marching Dots | strip | 26 | 48764 | native | 18/0/0 (late 4164 torn 4800 dma_rs 4932 lcd_rs 0 dma_err 0) | 38106 | 32423 | 48 | 23462 |
| marching rainbow | strip | 12 | 49168 | native | 18/0/0 (late 4193 torn 4817 dma_rs 4950 lcd_rs 0 dma_err 0) | 84473 | 80137 | 64 | 22489 |
| marching rainbow (buffered) | any | 11 | 50100 | native | 18/0/0 (late 4207 torn 4846 dma_rs 4975 lcd_rs 0 dma_err 0) | 96871 | 90811 | 73 | 25667 |
| Marquee Chase | strip | 23 | 49360 | native | 18/0/0 (late 4210 torn 4860 dma_rs 5004 lcd_rs 0 dma_err 0) | 42820 | 37978 | 51 | 22934 |
| Matrix 2 tone pulse | strip | 9 | 49428 | native | 18/0/0 (late 4221 torn 4874 dma_rs 5023 lcd_rs 0 dma_err 0) | 111147 | 106992 | 74 | 23085 |
| matrix 2D honeycomb | grid | 10 | 49668 | native | 18/0/0 (late 4230 torn 4886 dma_rs 5046 lcd_rs 0 dma_err 0) | 106894 | 102723 | 64 | 23453 |
| matrix 2D pulse edit | strip | 10 | 49972 | native | 18/0/0 (late 4247 torn 4900 dma_rs 5071 lcd_rs 0 dma_err 0) | 106069 | 101103 | 63 | 23205 |
| Matrix Green Waterfall 1D | strip | 17 | 48976 | native | 18/0/0 (late 4258 torn 4922 dma_rs 5103 lcd_rs 0 dma_err 0) | 61144 | 55476 | 66 | 31888 |
| Matrix Green Waterfall 2D | grid | 17 | 49800 | native | 18/0/0 (late 4276 torn 4930 dma_rs 5127 lcd_rs 0 dma_err 0) | 59793 | 55506 | 74 | 23042 |
| matrix rain | grid | 19 | 49896 | native | 18/0/0 (late 4306 torn 4947 dma_rs 5157 lcd_rs 0 dma_err 0) | 53677 | 49159 | 60 | 23065 |
| Metaballs of Fire 2D | grid | 4 | 49132 | native | 18/0/0 (late 4327 torn 4983 dma_rs 5187 lcd_rs 0 dma_err 0) | 328214 | 324098 | 107 | 24050 |
| Meteor Shower | strip | 15 | 48564 | native | 18/0/0 (late 4335 torn 5010 dma_rs 5219 lcd_rs 0 dma_err 0) | 66338 | 60556 | 61 | 27080 |
| MidpointDisplacement1D | strip | 17 | 48268 | native | 18/0/0 (late 4373 torn 5030 dma_rs 5248 lcd_rs 0 dma_err 0) | 60841 | 56089 | 54 | 22611 |
| millipede | strip | 16 | 49132 | native | 18/0/0 (late 4405 torn 5053 dma_rs 5269 lcd_rs 0 dma_err 0) | 62851 | 56791 | 59 | 22827 |
| millipede 1d/2d controls | grid | 3 | 48464 | native | 18/0/0 (late 4439 torn 5095 dma_rs 5314 lcd_rs 0 dma_err 0) | 355518 | 350819 | 104 | 25869 |
| multimap simpledemo | grid | 11 | 49200 | native | 18/0/0 (late 4458 torn 5133 dma_rs 5364 lcd_rs 0 dma_err 0) | 95695 | 91536 | 85 | 23109 |
| Multisegment Demo | strip | 11 | 46584 | native | 18/0/0 (late 4498 torn 5199 dma_rs 5397 lcd_rs 0 dma_err 0) | 94883 | 90619 | 86 | 23558 |
| Nano Orbital | strip | 16 | 49944 | native | 18/0/0 (late 4526 torn 5216 dma_rs 5408 lcd_rs 0 dma_err 0) | 65668 | 59669 | 54 | 34985 |
| NaturalLightSync | strip | 30 | 49740 | native | 18/0/0 (late 4553 torn 5232 dma_rs 5430 lcd_rs 0 dma_err 0) | 33267 | 27399 | 41 | 23965 |
| neutronorbit | strip | 8 | 48556 | native | 19/0/0 (late 4575 torn 5257 dma_rs 5461 lcd_rs 0 dma_err 0) | 124168 | 118719 | 76 | 44794 |
| Newfire | strip | 6 | 48724 | native | 19/0/0 (late 4600 torn 5302 dma_rs 5490 lcd_rs 0 dma_err 0) | 195467 | 189650 | 73 | 26202 |
| novas | strip | 4 | 47848 | native | 19/0/0 (late 4625 torn 5350 dma_rs 5519 lcd_rs 0 dma_err 0) | 273242 | 267274 | 83 | 44093 |
| Nyan Lights | grid | 31 | 47800 | native | 19/0/0 (late 4650 torn 5394 dma_rs 5548 lcd_rs 0 dma_err 0) | 12505 | 7013 | 37 | 24311 |
| Oasis | strip | 7 | 48112 | native | 20/0/0 (late 4704 torn 5419 dma_rs 5573 lcd_rs 0 dma_err 0) | 157020 | 152642 | 85 | 23769 |
| Ocean | strip | 12 | 49744 | native | 20/0/0 (late 4720 torn 5436 dma_rs 5592 lcd_rs 0 dma_err 0) | 86104 | 81803 | 65 | 22545 |
| Opening Act | grid | 15 | 40824 | native | 20/0/0 (late 4777 torn 5528 dma_rs 5618 lcd_rs 0 dma_err 0) | 68619 | 64302 | 66 | 23416 |
| opposites | strip | 14 | 48792 | native | 20/0/0 (late 4791 torn 5539 dma_rs 5643 lcd_rs 0 dma_err 0) | 74790 | 69317 | 69 | 22531 |
| Orv - Christmas Tree | grid | 8 | 48324 | native | 20/0/0 (late 4816 torn 5581 dma_rs 5667 lcd_rs 0 dma_err 0) | 127646 | 123549 | 100 | 23567 |
| Palette Fire 2D | grid | 5 | 50092 | native | 20/0/0 (late 4857 torn 5591 dma_rs 5681 lcd_rs 0 dma_err 0) | 236514 | 232363 | 81 | 23638 |
| Pendulum Wave | strip | 16 | 50076 | native | 20/0/0 (late 4885 torn 5605 dma_rs 5705 lcd_rs 0 dma_err 0) | 63186 | 58817 | 59 | 22497 |
| Performance test framework | strip | 23 | 49556 | native | 20/0/0 (late 4908 torn 5611 dma_rs 5741 lcd_rs 0 dma_err 0) | 43298 | 38526 | 45 | 23291 |
| Perlin fire | grid | 23 | 48288 | native | 20/0/0 (late 4930 torn 5668 dma_rs 5763 lcd_rs 0 dma_err 0) | 43692 | 38705 | 0 | 0 |
| perlin fire wind | grid | — | 47940 | native | 0/0/0 (late 36 torn 58 dma_rs 51 lcd_rs 0 dma_err 0) | — | — | — | — |
| perlin fire wind tunnel | grid | 4 | 48532 | native | 0/0/0 (late 48 torn 92 dma_rs 75 lcd_rs 0 dma_err 0) | 259262 | 254634 | 97 | 24651 |
| Perlin/Simplex Noise 1D | strip | 15 | 46680 | native | 0/0/0 (late 80 torn 122 dma_rs 108 lcd_rs 0 dma_err 0) | 67049 | 61142 | 62 | 24800 |
| Perlin/Simplex Noise 2D | grid | 12 | 46428 | native | 0/0/0 (late 104 torn 164 dma_rs 138 lcd_rs 0 dma_err 0) | 83993 | 79388 | 103 | 24627 |
| Pew-Pew-Pew! | any | 8 | 47696 | native | 0/0/0 (late 139 torn 200 dma_rs 188 lcd_rs 0 dma_err 0) | 130227 | 123548 | 71 | 43959 |
| pixelClock | strip | 22 | 48552 | native | 0/0/0 (late 155 torn 245 dma_rs 211 lcd_rs 0 dma_err 0) | 45754 | 39719 | 46 | 23193 |
| Polar mapping helper 2D / 3D | grid | 12 | 49068 | native | 0/0/0 (late 167 torn 277 dma_rs 231 lcd_rs 0 dma_err 0) | 86019 | 80663 | 80 | 23280 |
| policeLights | strip | 24 | 50000 | native | 0/0/0 (late 169 torn 292 dma_rs 255 lcd_rs 0 dma_err 0) | 42295 | 37531 | 43 | 22925 |
| portal | strip | 7 | 49316 | native | 0/0/0 (late 193 torn 318 dma_rs 288 lcd_rs 0 dma_err 0) | 159587 | 153637 | 99 | 32200 |
| Post-Process Chain | strip | 7 | 47984 | native | 0/0/0 (late 200 torn 358 dma_rs 309 lcd_rs 0 dma_err 0) | 143175 | 139104 | 94 | 23441 |
| quiet blinkfade | strip | 15 | 48920 | native | 0/0/0 (late 219 torn 382 dma_rs 333 lcd_rs 0 dma_err 0) | 69592 | 63737 | 58 | 27883 |
| Radar 2D | grid | 2 | 49456 | native | 0/0/0 (late 246 torn 400 dma_rs 370 lcd_rs 0 dma_err 0) | 532020 | 528043 | 104 | 23856 |
| radiant pulse 3 | grid | 6 | 48656 | native | 0/0/0 (late 287 torn 444 dma_rs 402 lcd_rs 0 dma_err 0) | 176427 | 172009 | 100 | 23867 |
| Rainbow | strip | 25 | 50344 | native | 0/0/0 (late 287 torn 445 dma_rs 425 lcd_rs 0 dma_err 0) | 40761 | 35936 | 35 | 22680 |
| Rainbow Comet | any | 14 | 49464 | native | 0/0/0 (late 312 torn 468 dma_rs 441 lcd_rs 0 dma_err 0) | 72434 | 65679 | 48 | 29997 |
| Rainbow Flag | strip | 19 | 50104 | native | 0/0/0 (late 321 torn 478 dma_rs 473 lcd_rs 0 dma_err 0) | 53191 | 48817 | 49 | 22504 |
| rainbow fonts | strip | 15 | 49204 | native | 0/0/0 (late 356 torn 496 dma_rs 500 lcd_rs 0 dma_err 0) | 68802 | 63015 | 58 | 22542 |
| Rainbow Melt | strip | 14 | 49068 | native | 0/0/0 (late 378 torn 517 dma_rs 517 lcd_rs 0 dma_err 0) | 72149 | 66501 | 70 | 23014 |
| rainbow pinwheel | strip | 20 | 48856 | native | 0/0/0 (late 400 torn 531 dma_rs 539 lcd_rs 0 dma_err 0) | 49871 | 43604 | 42 | 22423 |
| Rainbow rocket sparks | strip | 20 | 50136 | native | 0/0/0 (late 418 torn 547 dma_rs 561 lcd_rs 0 dma_err 0) | 49871 | 43633 | 50 | 22558 |
| Rainbow Smiley | grid | 18 | 49692 | native | 0/0/0 (late 441 torn 568 dma_rs 580 lcd_rs 0 dma_err 0) | 55644 | 50907 | 57 | 22780 |
| Rainbow v2 | strip | 26 | 48904 | native | 0/0/0 (late 457 torn 596 dma_rs 596 lcd_rs 0 dma_err 0) | 39282 | 33969 | 48 | 23032 |
| Raindrops 2D | grid | 29 | 46596 | native | 0/0/0 (late 505 torn 655 dma_rs 634 lcd_rs 0 dma_err 0) | 17372 | 11806 | 46 | 26565 |
| Rainstorm | grid | 5 | 49056 | native | 1/0/0 (late 536 torn 675 dma_rs 663 lcd_rs 0 dma_err 0) | 214569 | 210476 | 115 | 25048 |
| Reaction Diffusion 2D | grid | 16 | 49060 | native | 1/0/0 (late 551 torn 694 dma_rs 701 lcd_rs 0 dma_err 0) | 64414 | 59621 | 76 | 23687 |
| Real World Lights | strip | 6 | 47712 | native | 1/0/0 (late 592 torn 717 dma_rs 738 lcd_rs 0 dma_err 0) | 173156 | 169046 | 79 | 23238 |
| regenbogendrogen | strip | 18 | 49240 | native | 1/0/0 (late 627 torn 730 dma_rs 766 lcd_rs 0 dma_err 0) | 57567 | 53170 | 58 | 22545 |
| RGB-XYZ 3D Sweep | cloud | 19 | 50008 | native | 1/0/0 (late 645 torn 745 dma_rs 795 lcd_rs 0 dma_err 0) | 52378 | 47817 | 60 | 22795 |
| RGBclock 2D | grid | 3 | 47692 | native | 1/0/0 (late 694 torn 789 dma_rs 829 lcd_rs 0 dma_err 0) | 414874 | 410307 | 101 | 24289 |
| RGBW Mapping Tester | strip | 29 | 50152 | native | 1/0/0 (late 709 torn 798 dma_rs 856 lcd_rs 0 dma_err 0) | 35064 | 29119 | 32 | 23060 |
| RGBW Mapping Tester - HSV Version | strip | 26 | 50152 | native | 2/0/0 (late 731 torn 817 dma_rs 881 lcd_rs 0 dma_err 0) | 38218 | 32224 | 32 | 22844 |
| Ripples 2D | grid | 4 | 49456 | native | 2/0/0 (late 755 torn 825 dma_rs 903 lcd_rs 0 dma_err 0) | 260526 | 256406 | 101 | 24215 |
| Rock sparks | grid | 16 | 47924 | native | 2/0/0 (late 779 torn 880 dma_rs 936 lcd_rs 0 dma_err 0) | 65403 | 61014 | 90 | 23398 |
| Rocket by Tony Hampton | any | 13 | 47732 | native | 2/0/0 (late 815 torn 900 dma_rs 980 lcd_rs 0 dma_err 0) | 81878 | 75947 | 71 | 44148 |
| RYB colors | grid | 5 | 48760 | native | 2/0/0 (late 817 torn 913 dma_rs 997 lcd_rs 0 dma_err 0) | 230427 | 226381 | 102 | 24200 |
| SaberDeploy Tutorial | strip | 27 | 49480 | native | 2/0/0 (late 829 torn 948 dma_rs 1016 lcd_rs 0 dma_err 0) | 36892 | 31376 | 54 | 23346 |
| Scanner | strip | 14 | 48824 | native | 2/0/0 (late 852 torn 967 dma_rs 1046 lcd_rs 0 dma_err 0) | 73592 | 67413 | 63 | 28809 |
| Scary Pumpkin | grid | 3 | 48884 | native | 2/0/0 (late 887 torn 978 dma_rs 1069 lcd_rs 0 dma_err 0) | 332422 | 327393 | 103 | 25979 |
| scrolling text marquee 2D | grid | 17 | 48116 | native | 2/0/0 (late 933 torn 1032 dma_rs 1121 lcd_rs 0 dma_err 0) | 57766 | 53154 | 69 | 23284 |
| scrolls | strip | 7 | 49348 | native | 2/0/0 (late 982 torn 1060 dma_rs 1161 lcd_rs 0 dma_err 0) | 166384 | 160308 | 85 | 32423 |
| Shimmer Crossfade 2D | grid | 9 | 48536 | native | 2/0/0 (late 994 torn 1092 dma_rs 1186 lcd_rs 0 dma_err 0) | 112916 | 108702 | 100 | 23828 |
| Sierpinski Rainbow 2D | grid | 13 | 48964 | native | 2/0/0 (late 1001 torn 1119 dma_rs 1222 lcd_rs 0 dma_err 0) | 77508 | 73277 | 67 | 22549 |
| Single Color Picker - wide or spot | strip | 11 | 49544 | native | 2/0/0 (late 1020 torn 1127 dma_rs 1247 lcd_rs 0 dma_err 0) | 97582 | 93437 | 57 | 22836 |
| sinpulse 3D | grid | 8 | 48800 | native | 2/0/0 (late 1041 torn 1173 dma_rs 1273 lcd_rs 0 dma_err 0) | 128490 | 123682 | 81 | 23129 |
| sinus | grid | 13 | 48792 | native | 2/0/0 (late 1058 torn 1198 dma_rs 1304 lcd_rs 0 dma_err 0) | 80886 | 75496 | 55 | 22810 |
| Slime mold palette | grid | 16 | 47188 | native | 2/0/0 (late 1101 torn 1232 dma_rs 1333 lcd_rs 0 dma_err 0) | 63288 | 58866 | 60 | 23596 |
| slow color shift | strip | 12 | 49144 | native | 2/0/0 (late 1119 torn 1259 dma_rs 1350 lcd_rs 0 dma_err 0) | 88150 | 84007 | 59 | 22618 |
| slowflies | strip | 9 | 49472 | native | 2/0/0 (late 1139 torn 1300 dma_rs 1385 lcd_rs 0 dma_err 0) | 123599 | 118047 | 94 | 32649 |
| snake | strip | 21 | 48792 | native | 2/0/0 (late 1149 torn 1321 dma_rs 1407 lcd_rs 0 dma_err 0) | 48762 | 44166 | 50 | 22820 |
| Soap 2D | grid | 5 | 50004 | native | 2/0/0 (late 1171 torn 1343 dma_rs 1429 lcd_rs 0 dma_err 0) | 237283 | 233242 | 84 | 23881 |
| sound - blinkfade | strip | 10 | 49612 | native | 2/0/0 (late 1207 torn 1358 dma_rs 1455 lcd_rs 0 dma_err 0) | 104342 | 98870 | 63 | 27214 |
| SOUND - lavablob | grid | 10 | 49456 | native | 2/0/0 (late 1225 torn 1385 dma_rs 1474 lcd_rs 0 dma_err 0) | 109232 | 105076 | 77 | 22780 |
| sound - rays | strip | 16 | 49744 | native | 2/0/0 (late 1251 torn 1406 dma_rs 1504 lcd_rs 0 dma_err 0) | 65120 | 59316 | 54 | 26416 |
| sound - rays Frequency-BPM Reactive 1 | strip | 16 | 48096 | native | 2/0/0 (late 1301 torn 1432 dma_rs 1533 lcd_rs 0 dma_err 0) | 62229 | 56586 | 79 | 27970 |
| sound - spectro kalidastrip | strip | 9 | 48452 | native | 2/0/0 (late 1351 torn 1458 dma_rs 1555 lcd_rs 0 dma_err 0) | 110267 | 103275 | 82 | 24844 |
| sound - spectroblots - pow fade | grid | 4 | 48360 | native | 2/0/0 (late 1379 torn 1495 dma_rs 1581 lcd_rs 0 dma_err 0) | 274192 | 266909 | 109 | 26888 |
| sound - spectrokalidamandala | grid | 5 | 47460 | native | 2/0/0 (late 1400 torn 1556 dma_rs 1614 lcd_rs 0 dma_err 0) | 223124 | 216062 | 122 | 27253 |
| sound - spectromatrix agc | grid | 30 | 49308 | native | 2/0/0 (late 1417 torn 1596 dma_rs 1652 lcd_rs 0 dma_err 0) | 15495 | 10011 | 37 | 25290 |
| sound - spectromatrix render2D | grid | 8 | 49320 | native | 3/0/0 (late 1464 torn 1618 dma_rs 1695 lcd_rs 0 dma_err 0) | 130315 | 125684 | 99 | 23730 |
| Sound - Spectrum Analyser | grid | 16 | 49520 | native | 3/0/0 (late 1478 torn 1640 dma_rs 1712 lcd_rs 0 dma_err 0) | 62173 | 57269 | 55 | 23543 |
| sound - Starburst 2 | strip | 10 | 48964 | native | 3/0/0 (late 1499 torn 1666 dma_rs 1741 lcd_rs 0 dma_err 0) | 106146 | 99251 | 72 | 27910 |
| Sound & Music Spectrum Visualizer | strip | 16 | 48468 | native | 3/0/0 (late 1537 torn 1689 dma_rs 1775 lcd_rs 0 dma_err 0) | 61593 | 55802 | 62 | 27054 |
| Sound Reactive Color Fade | grid | 21 | 48308 | native | 3/0/0 (late 1568 torn 1737 dma_rs 1804 lcd_rs 0 dma_err 0) | 46833 | 41981 | 51 | 23336 |
| sparkfire | strip | 10 | 49752 | native | 3/0/0 (late 1579 torn 1774 dma_rs 1836 lcd_rs 0 dma_err 0) | 108354 | 102504 | 83 | 29444 |
| sparks | strip | 17 | 49048 | native | 3/0/0 (late 1625 torn 1799 dma_rs 1873 lcd_rs 0 dma_err 0) | 59702 | 53698 | 56 | 33147 |
| sparks center | strip | 16 | 49612 | native | 3/0/0 (late 1651 torn 1834 dma_rs 1894 lcd_rs 0 dma_err 0) | 62561 | 56811 | 57 | 31883 |
| spin cycle | strip | 14 | 48888 | native | 3/0/0 (late 1672 torn 1848 dma_rs 1917 lcd_rs 0 dma_err 0) | 70763 | 65289 | 61 | 22702 |
| Spinning Plasma 2D | grid | 8 | 49820 | native | 3/0/0 (late 1688 torn 1863 dma_rs 1950 lcd_rs 0 dma_err 0) | 138340 | 133539 | 104 | 24542 |
| Spinwheel 2D | grid | 7 | 50072 | native | 3/0/0 (late 1690 torn 1884 dma_rs 1975 lcd_rs 0 dma_err 0) | 150690 | 145989 | 85 | 24394 |
| Spiral 2D | grid | 7 | 49636 | native | 3/0/0 (late 1710 torn 1895 dma_rs 2003 lcd_rs 0 dma_err 0) | 157363 | 152557 | 89 | 24210 |
| spiral twirls star 2D | grid | 7 | 47740 | native | 3/0/0 (late 1729 torn 1926 dma_rs 2034 lcd_rs 0 dma_err 0) | 145599 | 140551 | 142 | 24265 |
| Spirograph 2D | grid | 17 | 49772 | native | 3/0/0 (late 1747 torn 1947 dma_rs 2058 lcd_rs 0 dma_err 0) | 61013 | 56528 | 59 | 23685 |
| spotlights / rotation 3D | grid | 9 | 49140 | native | 3/0/0 (late 1788 torn 1988 dma_rs 2082 lcd_rs 0 dma_err 0) | 111757 | 107277 | 90 | 23435 |
| Spring Colors | strip | 9 | 48732 | native | 3/0/0 (late 1812 torn 2008 dma_rs 2104 lcd_rs 0 dma_err 0) | 124817 | 119162 | 87 | 28252 |
| Stacker | strip | 23 | 48276 | native | 3/0/0 (late 1837 torn 2021 dma_rs 2125 lcd_rs 0 dma_err 0) | 43559 | 37410 | 47 | 23220 |
| Stairmaster 2D | grid | 10 | 49972 | native | 3/0/0 (late 1862 torn 2040 dma_rs 2150 lcd_rs 0 dma_err 0) | 102598 | 97584 | 86 | 22901 |
| Starfield 2D | grid | 17 | 49368 | native | 3/0/0 (late 1865 torn 2063 dma_rs 2166 lcd_rs 0 dma_err 0) | 59534 | 54962 | 68 | 23648 |
| StarGen polar 2D | grid | 6 | 44920 | native | 3/0/0 (late 1901 torn 2120 dma_rs 2203 lcd_rs 0 dma_err 0) | 170607 | 166357 | 89 | 24658 |
| Static Christmas Lights - 4 Colors | strip | 22 | 49916 | native | 3/0/0 (late 1922 torn 2131 dma_rs 2230 lcd_rs 0 dma_err 0) | 45360 | 40663 | 44 | 22998 |
| static random colors | strip | 16 | 50032 | native | 3/0/0 (late 1952 torn 2146 dma_rs 2250 lcd_rs 0 dma_err 0) | 62784 | 58614 | 52 | 22650 |
| Sun rays through trees | grid | 3 | 48472 | native | 3/0/0 (late 1970 torn 2186 dma_rs 2283 lcd_rs 0 dma_err 0) | 398275 | 394269 | 141 | 27137 |
| Sunrise | strip | 22 | 49748 | native | 3/0/0 (late 1972 torn 2218 dma_rs 2316 lcd_rs 0 dma_err 0) | 45018 | 40006 | 59 | 23464 |
| Sunrise 2D | grid | 16 | 48268 | native | 3/0/0 (late 2009 torn 2249 dma_rs 2345 lcd_rs 0 dma_err 0) | 62200 | 57518 | 57 | 23052 |
| Sunrise Alarm Clock | strip | 3 | 46580 | native | 3/0/0 (late 2036 torn 2306 dma_rs 2376 lcd_rs 0 dma_err 0) | 415330 | 411189 | 133 | 24281 |
| Sunset | strip | 9 | 49748 | native | 3/0/0 (late 2047 torn 2326 dma_rs 2406 lcd_rs 0 dma_err 0) | 118664 | 113672 | 105 | 23259 |
| Swirlpool 2D | grid | 31 | 49032 | native | 3/0/0 (late 2076 torn 2360 dma_rs 2440 lcd_rs 0 dma_err 0) | 13351 | 7973 | 44 | 24870 |
| Synchronized Random Numbers | strip | 4 | 49608 | native | 5/0/0 (late 2103 torn 2374 dma_rs 2459 lcd_rs 0 dma_err 0) | 298756 | 292927 | 103 | 23563 |
| Tetrix 2D | grid | 14 | 49588 | native | 5/0/0 (late 2125 torn 2395 dma_rs 2478 lcd_rs 0 dma_err 0) | 75255 | 70877 | 59 | 23203 |
| Three Red Pixels (array) | strip | 17 | 49956 | native | 5/0/0 (late 2138 torn 2405 dma_rs 2499 lcd_rs 0 dma_err 0) | 60542 | 54748 | 51 | 31637 |
| Thunderstorm | strip | 21 | 49484 | native | 5/0/0 (late 2169 torn 2417 dma_rs 2524 lcd_rs 0 dma_err 0) | 47011 | 42338 | 60 | 22726 |
| Time Flies 2D | grid | 8 | 49216 | native | 5/0/0 (late 2195 torn 2431 dma_rs 2544 lcd_rs 0 dma_err 0) | 134485 | 130407 | 95 | 23549 |
| tixy | grid | 10 | 48120 | native | 5/0/0 (late 2215 torn 2464 dma_rs 2568 lcd_rs 0 dma_err 0) | 101511 | 97317 | 74 | 22928 |
| Traffic | grid | 10 | 48836 | native | 5/0/0 (late 2240 torn 2475 dma_rs 2590 lcd_rs 0 dma_err 0) | 98883 | 93444 | 89 | 24903 |
| tree setup pattern | grid | 9 | 49632 | native | 5/0/0 (late 2271 torn 2484 dma_rs 2616 lcd_rs 0 dma_err 0) | 113326 | 108344 | 70 | 22926 |
| Tunnel of Squares 2D | grid | 6 | 49440 | native | 5/0/0 (late 2285 torn 2517 dma_rs 2641 lcd_rs 0 dma_err 0) | 168224 | 163535 | 102 | 23554 |
| TV Simulator | strip | 23 | 49968 | native | 5/0/0 (late 2309 torn 2536 dma_rs 2672 lcd_rs 0 dma_err 0) | 44687 | 38576 | 36 | 22723 |
| Twinkle | strip | 17 | 48812 | native | 5/0/0 (late 2330 torn 2550 dma_rs 2693 lcd_rs 0 dma_err 0) | 61197 | 55278 | 56 | 27408 |
| twinkle (2) | any | 18 | 48860 | native | 5/0/0 (late 2351 torn 2563 dma_rs 2708 lcd_rs 0 dma_err 0) | 54873 | 48849 | 54 | 31392 |
| Twinkling Classic Xmas Strands | strip | 8 | 46504 | native | 5/0/0 (late 2388 torn 2599 dma_rs 2734 lcd_rs 0 dma_err 0) | 130729 | 125118 | 86 | 24914 |
| twinkly stars | strip | 18 | 49192 | native | 5/0/0 (late 2416 torn 2614 dma_rs 2769 lcd_rs 0 dma_err 0) | 55663 | 48800 | 86 | 26271 |
| TwoColorHSVMix | strip | 16 | 48964 | native | 5/0/0 (late 2430 torn 2643 dma_rs 2787 lcd_rs 0 dma_err 0) | 62782 | 56955 | 55 | 22956 |
| Typing Heatmap 2D | grid | 9 | 49312 | native | 5/0/0 (late 2449 torn 2668 dma_rs 2811 lcd_rs 0 dma_err 0) | 111879 | 107537 | 172 | 23861 |
| Unstable Orbits 2D | grid | 16 | 48936 | native | 5/0/0 (late 2461 torn 2695 dma_rs 2842 lcd_rs 0 dma_err 0) | 62491 | 57690 | 63 | 23931 |
| Upward waves 3D using accelerometer | cloud | 10 | 49644 | native | 5/0/0 (late 2475 torn 2710 dma_rs 2865 lcd_rs 0 dma_err 0) | 102343 | 98083 | 99 | 24344 |
| US Flag | strip | 9 | 48528 | native | 5/0/0 (late 2502 torn 2727 dma_rs 2895 lcd_rs 0 dma_err 0) | 111796 | 107630 | 73 | 22828 |
| US Flag 2D | grid | 9 | 47864 | native | 5/0/0 (late 2534 torn 2762 dma_rs 2932 lcd_rs 0 dma_err 0) | 120919 | 116689 | 90 | 23642 |
| Utility: Palettes | strip | 8 | 46600 | native | 5/0/0 (late 2587 torn 2818 dma_rs 2975 lcd_rs 0 dma_err 0) | 129551 | 125463 | 73 | 22808 |
| UtilityColorTemp | strip | 31 | 49736 | native | 5/0/0 (late 2612 torn 2831 dma_rs 2994 lcd_rs 0 dma_err 0) | 32525 | 27041 | 31 | 23666 |
| Voronoi 2D | grid | 4 | 48828 | native | 6/0/0 (late 2632 torn 2872 dma_rs 3019 lcd_rs 0 dma_err 0) | 323002 | 318962 | 120 | 23220 |
| wanderedges | strip | 10 | 49572 | native | 6/0/0 (late 2657 torn 2892 dma_rs 3046 lcd_rs 0 dma_err 0) | 100718 | 94704 | 68 | 32698 |
| wanderers | grid | 19 | 49804 | native | 6/0/0 (late 2696 torn 2915 dma_rs 3070 lcd_rs 0 dma_err 0) | 53857 | 49157 | 55 | 22945 |
| Wavy Bands | grid | 5 | 49004 | native | 6/0/0 (late 2724 torn 2960 dma_rs 3100 lcd_rs 0 dma_err 0) | 213514 | 209427 | 98 | 24882 |
| White Rainbows | strip | 13 | 49380 | native | 6/0/0 (late 2735 torn 2985 dma_rs 3131 lcd_rs 0 dma_err 0) | 79333 | 72210 | 56 | 26883 |
| Wichmann–Hill PRNG | strip | 7 | 48476 | native | 6/0/0 (late 2751 torn 3016 dma_rs 3163 lcd_rs 0 dma_err 0) | 155925 | 148850 | 66 | 23969 |
| XmasFlies | strip | 11 | 49652 | native | 6/0/0 (late 2763 torn 3046 dma_rs 3184 lcd_rs 0 dma_err 0) | 90217 | 85930 | 69 | 23570 |
| xorcery 2D/3D | grid | 9 | 48924 | native | 6/0/0 (late 2783 torn 3083 dma_rs 3213 lcd_rs 0 dma_err 0) | 111290 | 106356 | 95 | 23830 |
| zoom kaleidoscope | grid | 3 | 48408 | native | 6/0/0 (late 2813 torn 3123 dma_rs 3243 lcd_rs 0 dma_err 0) | 355561 | 348458 | 101 | 26835 |

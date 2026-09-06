# Pattern performance sweep — 2026-09-06

*Device 192.168.0.238 (Seengreat HUB75 S3), master e5935e6 (`board-seengreat-hub75`, `CORE_O3=1`), 4096 px, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip> <report.md> --perf-only`.*

- **224 of 299** gallery patterns rendered and were measured; 75 were refused by the engine or faulted (listed at the end) — at 4096 px most of those are arrays that no longer fit.
- vm µs/frame: median **128437**, p90 **468966**, max 2382544.
- frame µs: median 134831, p90 475504. fps: median **8**, p10 3, p90 20.
- The VM is 95 % of the median frame; the HUB75 compose (`out`) is a flat 5.4–6.4 ms whatever runs.

| # | pattern | kind | fps | frame µs | vm µs | pipe µs | out µs | heap free |
|---:|---|---|---:|---:|---:|---:|---:|---:|
| 1 | cube fire 3D | grid | 1 | 2389091 | 2382544 | 130 | 6345 | 48004 |
| 2 | Crosstown Traffic 2D | grid | 2 | 1757116 | 1750559 | 130 | 6356 | 47188 |
| 3 | Crawling Spider 2D | grid | 1 | 1392033 | 1385484 | 126 | 6350 | 46284 |
| 4 | Butterfly 2D | grid | 2 | 1028925 | 1022374 | 130 | 6349 | 47220 |
| 5 | RYB colors | grid | 2 | 854933 | 848417 | 122 | 6321 | 48160 |
| 6 | Dire Spider 2D | grid | 3 | 712606 | 705659 | 167 | 6707 | 44672 |
| 7 | RGBclock 2D | grid | 2 | 657338 | 650845 | 107 | 6317 | 46324 |
| 8 | Radar 2D | grid | 2 | 639515 | 633033 | 99 | 6317 | 49244 |
| 9 | Kaleidoscope 2D | grid | 2 | 620878 | 614401 | 122 | 6298 | 46420 |
| 10 | 80s kid show | grid | 2 | 604099 | 597640 | 107 | 6295 | 42460 |
| 11 | 2D sinc(theta)/theta | grid | 2 | 600027 | 593554 | 114 | 6297 | 48520 |
| 12 | Glittering Jewels | strip | 2 | 595072 | 588454 | 144 | 6402 | 45720 |
| 13 | All Lasers Fire | grid | 2 | 594019 | 587506 | 111 | 6343 | 49132 |
| 14 | Voronoi 2D | grid | 2 | 571750 | 565286 | 107 | 6296 | 47904 |
| 15 | Mandelbrot 2D | grid | 2 | 568009 | 561565 | 82 | 6297 | 49816 |
| 16 | DBZBattleFinal | grid | 2 | 561283 | 554788 | 98 | 6334 | 43672 |
| 17 | Crosshair Pulse 2D | grid | 2 | 549339 | 542899 | 74 | 6329 | 48528 |
| 18 | Blue Holiday Candle 2D | grid | 2 | 532626 | 526147 | 99 | 6319 | 47668 |
| 19 | millipede 1d/2d controls | grid | 2 | 518455 | 511969 | 106 | 6317 | 47260 |
| 20 | 1D Aurora Borealis | strip | 2 | 502571 | 496114 | 94 | 6309 | 46272 |
| 21 | Metaballs of Fire 2D | grid | 3 | 489378 | 482912 | 82 | 6325 | 48528 |
| 22 | Coronal Ejection 2D | grid | 3 | 482549 | 476030 | 107 | 6334 | 44444 |
| 23 | Coronal Mass Ejection | grid | 3 | 475504 | 468966 | 113 | 6337 | 45936 |
| 24 | Blue Holiday Star 2D | grid | 3 | 457609 | 451092 | 115 | 6340 | 49196 |
| 25 | Ripples 2D | grid | 3 | 432235 | 425801 | 79 | 6309 | 49180 |
| 26 | Orv - Christmas Tree | grid | 3 | 427444 | 421071 | 71 | 6252 | 44080 |
| 27 | Eye of Sauron | grid | 3 | 420598 | 414106 | 104 | 6331 | 48292 |
| 28 | Eye of Sauron with movement | grid | 3 | 412768 | 406246 | 117 | 6345 | 48024 |
| 29 | perlin fire wind | grid | 3 | 412208 | 405685 | 113 | 6343 | 46576 |
| 30 | Time Flies 2D | grid | 3 | 394848 | 388379 | 94 | 6325 | 48428 |
| 31 | Carrie's Holiday Star 2D | grid | 3 | 391453 | 384915 | 118 | 6350 | 49180 |
| 32 | Utility: Palettes | strip | 3 | 381475 | 375035 | 107 | 6289 | 42624 |
| 33 | Beat Bounce | strip | 3 | 376796 | 370286 | 116 | 6331 | 49344 |
| 34 | Sunrise Alarm Clock | strip | 3 | 374794 | 368333 | 104 | 6295 | 43996 |
| 35 | Perlin fire | grid | 3 | 371281 | 364747 | 117 | 6354 | 47260 |
| 36 | Oasis | strip | 3 | 370878 | 364463 | 84 | 6289 | 45188 |
| 37 | Breathing Gradient | strip | 3 | 370909 | 364441 | 99 | 6311 | 49828 |
| 38 | Sun rays through trees | grid | 3 | 370638 | 364149 | 107 | 6330 | 47852 |
| 39 | 2d Clock with Hand Color Pickers | grid | 3 | 369861 | 363346 | 109 | 6351 | 46244 |
| 40 | Bouncing Balls RGB 2D | grid | 3 | 368202 | 361783 | 74 | 6302 | 46672 |
| 41 | Geometry Morphing Demo 2D | grid | 3 | 355140 | 348683 | 88 | 6312 | 47284 |
| 42 | Coral Plasma | grid | 3 | 350923 | 344445 | 108 | 6314 | 49352 |
| 43 | Emoji Animation #2 | grid | 3 | 345982 | 339542 | 83 | 6310 | 45516 |
| 44 | Rainstorm | grid | 3 | 345462 | 338956 | 90 | 6364 | 48720 |
| 45 | Scary Pumpkin | grid | 3 | 334828 | 328306 | 111 | 6351 | 48340 |
| 46 | 2D Bouncing Additive Primaries | grid | 4 | 326924 | 320459 | 89 | 6320 | 47104 |
| 47 | Animated Asterisks 2D | grid | 4 | 320408 | 313956 | 90 | 6301 | 48048 |
| 48 | spiral twirls star 2D | grid | 4 | 320117 | 313621 | 108 | 6332 | 46784 |
| 49 | perlin fire wind tunnel | grid | 4 | 301861 | 295388 | 112 | 6306 | 47680 |
| 50 | Interference 2D | grid | 4 | 301504 | 294997 | 116 | 6346 | 49512 |
| 51 | Post-Process Chain | strip | 4 | 295170 | 288745 | 92 | 6283 | 47280 |
| 52 | Lightning clouds | grid | 4 | 281016 | 274571 | 87 | 6310 | 48244 |
| 53 | sound - spectromatrix render2D | grid | 4 | 274409 | 267927 | 100 | 6332 | 46028 |
| 54 | Slime mold palette | grid | 4 | 269488 | 263010 | 101 | 6323 | 43076 |
| 55 | tixy | grid | 4 | 269407 | 262965 | 80 | 6314 | 46732 |
| 56 | angle and radius from coordinates | grid | 4 | 262381 | 255938 | 87 | 6306 | 48896 |
| 57 | DNA Helix 2D | grid | 4 | 249806 | 243371 | 89 | 6300 | 49888 |
| 58 | Bouncing RGB Balls - 2D | grid | 5 | 243157 | 236749 | 80 | 6292 | 49360 |
| 59 | Bouncer3D | grid | 5 | 242545 | 236130 | 69 | 6313 | 46860 |
| 60 | Raindrops 2D | grid | 5 | 241790 | 235395 | 73 | 6266 | 39596 |
| 61 | spotlights / rotation 3D | grid | 5 | 238825 | 232395 | 79 | 6311 | 48744 |
| 62 | xorcery 2D/3D | grid | 5 | 238314 | 231872 | 86 | 6316 | 48784 |
| 63 | Blinky Eyes 2D | grid | 5 | 235104 | 228676 | 78 | 6305 | 45812 |
| 64 | US Flag 2D | grid | 5 | 232584 | 226176 | 88 | 6276 | 47056 |
| 65 | multimap simpledemo | grid | 5 | 229757 | 223328 | 76 | 6302 | 49240 |
| 66 | Real World Lights | strip | 5 | 227706 | 221239 | 90 | 6337 | 47096 |
| 67 | 3D Rotation / Spotlights | cloud | 5 | 221879 | 215436 | 89 | 6324 | 49508 |
| 68 | radiant pulse 3 | grid | 5 | 205189 | 198765 | 80 | 6298 | 48276 |
| 69 | Soap 2D | grid | 5 | 203412 | 197047 | 65 | 6265 | 50272 |
| 70 | US Flag | strip | 5 | 203196 | 196802 | 88 | 6277 | 47928 |
| 71 | Continuous Cellular Automata | grid | 5 | 203081 | 196661 | 73 | 6303 | 44208 |
| 72 | Tunnel of Squares 2D | grid | 5 | 200932 | 194506 | 80 | 6294 | 49424 |
| 73 | Line Dancer 2D | grid | 6 | 199504 | 193084 | 90 | 6295 | 49144 |
| 74 | M5Stack Hex panels | cloud | 6 | 198306 | 191917 | 74 | 6272 | 48932 |
| 75 | firework nova | grid | 6 | 197501 | 191097 | 63 | 6300 | 48696 |
| 76 | SOUND - lavablob | grid | 6 | 197512 | 191055 | 81 | 6346 | 48760 |
| 77 | Halloween Wavy Bands | grid | 6 | 190737 | 184264 | 103 | 6323 | 48504 |
| 78 | 2D Spiral Twirls | grid | 6 | 187939 | 181547 | 70 | 6285 | 46628 |
| 79 | Sunset | strip | 6 | 187038 | 180637 | 75 | 6286 | 49908 |
| 80 | Spiral 2D | grid | 6 | 182796 | 176365 | 71 | 6322 | 49852 |
| 81 | Spinwheel 2D | grid | 6 | 180865 | 174473 | 63 | 6291 | 50296 |
| 82 | color bands | strip | 6 | 180505 | 174126 | 74 | 6271 | 49020 |
| 83 | Easing Library v1.0 | grid | 6 | 179880 | 173431 | 88 | 6317 | 48976 |
| 84 | Traffic | grid | 6 | 178957 | 172498 | 87 | 6322 | 48596 |
| 85 | Perlin/Simplex Noise 2D | grid | 6 | 175540 | 169082 | 96 | 6315 | 41816 |
| 86 | Matrix 2 tone pulse | strip | 6 | 174424 | 168015 | 82 | 6299 | 49456 |
| 87 | Ocean | strip | 6 | 174348 | 167989 | 68 | 6252 | 49812 |
| 88 | spin cycle | strip | 6 | 173883 | 167511 | 62 | 6278 | 48616 |
| 89 | Wavy Bands | grid | 6 | 172960 | 166517 | 96 | 6303 | 48980 |
| 90 | Crossfading | strip | 6 | 172209 | 165768 | 92 | 6302 | 47396 |
| 91 | Light Organ -- sensor board | strip | 6 | 170209 | 163734 | 86 | 6344 | 46068 |
| 92 | Multisegment Demo | strip | 6 | 169727 | 163325 | 77 | 6280 | 40740 |
| 93 | Aurora 2D | grid | 6 | 167631 | 161212 | 90 | 6290 | 49956 |
| 94 | green ripple reflections | strip | 6 | 166989 | 160603 | 70 | 6293 | 48912 |
| 95 | Shimmer Crossfade 2D | grid | 7 | 165345 | 158933 | 78 | 6292 | 47916 |
| 96 | Halloween color twinkles | strip | 7 | 161607 | 155232 | 63 | 6294 | 50164 |
| 97 | static random colors | strip | 7 | 160222 | 153878 | 69 | 6251 | 50404 |
| 98 | glitch bands | strip | 7 | 158236 | 151851 | 81 | 6274 | 50168 |
| 99 | Polar mapping helper 2D / 3D | grid | 7 | 157054 | 150647 | 70 | 6304 | 48628 |
| 100 | opposites | strip | 7 | 156144 | 149705 | 65 | 6331 | 48480 |
| 101 | heart | grid | 7 | 155600 | 149148 | 94 | 6317 | 49276 |
| 102 | Color Twinkles | strip | 7 | 153282 | 146885 | 72 | 6302 | 48796 |
| 103 | Infinity Flower 2D | grid | 7 | 153073 | 146656 | 69 | 6311 | 48508 |
| 104 | Digital Rain 2D | grid | 7 | 149945 | 143575 | 60 | 6279 | 49548 |
| 105 | matrix 2D pulse edit | strip | 7 | 146187 | 139782 | 71 | 6303 | 50148 |
| 106 | Doom Fire | grid | 8 | 141923 | 135510 | 68 | 6311 | 43056 |
| 107 | TwoColorHSVMix | strip | 8 | 140516 | 134159 | 58 | 6273 | 47180 |
| 108 | sinpulse 3D | grid | 8 | 139824 | 133421 | 74 | 6306 | 48828 |
| 109 | Upward waves 3D using accelerometer | cloud | 8 | 139656 | 133253 | 68 | 6295 | 49404 |
| 110 | Bessel Chaos | strip | 8 | 136327 | 129912 | 80 | 6304 | 49896 |
| 111 | Grinch's Heist | grid | 8 | 136115 | 129700 | 76 | 6304 | 42860 |
| 112 | marching rainbow | strip | 8 | 134831 | 128437 | 63 | 6309 | 49184 |
| 113 | Complements 3D | grid | 8 | 131251 | 124924 | 55 | 6242 | 48856 |
| 114 | fractal flower | grid | 8 | 131207 | 124774 | 93 | 6297 | 40016 |
| 115 | color fade pulse | strip | 8 | 128018 | 121629 | 73 | 6289 | 49236 |
| 116 | Stairmaster 2D | grid | 8 | 126177 | 119779 | 62 | 6301 | 50272 |
| 117 | block reflections | strip | 8 | 126049 | 119680 | 64 | 6282 | 49068 |
| 118 | sinus | grid | 8 | 125668 | 119267 | 65 | 6307 | 48492 |
| 119 | Infinite Snake | grid | 8 | 124684 | 118266 | 77 | 6306 | 28828 |
| 120 | Ember Diffusion | strip | 9 | 121185 | 114787 | 65 | 6303 | 17136 |
| 121 | matrix 2D honeycomb | grid | 9 | 120808 | 114446 | 62 | 6284 | 50136 |
| 122 | Nyan Lights | grid | 9 | 117787 | 111404 | 64 | 6284 | 38688 |
| 123 | Palette Fire 2D | grid | 9 | 117735 | 111347 | 63 | 6291 | 50276 |
| 124 | Breakout 2D | grid | 9 | 116136 | 109699 | 66 | 6333 | 45564 |
| 125 | Rock sparks | grid | 9 | 116006 | 109537 | 65 | 6307 | 46128 |
| 126 | Doom Fire 2D | grid | 9 | 115497 | 109114 | 59 | 6295 | 42528 |
| 127 | slow color shift | strip | 9 | 113304 | 106932 | 65 | 6282 | 49236 |
| 128 | tree setup pattern | grid | 9 | 112521 | 106121 | 63 | 6299 | 49920 |
| 129 | Color Blend | strip | 9 | 112031 | 105695 | 58 | 6251 | 48128 |
| 130 | Reaction Diffusion 2D | grid | 9 | 111600 | 105278 | 58 | 6240 | 40804 |
| 131 | Curl Flow 2D | grid | 9 | 111567 | 105119 | 97 | 6313 | 43488 |
| 132 | MidpointDisplacement1D | strip | 9 | 111429 | 105093 | 66 | 6245 | 46336 |
| 133 | fast pulse 3d | grid | 9 | 110905 | 104509 | 76 | 6294 | 48100 |
| 134 | Ice Floes 2D | grid | 10 | 108520 | 102198 | 59 | 6229 | 41168 |
| 135 | Bouncy Boxes | grid | 10 | 108444 | 102072 | 59 | 6282 | 39668 |
| 136 | Rainbow Melt | strip | 10 | 107953 | 101637 | 51 | 6246 | 49168 |
| 137 | millipede | strip | 10 | 107201 | 100856 | 49 | 6283 | 49188 |
| 138 | Sierpinski Rainbow 2D | grid | 10 | 106611 | 100292 | 51 | 6250 | 49060 |
| 139 | color twinkle bounce | strip | 10 | 104621 | 98257 | 62 | 6278 | 48264 |
| 140 | Spinning Plasma 2D | grid | 10 | 102468 | 96130 | 74 | 6222 | 50088 |
| 141 | Angry Xmass 3D | cloud | 10 | 101312 | 94953 | 56 | 6281 | 50292 |
| 142 | Sound - Spectrum Analyser | grid | 10 | 100013 | 93617 | 67 | 6300 | 48176 |
| 143 | Rainbow rocket sparks | strip | 11 | 99755 | 93373 | 59 | 6301 | 50444 |
| 144 | 2D Wandering Fireball | grid | 11 | 94215 | 87846 | 54 | 6291 | 48968 |
| 145 | Tetrix 2D | grid | 11 | 93914 | 87569 | 44 | 6275 | 48812 |
| 146 | firework rocket sparks | strip | 11 | 93089 | 86675 | 56 | 6331 | 48808 |
| 147 | rainbow fonts | strip | 11 | 92895 | 86550 | 54 | 6272 | 49312 |
| 148 | ChristmasStretch | strip | 11 | 91902 | 85561 | 56 | 6259 | 50204 |
| 149 | fast pulse | strip | 12 | 90328 | 83956 | 53 | 6295 | 50560 |
| 150 | sound - spectromatrix agc | grid | 12 | 89901 | 83509 | 61 | 6305 | 44092 |
| 151 | Glitter | strip | 12 | 87991 | 81579 | 61 | 6323 | 49884 |
| 152 | regenbogendrogen | strip | 12 | 87472 | 81128 | 54 | 6267 | 49184 |
| 153 | KITT (w/ color picker) | strip | 12 | 86738 | 80356 | 59 | 6300 | 16792 |
| 154 | Single Color Picker - wide or spot | strip | 12 | 85769 | 79427 | 64 | 6254 | 49544 |
| 155 | Color Pick Fade | strip | 12 | 85177 | 78804 | 52 | 6302 | 49084 |
| 156 | Stacker | strip | 12 | 83932 | 77570 | 51 | 6288 | 47696 |
| 157 | snake | strip | 13 | 82829 | 76456 | 51 | 6303 | 48412 |
| 158 | Example: time and animation | strip | 13 | 82336 | 76016 | 52 | 6249 | 48644 |
| 159 | Cylon | strip | 13 | 82246 | 75895 | 47 | 6286 | 16504 |
| 160 | Christmas Lights | strip | 13 | 82210 | 75875 | 51 | 6266 | 48612 |
| 161 | Flow Field 2D | grid | 13 | 81395 | 74960 | 86 | 6320 | 45076 |
| 162 | Thunderstorm | strip | 13 | 80693 | 74376 | 51 | 6243 | 49500 |
| 163 | KITT | strip | 13 | 80149 | 73790 | 43 | 6298 | 17620 |
| 164 | Cyclic Cellular Automata 2D | grid | 13 | 77992 | 71644 | 52 | 6273 | 43604 |
| 165 | Sunrise 2D | grid | 13 | 77953 | 71560 | 60 | 6310 | 41772 |
| 166 | Falling Sand 2D | grid | 13 | 77913 | 71546 | 55 | 6287 | 47488 |
| 167 | pixelClock | strip | 13 | 77851 | 71472 | 56 | 6302 | 48232 |
| 168 | 2D canvas example | grid | 13 | 77788 | 71399 | 65 | 6307 | 46016 |
| 169 | Matrix Green Waterfall 2D | grid | 13 | 77336 | 71007 | 44 | 6272 | 50084 |
| 170 | Boids 2D | grid | 13 | 77165 | 70777 | 63 | 6301 | 46332 |
| 171 | RGB-XYZ 3D Sweep | cloud | 13 | 76897 | 70533 | 56 | 6285 | 50224 |
| 172 | matrix rain | grid | 14 | 76107 | 69726 | 56 | 6292 | 49124 |
| 173 | Gradient blue  purple pink | strip | 14 | 76017 | 69670 | 58 | 6264 | 49300 |
| 174 | Spirograph 2D | grid | 14 | 75620 | 69253 | 59 | 6292 | 47828 |
| 175 | Holiday_Diagonal_Stripes | grid | 14 | 75517 | 69165 | 49 | 6282 | 50160 |
| 176 | scrolling text marquee 2D | grid | 14 | 75475 | 69101 | 60 | 6287 | 41248 |
| 177 | Golden Tix | strip | 14 | 75309 | 68937 | 65 | 6277 | 49212 |
| 178 | Bouncing Balls 2D | grid | 14 | 71829 | 65476 | 51 | 6273 | 45292 |
| 179 | Lissajous curve tracer | grid | 15 | 70468 | 64126 | 57 | 6264 | 42280 |
| 180 | Swirlpool 2D | grid | 15 | 67228 | 60843 | 54 | 6309 | 44596 |
| 181 | Unstable Orbits 2D | grid | 15 | 67177 | 60792 | 63 | 6304 | 44732 |
| 182 | Starfield 2D | grid | 16 | 66098 | 59738 | 56 | 6286 | 46668 |
| 183 | Edgeburst | strip | 16 | 66050 | 59682 | 55 | 6291 | 50496 |
| 184 | Pendulum Wave | strip | 16 | 65635 | 59326 | 57 | 6239 | 50496 |
| 185 | Example: modes and waveforms | strip | 16 | 65470 | 59152 | 48 | 6251 | 47540 |
| 186 | wanderers | grid | 16 | 64645 | 58302 | 46 | 6279 | 47336 |
| 187 | Accelerometer level example | grid | 16 | 64325 | 57965 | 50 | 6281 | 49956 |
| 188 | Sunrise | strip | 16 | 63043 | 56630 | 56 | 6304 | 49696 |
| 189 | Rainbow Smiley | grid | 17 | 61716 | 55389 | 47 | 6263 | 47688 |
| 190 | Mapping Helper Single and 10x | strip | 17 | 61432 | 55076 | 50 | 6286 | 49704 |
| 191 | Performance test framework | strip | 17 | 60933 | 54572 | 59 | 6282 | 49496 |
| 192 | rainbow pinwheel | strip | 17 | 60255 | 53934 | 46 | 6260 | 48692 |
| 193 | 2 Colors | strip | 17 | 59520 | 53197 | 41 | 6271 | 49340 |
| 194 | Christmas Candy Cane | strip | 17 | 59027 | 52668 | 51 | 6288 | 49236 |
| 195 | Example: color hues | strip | 18 | 58573 | 52258 | 42 | 6258 | 47912 |
| 196 | Typing Heatmap 2D | grid | 18 | 57674 | 51291 | 60 | 6291 | 44816 |
| 197 | Rainbow Flag | strip | 18 | 55892 | 49542 | 51 | 6281 | 50408 |
| 198 | ChristmasLights | strip | 19 | 54608 | 48295 | 45 | 6255 | 49080 |
| 199 | Three Red Pixels (array) | strip | 19 | 54011 | 47687 | 46 | 6264 | 17480 |
| 200 | Drip | strip | 19 | 53042 | 46675 | 55 | 6292 | 17100 |
| 201 | twinkly stars | strip | 19 | 52804 | 46497 | 44 | 6249 | 16412 |
| 202 | Marquee Chase | strip | 20 | 51746 | 45429 | 43 | 6263 | 49460 |
| 203 | 3 color rotation | strip | 20 | 50229 | 43933 | 42 | 6244 | 49692 |
| 204 | TV Simulator | strip | 21 | 47957 | 41679 | 46 | 6217 | 50120 |
| 205 | Chevron 2D | grid | 22 | 46942 | 40644 | 43 | 6245 | 50748 |
| 206 | policeLights | strip | 23 | 44551 | 38200 | 42 | 6299 | 50368 |
| 207 | Marching Dots | strip | 23 | 44495 | 38193 | 43 | 6247 | 48536 |
| 208 | Static Christmas Lights - 4 Colors | strip | 23 | 44340 | 38025 | 38 | 6269 | 50164 |
| 209 | Fast Palette Blending | strip | 24 | 42503 | 36213 | 50 | 6219 | 45016 |
| 210 | RGBW Mapping Tester | strip | 24 | 42403 | 36051 | 43 | 6298 | 50496 |
| 211 | Lightbulb - Crank Hue to Complete | strip | 24 | 42171 | 35776 | 54 | 6290 | 47868 |
| 212 | b_lightning_flashes | strip | 24 | 42007 | 35650 | 47 | 6291 | 48784 |
| 213 | SaberDeploy Tutorial | strip | 26 | 39765 | 33408 | 44 | 6299 | 49480 |
| 214 | RGBW Mapping Tester - HSV Version | strip | 26 | 38392 | 32063 | 43 | 6276 | 50496 |
| 215 | Rainbow | strip | 29 | 34650 | 28355 | 41 | 6245 | 50760 |
| 216 | Rainbow v2 | strip | 30 | 34194 | 27896 | 40 | 6249 | 49084 |
| 217 | A Peak Integrator | strip | 32 | 31633 | 25279 | 45 | 6292 | 46520 |
| 218 | Example: Smooth Speed Slider | strip | 33 | 30875 | 24570 | 41 | 6253 | 50220 |
| 219 | Example - Button w/ debounce | strip | 37 | 27738 | 21384 | 40 | 6298 | 49224 |
| 220 | Sound Reactive Color Fade | grid | 37 | 27006 | 20667 | 41 | 6290 | 46680 |
| 221 | 1 White Fade | strip | 42 | 24114 | 17880 | 38 | 6189 | 50624 |
| 222 | 2 Purple Fade | strip | 42 | 24153 | 17865 | 38 | 6243 | 50572 |
| 223 | NaturalLightSync | strip | 44 | 23068 | 16812 | 43 | 6198 | 49740 |
| 224 | UtilityColorTemp | strip | 44 | 22972 | 16714 | 42 | 6204 | 49856 |

## Not measured

| pattern | kind | why |
|---|---|---|
| _Fairies | strip | indexing a non-array value |
| 2D Fireworks Fade | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| 4th | strip | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| amoeba | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Audio Volume Meter | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| aurorashivers | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Autumn Colors | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Blink Fade | strip | indexing a non-array value |
| bouncing balls - hsv | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| bouncing balls - rgb | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Bubble Column | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| bustle | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Cellular Automata 1D | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Chasing Rainbows & HSLuv | strip | array memory budget exceeded (pattern too large for this device) |
| chill confetti | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Christmas RG Fade | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| ChristmasPewPew | strip | feedback of a non-array |
| color bands (buffered) | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| colourful fireflies | strip | feedback of a non-array |
| Comets | strip | feedback of a non-array |
| coolaura | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Custom Sequences | strip | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| distance function kaleidoscope 2 | grid | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| fire - blue | strip | array memory budget exceeded (pattern too large for this device) |
| fire - red | strip | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| fireblobs | strip | array index out of bounds |
| Fireflies | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| firework dust | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Fireworks Finale | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Flash Posterize + Music Sequencer framework | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Frogger 2D | grid | push failed: TypeError: fetch failed |
| GlowFlow (3D coord transform API port) | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| heatshivers | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Light Organ - 2.0 | strip | rejected: not enough free memory on the device for this 14 KB upload (about 14 KB free) — it is too large to run here |
| Lightning Strike | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Main Stage | grid | rejected: not enough free memory on the device for this 41 KB upload (about 46 KB free) — it is too large to run here |
| marching rainbow (buffered) | strip | indexing a non-array value |
| Matrix Green Waterfall 1D | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Meteor Shower | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Nano Orbital | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| neutronorbit | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Newfire | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| novas | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Opening Act | grid | call of a non-function value |
| Perlin/Simplex Noise 1D | strip | indexing a non-array value |
| Pew-Pew-Pew! | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| portal | strip | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| quiet blinkfade | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Rainbow Comet | strip | indexing a non-array value |
| Rocket by Tony Hampton | strip | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| Scanner | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| scrolls | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| slowflies | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - blinkfade | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - rays | strip | indexing a non-array value |
| sound - rays Frequency-BPM Reactive 1 | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - spectro kalidastrip | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - spectroblots - pow fade | grid | indexing a non-array value |
| sound - spectrokalidamandala | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - Starburst 2 | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Sound & Music Spectrum Visualizer | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| sparkfire | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| sparks | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| sparks center | strip | pattern too large for this device — it left only 10 KB of heap free (the firmware needs 20 KB to keep running) |
| Spring Colors | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| StarGen polar 2D | grid | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Synchronized Random Numbers | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Twinkle | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| twinkle (2) | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Twinkling Classic Xmas Strands | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| wanderedges | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| White Rainbows | strip | indexing a non-array value |
| Wichmann–Hill PRNG | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| XmasFlies | strip | feedback of a non-array |
| zoom kaleidoscope | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |


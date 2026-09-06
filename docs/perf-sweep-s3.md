# Pattern performance sweep — 2026-09-06

*Device 192.168.0.238 (Seengreat HUB75 S3), master `0f83975`
(`board-seengreat-hub75`, `CORE_O3=1`, superinstructions on, both OTA slots
carrying the same image), 4096 px, brightness 31 (see #297 — the board put
itself there mid-run; it does not affect timing).*
*Regenerate: `node tools/hw-bench.mjs <ip> <report.md> --perf-only`.*

- 225 of 299 gallery patterns measured (74 rejected or faulted — at 4096 px
  most of those are arrays that no longer fit; they are listed at the end).
- vm µs/frame: median **111211**, p90 **382809**, max 1423293.
- frame µs: median 116940, p90 388644. fps: median **9**, p10 3.

| # | pattern | kind | fps | frame µs | vm µs | pipe µs | out µs | heap free |
|---:|---|---|---:|---:|---:|---:|---:|---:|
| 1 | Crosstown Traffic 2D | grid | 2 | 1429547 | 1423293 | 193 | 5952 | 47396 |
| 2 | Dire Spider 2D | grid | 1 | 1410989 | 1405121 | 126 | 5676 | 44188 |
| 3 | Crawling Spider 2D | grid | 1 | 1071958 | 1066063 | 119 | 5714 | 45948 |
| 4 | Butterfly 2D | grid | 2 | 862552 | 856668 | 123 | 5703 | 47040 |
| 5 | Glittering Jewels | strip | 2 | 834326 | 828437 | 129 | 5652 | 45844 |
| 6 | RYB colors | grid | 2 | 781423 | 775579 | 116 | 5661 | 48112 |
| 7 | fire - blue | strip | 2 | 646801 | 640966 | 101 | 5689 | 11336 |
| 8 | Crosshair Pulse 2D | grid | 2 | 574654 | 568720 | 111 | 5774 | 48704 |
| 9 | 2D sinc(theta)/theta | grid | 2 | 554477 | 548785 | 107 | 5538 | 48796 |
| 10 | Radar 2D | grid | 2 | 546739 | 540875 | 107 | 5699 | 49508 |
| 11 | RGBclock 2D | grid | 2 | 542755 | 536905 | 112 | 5679 | 46388 |
| 12 | Kaleidoscope 2D | grid | 2 | 531510 | 525803 | 119 | 5529 | 46060 |
| 13 | All Lasers Fire | grid | 2 | 525661 | 519815 | 104 | 5697 | 49400 |
| 14 | 80s kid show | grid | 2 | 504816 | 499033 | 116 | 5609 | 41896 |
| 15 | Voronoi 2D | grid | 3 | 476171 | 470402 | 98 | 5615 | 48056 |
| 16 | millipede 1d/2d controls | grid | 3 | 455749 | 449898 | 114 | 5683 | 47032 |
| 17 | DBZBattleFinal | grid | 3 | 428804 | 422928 | 115 | 5722 | 42552 |
| 18 | Blue Holiday Candle 2D | grid | 3 | 423788 | 417988 | 92 | 5661 | 47512 |
| 19 | Blue Holiday Star 2D | grid | 3 | 418076 | 412225 | 107 | 5697 | 49472 |
| 20 | 1D Aurora Borealis | strip | 3 | 413911 | 408145 | 100 | 5621 | 45992 |
| 21 | Metaballs of Fire 2D | grid | 3 | 409747 | 403891 | 107 | 5703 | 48748 |
| 22 | Coronal Ejection 2D | grid | 3 | 396233 | 390376 | 116 | 5691 | 44364 |
| 23 | Coronal Mass Ejection | grid | 3 | 388644 | 382809 | 107 | 5678 | 45892 |
| 24 | Breathing Gradient | strip | 3 | 377250 | 371503 | 106 | 5597 | 50212 |
| 25 | Eye of Sauron | grid | 3 | 370426 | 364591 | 103 | 5694 | 48596 |
| 26 | Beat Bounce | strip | 3 | 366429 | 360584 | 109 | 5682 | 49640 |
| 27 | Ripples 2D | grid | 3 | 364179 | 358472 | 80 | 5584 | 49420 |
| 28 | Eye of Sauron with movement | grid | 3 | 363835 | 357968 | 105 | 5704 | 48220 |
| 29 | perlin fire wind | grid | 3 | 360415 | 354538 | 109 | 5710 | 46580 |
| 30 | Carrie's Holiday Star 2D | grid | 3 | 345786 | 339959 | 103 | 5687 | 49376 |
| 31 | Sunrise Alarm Clock | strip | 3 | 342944 | 337181 | 99 | 5612 | 43712 |
| 32 | Orv - Christmas Tree | grid | 3 | 340976 | 335230 | 91 | 5606 | 44140 |
| 33 | Utility: Palettes | strip | 3 | 339215 | 333487 | 93 | 5582 | 41316 |
| 34 | 2d Clock with Hand Color Pickers | grid | 4 | 325249 | 319460 | 96 | 5659 | 46152 |
| 35 | Sun rays through trees | grid | 4 | 324491 | 318657 | 104 | 5677 | 48100 |
| 36 | Perlin fire | grid | 4 | 320891 | 315015 | 110 | 5710 | 47356 |
| 37 | Mandelbrot 2D | grid | 4 | 317979 | 312381 | 79 | 5496 | 50156 |
| 38 | Time Flies 2D | grid | 4 | 312091 | 306258 | 98 | 5694 | 48476 |
| 39 | Rainstorm | grid | 4 | 302850 | 297073 | 92 | 5640 | 48876 |
| 40 | Oasis | strip | 4 | 299464 | 293741 | 97 | 5586 | 45216 |
| 41 | Scary Pumpkin | grid | 4 | 298887 | 293011 | 119 | 5701 | 48532 |
| 42 | Emoji Animation #2 | grid | 4 | 296699 | 290889 | 97 | 5671 | 45248 |
| 43 | Coral Plasma | grid | 4 | 294248 | 288473 | 92 | 5649 | 49556 |
| 44 | Interference 2D | grid | 4 | 293966 | 288114 | 105 | 5702 | 49736 |
| 45 | Bouncing Balls RGB 2D | grid | 4 | 287237 | 281444 | 93 | 5667 | 46440 |
| 46 | spiral twirls star 2D | grid | 4 | 272346 | 266502 | 106 | 5697 | 46784 |
| 47 | 2D Bouncing Additive Primaries | grid | 4 | 266293 | 260534 | 84 | 5648 | 47180 |
| 48 | perlin fire wind tunnel | grid | 4 | 257838 | 252069 | 98 | 5620 | 48116 |
| 49 | Geometry Morphing Demo 2D | grid | 4 | 254794 | 248952 | 89 | 5713 | 47116 |
| 50 | Animated Asterisks 2D | grid | 5 | 239904 | 234109 | 92 | 5664 | 48252 |
| 51 | Post-Process Chain | strip | 5 | 237849 | 232053 | 99 | 5661 | 47368 |
| 52 | Lightning clouds | grid | 5 | 228790 | 223018 | 93 | 5640 | 48528 |
| 53 | angle and radius from coordinates | grid | 5 | 227012 | 221251 | 89 | 5648 | 49156 |
| 54 | Bouncing RGB Balls - 2D | grid | 5 | 225208 | 219474 | 69 | 5640 | 49508 |
| 55 | sound - spectromatrix render2D | grid | 5 | 224576 | 218664 | 93 | 5787 | 46236 |
| 56 | DNA Helix 2D | grid | 5 | 217603 | 211857 | 78 | 5636 | 50160 |
| 57 | tixy | grid | 5 | 206723 | 200954 | 77 | 5658 | 46824 |
| 58 | spotlights / rotation 3D | grid | 5 | 205649 | 199842 | 90 | 5679 | 48788 |
| 59 | xorcery 2D/3D | grid | 5 | 202796 | 197022 | 77 | 5672 | 49040 |
| 60 | Real World Lights | strip | 6 | 199526 | 193745 | 73 | 5687 | 46936 |
| 61 | US Flag 2D | grid | 6 | 199231 | 193567 | 67 | 5567 | 47132 |
| 62 | Slime mold palette | grid | 6 | 198192 | 192411 | 78 | 5671 | 43468 |
| 63 | Raindrops 2D | grid | 6 | 193231 | 187517 | 95 | 5579 | 39460 |
| 64 | Bouncer3D | grid | 6 | 190042 | 184258 | 80 | 5677 | 46940 |
| 65 | 3D Rotation / Spotlights | cloud | 6 | 186618 | 180833 | 79 | 5676 | 49556 |
| 66 | Blinky Eyes 2D | grid | 6 | 184909 | 179134 | 81 | 5665 | 47620 |
| 67 | multimap simpledemo | grid | 6 | 180868 | 175105 | 87 | 5652 | 49508 |
| 68 | Continuous Cellular Automata | grid | 6 | 179681 | 173887 | 80 | 5685 | 44072 |
| 69 | radiant pulse 3 | grid | 6 | 179139 | 173380 | 83 | 5649 | 48612 |
| 70 | Line Dancer 2D | grid | 6 | 177378 | 171629 | 88 | 5623 | 49432 |
| 71 | cube fire 3D | grid | 6 | 177336 | 171519 | 91 | 5692 | 48236 |
| 72 | US Flag | strip | 6 | 175950 | 170302 | 66 | 5564 | 48044 |
| 73 | SOUND - lavablob | grid | 6 | 176068 | 170290 | 83 | 5672 | 49068 |
| 74 | Soap 2D | grid | 6 | 174838 | 169146 | 79 | 5583 | 50716 |
| 75 | M5Stack Hex panels | cloud | 6 | 170035 | 164213 | 102 | 5683 | 49140 |
| 76 | firework nova | grid | 6 | 168661 | 162880 | 70 | 5684 | 48992 |
| 77 | Tunnel of Squares 2D | grid | 7 | 164692 | 158976 | 79 | 5614 | 49740 |
| 78 | Halloween Wavy Bands | grid | 7 | 163917 | 158167 | 84 | 5638 | 48676 |
| 79 | color bands | strip | 7 | 162525 | 156842 | 72 | 5585 | 49300 |
| 80 | 2D Spiral Twirls | grid | 7 | 160605 | 154836 | 82 | 5657 | 48608 |
| 81 | Sunset | strip | 7 | 160547 | 154826 | 79 | 5617 | 50236 |
| 82 | Spiral 2D | grid | 7 | 158803 | 153030 | 93 | 5649 | 50236 |
| 83 | Easing Library v1.0 | grid | 7 | 157938 | 152144 | 89 | 5674 | 48932 |
| 84 | Crossfading | strip | 7 | 156269 | 150596 | 87 | 5559 | 47420 |
| 85 | Spinwheel 2D | grid | 7 | 154812 | 149048 | 80 | 5663 | 50664 |
| 86 | Multisegment Demo | strip | 7 | 153154 | 147407 | 84 | 5631 | 39832 |
| 87 | Wavy Bands | grid | 7 | 151953 | 146210 | 92 | 5621 | 49312 |
| 88 | Aurora 2D | grid | 7 | 148721 | 142962 | 92 | 5632 | 50412 |
| 89 | Perlin/Simplex Noise 2D | grid | 7 | 147817 | 142026 | 96 | 5667 | 41740 |
| 90 | Traffic | grid | 7 | 146628 | 140836 | 81 | 5678 | 48872 |
| 91 | green ripple reflections | strip | 7 | 146388 | 140694 | 59 | 5616 | 49220 |
| 92 | Matrix 2 tone pulse | strip | 7 | 146399 | 140655 | 76 | 5648 | 49760 |
| 93 | Ocean | strip | 7 | 146227 | 140593 | 70 | 5538 | 50312 |
| 94 | Shimmer Crossfade 2D | grid | 7 | 143580 | 137825 | 81 | 5642 | 47948 |
| 95 | spin cycle | strip | 7 | 142620 | 136933 | 66 | 5598 | 48956 |
| 96 | Halloween color twinkles | strip | 8 | 140175 | 134372 | 83 | 5690 | 50520 |
| 97 | glitch bands | strip | 8 | 139480 | 133834 | 74 | 5550 | 50520 |
| 98 | Polar mapping helper 2D / 3D | grid | 8 | 137073 | 131308 | 68 | 5676 | 48760 |
| 99 | Color Twinkles | strip | 8 | 135958 | 130205 | 69 | 5664 | 47368 |
| 100 | opposites | strip | 8 | 130505 | 124789 | 65 | 5630 | 48816 |
| 101 | Digital Rain 2D | grid | 8 | 129169 | 123446 | 71 | 5628 | 49856 |
| 102 | Infinity Flower 2D | grid | 8 | 128838 | 123065 | 79 | 5676 | 48624 |
| 103 | Light Organ - 2.0 | strip | 8 | 128379 | 122585 | 84 | 5681 | 44580 |
| 104 | sinpulse 3D | grid | 8 | 128278 | 122571 | 64 | 5627 | 49116 |
| 105 | static random colors | strip | 8 | 126672 | 121085 | 64 | 5508 | 50808 |
| 106 | heart | grid | 8 | 126598 | 120870 | 78 | 5625 | 49588 |
| 107 | Light Organ -- sensor board | strip | 8 | 126044 | 120243 | 75 | 5698 | 45984 |
| 108 | matrix 2D pulse edit | strip | 9 | 124357 | 118607 | 74 | 5652 | 50488 |
| 109 | marching rainbow | strip | 9 | 121140 | 115431 | 61 | 5635 | 49548 |
| 110 | Complements 3D | grid | 9 | 120325 | 114656 | 65 | 5589 | 49124 |
| 111 | TwoColorHSVMix | strip | 9 | 118518 | 112840 | 68 | 5584 | 49208 |
| 112 | Upward waves 3D using accelerometer | cloud | 9 | 117624 | 111850 | 81 | 5664 | 49632 |
| 113 | Bessel Chaos | strip | 9 | 116940 | 111211 | 77 | 5628 | 50196 |
| 114 | Doom Fire | grid | 9 | 114676 | 108846 | 82 | 5718 | 43100 |
| 115 | Grinch's Heist | grid | 9 | 112932 | 107184 | 81 | 5645 | 42384 |
| 116 | color fade pulse | strip | 9 | 111962 | 106268 | 62 | 5619 | 49520 |
| 117 | Stairmaster 2D | grid | 9 | 110960 | 105251 | 69 | 5621 | 50624 |
| 118 | block reflections | strip | 10 | 109017 | 103320 | 58 | 5623 | 49332 |
| 119 | matrix 2D honeycomb | grid | 10 | 108389 | 102666 | 62 | 5643 | 50492 |
| 120 | Ember Diffusion | strip | 10 | 108167 | 102417 | 62 | 5668 | 17496 |
| 121 | sinus | grid | 10 | 105658 | 99921 | 56 | 5663 | 48808 |
| 122 | fractal flower | grid | 10 | 105268 | 99464 | 84 | 5695 | 40004 |
| 123 | Rainbow Melt | strip | 10 | 101611 | 95948 | 69 | 5575 | 49532 |
| 124 | Curl Flow 2D | grid | 10 | 101318 | 95580 | 88 | 5619 | 43728 |
| 125 | Palette Fire 2D | grid | 10 | 101278 | 95531 | 71 | 5660 | 50796 |
| 126 | fast pulse 3d | grid | 10 | 100722 | 94997 | 75 | 5636 | 48380 |
| 127 | Doom Fire 2D | grid | 10 | 100420 | 94635 | 81 | 5677 | 42592 |
| 128 | Infinite Snake | grid | 10 | 99978 | 94217 | 79 | 5657 | 28304 |
| 129 | Color Blend | strip | 11 | 99127 | 93480 | 62 | 5566 | 48388 |
| 130 | Sierpinski Rainbow 2D | grid | 11 | 98777 | 93149 | 56 | 5560 | 49380 |
| 131 | Breakout 2D | grid | 11 | 98121 | 92405 | 77 | 5615 | 45632 |
| 132 | slow color shift | strip | 11 | 97117 | 91466 | 57 | 5581 | 49560 |
| 133 | tree setup pattern | grid | 11 | 97081 | 91361 | 63 | 5643 | 50296 |
| 134 | color twinkle bounce | strip | 11 | 94931 | 89276 | 55 | 5587 | 48548 |
| 135 | millipede | strip | 11 | 93760 | 88043 | 60 | 5639 | 49532 |
| 136 | Reaction Diffusion 2D | grid | 11 | 91891 | 86201 | 58 | 5616 | 40968 |
| 137 | MidpointDisplacement1D | strip | 11 | 91779 | 86116 | 61 | 5585 | 46456 |
| 138 | Rock sparks | grid | 11 | 91659 | 85861 | 70 | 5672 | 45988 |
| 139 | Nyan Lights | grid | 12 | 89981 | 84243 | 65 | 5649 | 38684 |
| 140 | Ice Floes 2D | grid | 12 | 88488 | 82820 | 86 | 5551 | 41256 |
| 141 | Tetrix 2D | grid | 12 | 86885 | 81172 | 59 | 5641 | 49036 |
| 142 | Spinning Plasma 2D | grid | 12 | 85319 | 79673 | 82 | 5541 | 50488 |
| 143 | Bouncy Boxes | grid | 12 | 85063 | 79335 | 78 | 5631 | 39336 |
| 144 | Sound - Spectrum Analyser | grid | 12 | 82935 | 77157 | 77 | 5681 | 48368 |
| 145 | quiet blinkfade | strip | 13 | 82606 | 76888 | 48 | 5654 | 16352 |
| 146 | 2D Wandering Fireball | grid | 13 | 81464 | 75764 | 51 | 5636 | 49272 |
| 147 | rainbow fonts | strip | 13 | 81029 | 75344 | 53 | 5617 | 49696 |
| 148 | Angry Xmass 3D | cloud | 13 | 80709 | 75003 | 57 | 5632 | 50684 |
| 149 | firework rocket sparks | strip | 13 | 80734 | 75003 | 56 | 5657 | 49140 |
| 150 | Rainbow rocket sparks | strip | 13 | 79205 | 73452 | 56 | 5677 | 50852 |
| 151 | Single Color Picker - wide or spot | strip | 13 | 77810 | 72154 | 59 | 5590 | 49928 |
| 152 | fast pulse | strip | 13 | 77518 | 71811 | 58 | 5632 | 50952 |
| 153 | Color Pick Fade | strip | 14 | 76058 | 70314 | 57 | 5669 | 49420 |
| 154 | ChristmasStretch | strip | 14 | 75151 | 69483 | 53 | 5602 | 50544 |
| 155 | regenbogendrogen | strip | 14 | 74750 | 69029 | 64 | 5639 | 49768 |
| 156 | Example: time and animation | strip | 14 | 74489 | 68865 | 64 | 5543 | 48720 |
| 157 | Flow Field 2D | grid | 14 | 74178 | 68390 | 77 | 5683 | 45132 |
| 158 | 2D canvas example | grid | 14 | 73806 | 68042 | 64 | 5682 | 46584 |
| 159 | Glitter | strip | 14 | 72966 | 67306 | 58 | 5584 | 50280 |
| 160 | sound - spectromatrix agc | grid | 14 | 72325 | 66577 | 64 | 5665 | 44260 |
| 161 | Boids 2D | grid | 15 | 68712 | 62932 | 78 | 5682 | 46408 |
| 162 | Cyclic Cellular Automata 2D | grid | 15 | 68269 | 62582 | 60 | 5608 | 43552 |
| 163 | KITT | strip | 15 | 67968 | 62254 | 47 | 5657 | 18016 |
| 164 | Stacker | strip | 15 | 67546 | 61813 | 51 | 5666 | 47896 |
| 165 | Spirograph 2D | grid | 15 | 67263 | 61521 | 68 | 5662 | 48128 |
| 166 | Sunrise 2D | grid | 15 | 67203 | 61467 | 53 | 5665 | 41560 |
| 167 | Golden Tix | strip | 15 | 67121 | 61438 | 58 | 5608 | 49588 |
| 168 | Christmas Lights | strip | 16 | 66305 | 60617 | 54 | 5623 | 48836 |
| 169 | Gradient blue  purple pink | strip | 16 | 65992 | 60458 | 55 | 5468 | 49676 |
| 170 | Holiday_Diagonal_Stripes | grid | 16 | 66129 | 60430 | 58 | 5625 | 50552 |
| 171 | snake | strip | 16 | 64896 | 59187 | 48 | 5652 | 48736 |
| 172 | Thunderstorm | strip | 16 | 64652 | 58982 | 55 | 5599 | 49828 |
| 173 | Matrix Green Waterfall 2D | grid | 16 | 64639 | 58925 | 56 | 5639 | 50480 |
| 174 | Falling Sand 2D | grid | 16 | 63968 | 58237 | 59 | 5655 | 47688 |
| 175 | pixelClock | strip | 16 | 63262 | 57609 | 53 | 5587 | 48552 |
| 176 | scrolling text marquee 2D | grid | 16 | 63191 | 57467 | 55 | 5655 | 40920 |
| 177 | matrix rain | grid | 17 | 61132 | 55421 | 51 | 5651 | 49416 |
| 178 | Unstable Orbits 2D | grid | 17 | 60757 | 55008 | 63 | 5672 | 45012 |
| 179 | Pendulum Wave | strip | 17 | 59833 | 54230 | 46 | 5549 | 50896 |
| 180 | RGB-XYZ 3D Sweep | cloud | 17 | 59671 | 53971 | 50 | 5636 | 50588 |
| 181 | Example: modes and waveforms | strip | 17 | 59196 | 53591 | 54 | 5539 | 47656 |
| 182 | Bouncing Balls 2D | grid | 17 | 58827 | 53159 | 66 | 5588 | 45348 |
| 183 | Starfield 2D | grid | 18 | 58368 | 52629 | 69 | 5654 | 46920 |
| 184 | Lissajous curve tracer | grid | 18 | 57753 | 52114 | 57 | 5568 | 42324 |
| 185 | rainbow pinwheel | strip | 19 | 55248 | 49583 | 50 | 5605 | 48864 |
| 186 | Edgeburst | strip | 19 | 55273 | 49572 | 52 | 5635 | 50912 |
| 187 | wanderers | grid | 19 | 54893 | 49189 | 51 | 5642 | 47656 |
| 188 | Typing Heatmap 2D | grid | 19 | 54799 | 49114 | 63 | 5607 | 45196 |
| 189 | Swirlpool 2D | grid | 19 | 54665 | 48898 | 60 | 5691 | 44804 |
| 190 | Accelerometer level example | grid | 19 | 53152 | 47414 | 67 | 5650 | 50296 |
| 191 | Sunrise | strip | 19 | 53015 | 47225 | 49 | 5710 | 50036 |
| 192 | Example: color hues | strip | 20 | 52372 | 46607 | 52 | 5704 | 48028 |
| 193 | Mapping Helper Single and 10x | strip | 20 | 51752 | 46038 | 48 | 5653 | 49720 |
| 194 | 2 Colors | strip | 20 | 51633 | 45993 | 42 | 5591 | 49832 |
| 195 | Rainbow Smiley | grid | 20 | 50831 | 45277 | 49 | 5496 | 48044 |
| 196 | Christmas Candy Cane | strip | 21 | 49150 | 43508 | 46 | 5587 | 49600 |
| 197 | Performance test framework | strip | 21 | 49013 | 43423 | 53 | 5528 | 49780 |
| 198 | Marquee Chase | strip | 22 | 46438 | 40708 | 48 | 5671 | 49844 |
| 199 | ChristmasLights | strip | 22 | 46155 | 40470 | 48 | 5627 | 49392 |
| 200 | TV Simulator | strip | 22 | 45896 | 40292 | 46 | 5544 | 50480 |
| 201 | Drip | strip | 23 | 45159 | 39442 | 55 | 5653 | 17380 |
| 202 | twinkly stars | strip | 23 | 44489 | 39020 | 47 | 5414 | 16768 |
| 203 | Three Red Pixels (array) | strip | 24 | 43369 | 37904 | 42 | 5417 | 17864 |
| 204 | Rainbow Flag | strip | 24 | 43188 | 37552 | 43 | 5587 | 50784 |
| 205 | Lightbulb - Crank Hue to Complete | strip | 25 | 40962 | 35273 | 48 | 5610 | 47896 |
| 206 | Chevron 2D | grid | 25 | 40525 | 34984 | 41 | 5491 | 51172 |
| 207 | 3 color rotation | strip | 25 | 40261 | 34664 | 46 | 5540 | 48340 |
| 208 | Marching Dots | strip | 26 | 39239 | 33772 | 45 | 5414 | 48892 |
| 209 | Fast Palette Blending | strip | 27 | 37672 | 32084 | 46 | 5533 | 45104 |
| 210 | policeLights | strip | 27 | 37563 | 31881 | 46 | 5627 | 50784 |
| 211 | Static Christmas Lights - 4 Colors | strip | 28 | 36226 | 30574 | 44 | 5601 | 50548 |
| 212 | b_lightning_flashes | strip | 29 | 35440 | 29729 | 43 | 5659 | 49036 |
| 213 | SaberDeploy Tutorial | strip | 30 | 34198 | 28372 | 45 | 5772 | 49828 |
| 214 | RGBW Mapping Tester | strip | 30 | 33537 | 27727 | 38 | 5767 | 50928 |
| 215 | Rainbow v2 | strip | 32 | 32176 | 26594 | 42 | 5532 | 49440 |
| 216 | RGBW Mapping Tester - HSV Version | strip | 32 | 31730 | 26097 | 39 | 5589 | 50928 |
| 217 | Rainbow | strip | 34 | 29735 | 24146 | 43 | 5540 | 51208 |
| 218 | A Peak Integrator | strip | 37 | 27208 | 21487 | 51 | 5661 | 46528 |
| 219 | Example: Smooth Speed Slider | strip | 38 | 26916 | 21335 | 39 | 5536 | 50620 |
| 220 | Example - Button w/ debounce | strip | 41 | 24715 | 18887 | 41 | 5773 | 49584 |
| 221 | Sound Reactive Color Fade | grid | 42 | 23900 | 18122 | 41 | 5730 | 46784 |
| 222 | 2 Purple Fade | strip | 44 | 22718 | 17097 | 43 | 5571 | 50992 |
| 223 | 1 White Fade | strip | 45 | 22526 | 16948 | 42 | 5529 | 51044 |
| 224 | NaturalLightSync | strip | 46 | 21709 | 16036 | 45 | 5621 | 50048 |
| 225 | UtilityColorTemp | strip | 47 | 21567 | 15933 | 45 | 5582 | 50200 |

## Not measured

| pattern | kind | why |
|---|---|---|
| _Fairies | strip | indexing a non-array value |
| 2D Fireworks Fade | grid | pattern too large for this device — it left only 11 KB of heap free (the firmware needs 20 KB to keep running) |
| 4th | strip | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| amoeba | strip | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| Audio Volume Meter | strip | indexing a non-array value |
| aurorashivers | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Autumn Colors | strip | indexing a non-array value |
| Blink Fade | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| bouncing balls - hsv | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| bouncing balls - rgb | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Bubble Column | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| bustle | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Cellular Automata 1D | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Chasing Rainbows & HSLuv | strip | array memory budget exceeded (pattern too large for this device) |
| chill confetti | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Christmas RG Fade | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| ChristmasPewPew | strip | feedback of a non-array |
| color bands (buffered) | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| colourful fireflies | strip | feedback of a non-array |
| Comets | strip | feedback of a non-array |
| coolaura | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Custom Sequences | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Cylon | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| distance function kaleidoscope 2 | grid | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| fire - red | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| fireblobs | strip | array index out of bounds |
| Fireflies | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| firework dust | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Fireworks Finale | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Flash Posterize + Music Sequencer framework | grid | call of a non-function value |
| Frogger 2D | grid | rejected: not enough free memory on the device for this 35 KB upload (about 37 KB free) — it is too large to run here |
| GlowFlow (3D coord transform API port) | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| heatshivers | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| KITT (w/ color picker) | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| Lightning Strike | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Main Stage | grid | rejected: not enough free memory on the device for this 45 KB upload (about 61 KB free) — it is too large to run here |
| marching rainbow (buffered) | strip | indexing a non-array value |
| Matrix Green Waterfall 1D | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Meteor Shower | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| Nano Orbital | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| neutronorbit | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Newfire | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| novas | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Opening Act | grid | call of a non-function value |
| Perlin/Simplex Noise 1D | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Pew-Pew-Pew! | strip | feedback of a non-array |
| portal | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| Rainbow Comet | strip | indexing a non-array value |
| Rocket by Tony Hampton | strip | pattern too large for this device — it left only 17 KB of heap free (the firmware needs 20 KB to keep running) |
| Scanner | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| scrolls | strip | feedback of a non-array |
| slowflies | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - blinkfade | strip | indexing a non-array value |
| sound - rays | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - rays Frequency-BPM Reactive 1 | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - spectro kalidastrip | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - spectroblots - pow fade | grid | indexing a non-array value |
| sound - spectrokalidamandala | grid | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| sound - Starburst 2 | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Sound & Music Spectrum Visualizer | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| sparkfire | strip | pattern too large for this device — it left only 15 KB of heap free (the firmware needs 20 KB to keep running) |
| sparks | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| sparks center | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| Spring Colors | strip | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| StarGen polar 2D | grid | pattern too large for this device — it left only 12 KB of heap free (the firmware needs 20 KB to keep running) |
| Synchronized Random Numbers | strip | pattern too large for this device — it left only 16 KB of heap free (the firmware needs 20 KB to keep running) |
| Twinkle | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| twinkle (2) | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Twinkling Classic Xmas Strands | strip | pattern too large for this device — it left only 13 KB of heap free (the firmware needs 20 KB to keep running) |
| wanderedges | strip | pattern too large for this device — it left only 19 KB of heap free (the firmware needs 20 KB to keep running) |
| White Rainbows | strip | pattern too large for this device — it left only 14 KB of heap free (the firmware needs 20 KB to keep running) |
| Wichmann–Hill PRNG | strip | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |
| XmasFlies | strip | feedback of a non-array |
| zoom kaleidoscope | grid | pattern too large for this device — it left only 18 KB of heap free (the firmware needs 20 KB to keep running) |

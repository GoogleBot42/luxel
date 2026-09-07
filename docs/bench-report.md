# Hardware soak + benchmark — 2026-09-07

*Device 192.168.0.183, firmware v0.1.40, 60 px ws2812, brightness 4.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 305 patterns: **305 clean**, 0 with errors, 4 under 30 fps.
- fps at 60 px (the count the sweep ran at): median **123**, p10 114, p90 123.
- lowest heap_free seen while soaking: 59668 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 124 |
| 150 | 124 |
| 300 | 81 |
| 600 | 42 |
| 1024 | 25 |
| 2048 | 13 |

## Slowest (< 30 fps at 60 px)

| pattern | kind | fps |
|---|---|---:|
| Slime mold palette | grid | 8 |
| fractal flower | grid | 21 |
| Ice Floes 2D | grid | 25 |
| Bouncy Boxes | grid | 28 |

## All results

| pattern | kind | fps | frame µs | vm µs | pipe µs | out µs |
|---|---|---:|---:|---:|---:|---:|
| _Fairies | strip | 123 | 3682 | 1049 | 36 | 2535 |
| 1 White Fade | strip | 124 | 3006 | 372 | 44 | 2530 |
| 1D Aurora Borealis | strip | 123 | 7641 | 4929 | 37 | 2606 |
| 2 Colors | strip | 124 | 3335 | 715 | 38 | 2521 |
| 2 Purple Fade | strip | 124 | 3024 | 382 | 44 | 2536 |
| 2D Bouncing Additive Primaries | grid | 123 | 6081 | 3376 | 37 | 2605 |
| 2D canvas example | grid | 123 | 4011 | 1325 | 45 | 2571 |
| 2d Clock with Hand Color Pickers | grid | 98 | 10129 | 7399 | 41 | 2608 |
| 2D Fireworks Fade | grid | 122 | 5471 | 2766 | 36 | 2600 |
| 2D sinc(theta)/theta | grid | 110 | 8981 | 6348 | 39 | 2526 |
| 2D Spiral Twirls | grid | 123 | 4829 | 2164 | 38 | 2565 |
| 2D Wandering Fireball | grid | 123 | 3976 | 1285 | 48 | 2582 |
| 3 color rotation | strip | 123 | 3269 | 627 | 37 | 2544 |
| 3D Rotation / Spotlights | cloud | 123 | 5152 | 2436 | 53 | 2596 |
| 4th | strip | 123 | 5120 | 2411 | 38 | 2602 |
| 80s kid show | grid | 103 | 9604 | 6928 | 40 | 2563 |
| A Peak Integrator | strip | 123 | 3646 | 946 | 37 | 2591 |
| Accelerometer level example | grid | 123 | 3599 | 930 | 39 | 2564 |
| All Lasers Fire | grid | 104 | 9496 | 6861 | 39 | 2529 |
| amoeba | strip | 123 | 5808 | 3053 | 37 | 2649 |
| angle and radius from coordinates | grid | 123 | 5802 | 3126 | 36 | 2570 |
| Angry Xmass 3D | cloud | 123 | 4028 | 1349 | 44 | 2572 |
| Animated Asterisks 2D | grid | 123 | 6151 | 3466 | 45 | 2570 |
| Audio Volume Meter | strip | 123 | 4563 | 1841 | 44 | 2611 |
| Aurora 2D | grid | 123 | 4950 | 2307 | 38 | 2544 |
| aurorashivers | strip | 123 | 5560 | 2755 | 49 | 2675 |
| Autumn Colors | strip | 123 | 3345 | 717 | 37 | 2529 |
| b_lightning_flashes | strip | 123 | 3401 | 734 | 37 | 2568 |
| Beat Bounce | strip | 114 | 8598 | 5912 | 42 | 2572 |
| Bessel Chaos | strip | 123 | 4359 | 1668 | 45 | 2586 |
| Blink Fade | strip | 123 | 3820 | 1157 | 44 | 2557 |
| Blinky Eyes 2D | grid | 123 | 6340 | 3600 | 37 | 2641 |
| block reflections | strip | 123 | 4332 | 1650 | 45 | 2575 |
| Blue Holiday Candle 2D | grid | 84 | 11757 | 8984 | 46 | 2646 |
| Blue Holiday Star 2D | grid | 122 | 7998 | 5341 | 34 | 2549 |
| Boids 2D | grid | 123 | 4979 | 2171 | 60 | 2662 |
| Bouncer3D | grid | 123 | 5419 | 2714 | 42 | 2597 |
| bouncing balls - hsv | strip | 123 | 3643 | 956 | 37 | 2577 |
| bouncing balls - rgb | strip | 123 | 4202 | 1540 | 40 | 2560 |
| Bouncing Balls 2D | grid | 123 | 7314 | 4563 | 51 | 2635 |
| Bouncing Balls RGB 2D | grid | 123 | 7242 | 4537 | 35 | 2599 |
| Bouncing RGB Balls - 2D | grid | 123 | 5372 | 2698 | 41 | 2561 |
| Bouncy Boxes | grid | 28 | 36459 | 33755 | 45 | 2582 |
| Breakout 2D | grid | 123 | 4523 | 1800 | 46 | 2614 |
| Breathing Gradient | strip | 109 | 9056 | 6383 | 50 | 2555 |
| Bubble Column | strip | 123 | 7640 | 4961 | 46 | 2562 |
| Bulk Bouncing Balls 2D | grid | 123 | 3799 | 1068 | 45 | 2585 |
| Bulk Canvas Ripples 2D | grid | 122 | 7278 | 4501 | 34 | 2635 |
| Bulk Comet Trails | strip | 123 | 2903 | 252 | 38 | 2540 |
| Bulk Rainbow | strip | 123 | 2869 | 241 | 39 | 2513 |
| Bulk Sprite Scroll 2D | grid | 122 | 3159 | 487 | 34 | 2560 |
| bustle | strip | 123 | 4869 | 2078 | 49 | 2661 |
| Butterfly 2D | grid | 52 | 19154 | 16346 | 50 | 2681 |
| Carrie's Holiday Star 2D | grid | 123 | 7111 | 4451 | 38 | 2554 |
| Cellular Automata 1D | strip | 123 | 3416 | 748 | 37 | 2559 |
| Chasing Rainbows & HSLuv | strip | 123 | 3350 | 704 | 37 | 2542 |
| Chevron 2D | grid | 124 | 3270 | 626 | 45 | 2538 |
| chill confetti | strip | 123 | 3064 | 432 | 37 | 2531 |
| Christmas Candy Cane | strip | 123 | 3418 | 766 | 38 | 2551 |
| Christmas Lights | strip | 123 | 3612 | 984 | 37 | 2530 |
| Christmas RG Fade | strip | 123 | 3724 | 1079 | 37 | 2546 |
| ChristmasLights | strip | 123 | 3350 | 701 | 37 | 2551 |
| ChristmasPewPew | strip | 123 | 3475 | 797 | 37 | 2575 |
| ChristmasStretch | strip | 123 | 3764 | 1131 | 35 | 2537 |
| color bands | strip | 124 | 4561 | 1923 | 44 | 2534 |
| color bands (buffered) | strip | 123 | 5063 | 2411 | 45 | 2546 |
| Color Blend | strip | 123 | 3813 | 1178 | 37 | 2538 |
| color fade pulse | strip | 123 | 4294 | 1620 | 45 | 2565 |
| Color Pick Fade | strip | 124 | 3798 | 1150 | 43 | 2545 |
| color twinkle bounce | strip | 123 | 3947 | 1285 | 45 | 2556 |
| Color Twinkles | strip | 123 | 4362 | 1696 | 45 | 2557 |
| colourful fireflies | strip | 123 | 3722 | 948 | 51 | 2640 |
| Comets | strip | 123 | 3832 | 1101 | 47 | 2602 |
| Complements 3D | grid | 123 | 4549 | 1881 | 35 | 2558 |
| Continuous Cellular Automata | grid | 123 | 5445 | 2717 | 54 | 2605 |
| coolaura | strip | 123 | 5661 | 2895 | 50 | 2655 |
| Coral Plasma | grid | 123 | 7129 | 4447 | 44 | 2562 |
| Coronal Ejection 2D | grid | 110 | 8991 | 6295 | 42 | 2585 |
| Coronal Mass Ejection | grid | 111 | 8853 | 6149 | 38 | 2597 |
| Crawling Spider 2D | grid | 57 | 17648 | 14906 | 41 | 2625 |
| Crossfading | strip | 123 | 5242 | 2446 | 47 | 2676 |
| Crosshair Pulse 2D | grid | 119 | 7387 | 4672 | 51 | 2590 |
| Crosstown Traffic 2D | grid | 37 | 27405 | 24732 | 51 | 2555 |
| cube fire 3D | grid | 123 | 4796 | 2119 | 44 | 2564 |
| Curl Flow 2D | grid | 122 | 6811 | 3955 | 52 | 2724 |
| Custom Sequences | strip | 123 | 4958 | 2275 | 44 | 2577 |
| Cyclic Cellular Automata 2D | grid | 122 | 4057 | 1383 | 37 | 2576 |
| Cylon | strip | 123 | 3645 | 998 | 36 | 2551 |
| DBZBattleFinal | grid | 119 | 8199 | 5542 | 33 | 2548 |
| Digital Rain 2D | grid | 123 | 4830 | 2174 | 38 | 2552 |
| Dire Spider 2D | grid | 45 | 22067 | 19362 | 48 | 2586 |
| distance function kaleidoscope 2 | grid | 120 | 8197 | 5457 | 60 | 2602 |
| DNA Helix 2D | grid | 123 | 5839 | 3135 | 46 | 2594 |
| Doom Fire | grid | 118 | 5507 | 2834 | 42 | 2566 |
| Doom Fire 2D | grid | 121 | 5006 | 2314 | 40 | 2586 |
| Drip | strip | 123 | 3447 | 780 | 35 | 2561 |
| Easing Library v1.0 | grid | 123 | 3398 | 738 | 41 | 2553 |
| Edgeburst | strip | 123 | 3470 | 815 | 48 | 2546 |
| Ember Diffusion | strip | 123 | 4285 | 1643 | 35 | 2541 |
| Emoji Animation #2 | grid | 123 | 6702 | 3999 | 35 | 2593 |
| Example - Button w/ debounce | strip | 123 | 3211 | 418 | 44 | 2581 |
| Example: color hues | strip | 123 | 3469 | 816 | 36 | 2544 |
| Example: modes and waveforms | strip | 123 | 3612 | 929 | 33 | 2579 |
| Example: Smooth Speed Slider | strip | 124 | 3056 | 434 | 37 | 2523 |
| Example: time and animation | strip | 123 | 3801 | 1116 | 47 | 2564 |
| Eye of Sauron | grid | 122 | 7766 | 5079 | 59 | 2560 |
| Eye of Sauron with movement | grid | 122 | 7877 | 5112 | 60 | 2631 |
| Falling Sand 2D | grid | 123 | 4008 | 1342 | 41 | 2557 |
| Fast Palette Blending | strip | 123 | 3386 | 700 | 45 | 2578 |
| fast pulse | strip | 123 | 3727 | 1066 | 45 | 2555 |
| fast pulse 3d | grid | 123 | 3678 | 1004 | 45 | 2568 |
| fire - blue | strip | 123 | 3841 | 1177 | 45 | 2553 |
| fire - red | strip | 123 | 4080 | 1418 | 35 | 2558 |
| fireblobs | strip | 119 | 8255 | 5542 | 32 | 2603 |
| Fireflies | strip | 123 | 3684 | 961 | 42 | 2607 |
| firework dust | strip | 123 | 3628 | 986 | 37 | 2543 |
| firework nova | grid | 123 | 4994 | 2304 | 43 | 2576 |
| firework rocket sparks | strip | 123 | 3750 | 1084 | 44 | 2559 |
| Fireworks Finale | strip | 123 | 4239 | 1517 | 36 | 2615 |
| Flash Posterize + Music Sequencer framework | grid | 123 | 5620 | 2938 | 42 | 2570 |
| Flow Field 2D | grid | 123 | 4869 | 2147 | 46 | 2606 |
| fractal flower | grid | 21 | 49750 | 46982 | 47 | 2645 |
| Frogger 2D | grid | 123 | 5996 | 3298 | 37 | 2589 |
| Geometry Morphing Demo 2D | grid | 122 | 6427 | 3674 | 48 | 2629 |
| glitch bands | strip | 123 | 4630 | 1944 | 45 | 2577 |
| Glitter | strip | 123 | 3863 | 1184 | 46 | 2568 |
| Glittering Jewels | strip | 63 | 15902 | 13178 | 46 | 2611 |
| GlowFlow (3D coord transform API port) | grid | 123 | 5435 | 2608 | 53 | 2694 |
| Golden Tix | strip | 123 | 3635 | 988 | 35 | 2543 |
| Gradient blue  purple pink | strip | 123 | 3662 | 1014 | 44 | 2543 |
| green ripple reflections | strip | 123 | 4608 | 1954 | 44 | 2549 |
| Grinch's Heist | grid | 123 | 5831 | 3075 | 43 | 2637 |
| Halloween color twinkles | strip | 123 | 4539 | 1848 | 45 | 2584 |
| Halloween Wavy Bands | grid | 123 | 5196 | 2491 | 44 | 2583 |
| heart | grid | 123 | 4530 | 1839 | 46 | 2583 |
| heatshivers | strip | 123 | 4993 | 2198 | 44 | 2667 |
| Holiday_Diagonal_Stripes | grid | 123 | 3465 | 814 | 42 | 2547 |
| Ice Floes 2D | grid | 25 | 39808 | 37084 | 44 | 2606 |
| Infinite Snake | grid | 123 | 4054 | 1344 | 42 | 2585 |
| Infinite Snake v2 | grid | 122 | 3619 | 922 | 43 | 2562 |
| Infinity Flower 2D | grid | 123 | 4587 | 1870 | 38 | 2610 |
| Interference 2D | grid | 123 | 5623 | 2934 | 55 | 2574 |
| Kaleidoscope 2D | grid | 77 | 12837 | 10093 | 39 | 2636 |
| KITT | strip | 123 | 3637 | 966 | 45 | 2563 |
| KITT (w/ color picker) | strip | 123 | 3747 | 1088 | 37 | 2560 |
| Light Organ - 2.0 | strip | 123 | 4961 | 2220 | 44 | 2622 |
| Light Organ -- sensor board | strip | 123 | 5093 | 2373 | 39 | 2615 |
| Lightbulb - Crank Hue to Complete | strip | 123 | 3685 | 836 | 44 | 2639 |
| Lightning clouds | grid | 123 | 6075 | 3334 | 57 | 2605 |
| Lightning Strike | strip | 123 | 4591 | 1899 | 37 | 2591 |
| Line Dancer 2D | grid | 123 | 4866 | 2168 | 51 | 2581 |
| Lissajous curve tracer | grid | 83 | 11942 | 9256 | 41 | 2580 |
| M5Stack Hex panels | cloud | 123 | 5091 | 2370 | 49 | 2598 |
| Main Stage | grid | 123 | 3496 | 788 | 36 | 2600 |
| Mandelbrot 2D | grid | 123 | 7006 | 4331 | 44 | 2570 |
| Mapping Helper Single and 10x | strip | 123 | 3479 | 833 | 37 | 2548 |
| Marching Dots | strip | 124 | 3247 | 628 | 35 | 2524 |
| marching rainbow | strip | 123 | 3975 | 1326 | 46 | 2541 |
| marching rainbow (buffered) | strip | 123 | 4140 | 1476 | 45 | 2557 |
| Marquee Chase | strip | 123 | 3391 | 750 | 45 | 2533 |
| Matrix 2 tone pulse | strip | 123 | 4757 | 2078 | 45 | 2573 |
| matrix 2D honeycomb | grid | 123 | 4466 | 1789 | 42 | 2566 |
| matrix 2D pulse edit | strip | 123 | 4471 | 1778 | 47 | 2584 |
| Matrix Green Waterfall 1D | strip | 123 | 4187 | 1491 | 46 | 2583 |
| Matrix Green Waterfall 2D | grid | 123 | 3634 | 964 | 44 | 2564 |
| matrix rain | grid | 123 | 3734 | 1084 | 34 | 2557 |
| Metaballs of Fire 2D | grid | 123 | 7148 | 4448 | 46 | 2587 |
| Meteor Shower | strip | 123 | 3376 | 705 | 48 | 2558 |
| MidpointDisplacement1D | strip | 123 | 4042 | 1375 | 45 | 2560 |
| millipede | strip | 123 | 3834 | 1204 | 34 | 2534 |
| millipede 1d/2d controls | grid | 115 | 8560 | 5778 | 37 | 2663 |
| multimap simpledemo | grid | 123 | 5762 | 3080 | 44 | 2568 |
| Multisegment Demo | strip | 123 | 5176 | 2448 | 36 | 2617 |
| Nano Orbital | strip | 123 | 3232 | 578 | 35 | 2548 |
| NaturalLightSync | strip | 123 | 3262 | 540 | 45 | 2594 |
| neutronorbit | strip | 123 | 7028 | 4270 | 45 | 2633 |
| Newfire | strip | 123 | 3858 | 1191 | 37 | 2562 |
| novas | strip | 117 | 8340 | 5640 | 34 | 2598 |
| Nyan Lights | grid | 123 | 4053 | 1373 | 38 | 2575 |
| Oasis | strip | 123 | 6965 | 4233 | 51 | 2618 |
| Ocean | strip | 123 | 4539 | 1876 | 45 | 2556 |
| Opening Act | grid | 123 | 4203 | 1543 | 35 | 2556 |
| opposites | strip | 123 | 4307 | 1648 | 44 | 2554 |
| Orv - Christmas Tree | grid | 114 | 8687 | 5955 | 36 | 2619 |
| Palette Fire 2D | grid | 123 | 4250 | 1600 | 54 | 2530 |
| Pendulum Wave | strip | 123 | 3467 | 836 | 37 | 2532 |
| Performance test framework | strip | 123 | 4089 | 1422 | 35 | 2562 |
| Perlin fire | grid | 109 | 9075 | 6304 | 51 | 2634 |
| perlin fire wind | grid | 104 | 9497 | 6714 | 52 | 2648 |
| perlin fire wind tunnel | grid | 123 | 6465 | 3707 | 56 | 2613 |
| Perlin/Simplex Noise 1D | strip | 123 | 4454 | 1757 | 37 | 2591 |
| Perlin/Simplex Noise 2D | grid | 89 | 11184 | 8456 | 51 | 2602 |
| Pew-Pew-Pew! | strip | 123 | 4397 | 1694 | 37 | 2601 |
| pixelClock | strip | 123 | 3737 | 1035 | 45 | 2584 |
| Polar mapping helper 2D / 3D | grid | 123 | 5135 | 2386 | 48 | 2630 |
| policeLights | strip | 123 | 3244 | 597 | 38 | 2549 |
| portal | strip | 122 | 7046 | 4274 | 39 | 2661 |
| Post-Process Chain | strip | 123 | 6107 | 3398 | 45 | 2588 |
| quiet blinkfade | strip | 123 | 3828 | 1194 | 37 | 2539 |
| Radar 2D | grid | 114 | 8703 | 6070 | 42 | 2528 |
| radiant pulse 3 | grid | 123 | 5151 | 2448 | 52 | 2580 |
| Rainbow | strip | 124 | 3070 | 441 | 47 | 2521 |
| Rainbow Comet | strip | 123 | 3849 | 1177 | 43 | 2567 |
| Rainbow Flag | strip | 123 | 3262 | 636 | 35 | 2532 |
| rainbow fonts | strip | 123 | 3724 | 1058 | 45 | 2560 |
| Rainbow Melt | strip | 123 | 3888 | 1224 | 44 | 2559 |
| rainbow pinwheel | strip | 123 | 3397 | 753 | 44 | 2538 |
| Rainbow rocket sparks | strip | 123 | 3851 | 1196 | 43 | 2552 |
| Rainbow Smiley | grid | 124 | 3061 | 433 | 44 | 2523 |
| Rainbow v2 | strip | 123 | 3138 | 516 | 36 | 2525 |
| Raindrops 2D | grid | 109 | 6849 | 4165 | 38 | 2578 |
| Rainstorm | grid | 123 | 6402 | 3743 | 37 | 2552 |
| Reaction Diffusion 2D | grid | 40 | 24889 | 22210 | 44 | 2556 |
| Real World Lights | strip | 123 | 6146 | 3439 | 43 | 2588 |
| regenbogendrogen | strip | 123 | 3653 | 980 | 44 | 2566 |
| RGB-XYZ 3D Sweep | cloud | 123 | 3676 | 991 | 44 | 2579 |
| RGBclock 2D | grid | 78 | 12679 | 9904 | 47 | 2649 |
| RGBW Mapping Tester | strip | 123 | 3209 | 547 | 45 | 2554 |
| RGBW Mapping Tester - HSV Version | strip | 123 | 3118 | 469 | 46 | 2541 |
| Ripples 2D | grid | 123 | 7119 | 4468 | 37 | 2547 |
| Rock sparks | grid | 123 | 4678 | 1766 | 51 | 2676 |
| Rocket by Tony Hampton | strip | 123 | 3972 | 1185 | 34 | 2677 |
| RYB colors | grid | 75 | 13216 | 10499 | 56 | 2581 |
| SaberDeploy Tutorial | strip | 123 | 3214 | 576 | 38 | 2534 |
| Scanner | strip | 123 | 3972 | 1246 | 46 | 2608 |
| Scary Pumpkin | grid | 123 | 6021 | 3264 | 44 | 2635 |
| scrolling text marquee 2D | grid | 123 | 3898 | 1199 | 54 | 2577 |
| scrolls | strip | 123 | 6272 | 3524 | 41 | 2641 |
| Shimmer Crossfade 2D | grid | 123 | 4862 | 2065 | 52 | 2668 |
| Sierpinski Rainbow 2D | grid | 123 | 4090 | 1415 | 42 | 2564 |
| Single Color Picker - wide or spot | strip | 124 | 3683 | 1051 | 38 | 2529 |
| sinpulse 3D | grid | 123 | 4456 | 1769 | 42 | 2575 |
| sinus | grid | 123 | 4215 | 1534 | 45 | 2575 |
| Slime mold palette | grid | 8 | 129205 | 126430 | 56 | 2635 |
| slow color shift | strip | 123 | 3926 | 1271 | 44 | 2550 |
| slowflies | strip | 123 | 5115 | 2315 | 51 | 2669 |
| snake | strip | 123 | 3669 | 1000 | 44 | 2563 |
| Soap 2D | grid | 123 | 5429 | 2782 | 46 | 2536 |
| sound - blinkfade | strip | 123 | 4345 | 1673 | 44 | 2566 |
| SOUND - lavablob | grid | 123 | 4868 | 2190 | 42 | 2566 |
| sound - rays | strip | 123 | 3681 | 1024 | 44 | 2553 |
| sound - rays Frequency-BPM Reactive 1 | strip | 123 | 4149 | 1380 | 37 | 2666 |
| sound - spectro kalidastrip | strip | 123 | 5873 | 3138 | 54 | 2608 |
| sound - spectroblots - pow fade | grid | 120 | 7986 | 5202 | 53 | 2654 |
| sound - spectrokalidamandala | grid | 123 | 7047 | 4279 | 47 | 2647 |
| sound - spectromatrix agc | grid | 64 | 15465 | 12790 | 50 | 2553 |
| sound - spectromatrix render2D | grid | 123 | 6830 | 4102 | 52 | 2603 |
| Sound - Spectrum Analyser | grid | 123 | 4509 | 1785 | 53 | 2603 |
| sound - Starburst 2 | strip | 123 | 4479 | 1728 | 54 | 2624 |
| Sound & Music Spectrum Visualizer | strip | 123 | 5050 | 2308 | 46 | 2624 |
| Sound Reactive Color Fade | grid | 123 | 3058 | 391 | 38 | 2563 |
| sparkfire | strip | 123 | 5128 | 2445 | 46 | 2574 |
| sparks | strip | 123 | 3983 | 1310 | 37 | 2567 |
| sparks center | strip | 123 | 3847 | 1196 | 37 | 2553 |
| spin cycle | strip | 123 | 4470 | 1800 | 50 | 2556 |
| Spinning Plasma 2D | grid | 123 | 4063 | 1409 | 47 | 2544 |
| Spinwheel 2D | grid | 123 | 4783 | 2103 | 45 | 2574 |
| Spiral 2D | grid | 123 | 4717 | 2058 | 43 | 2557 |
| spiral twirls star 2D | grid | 101 | 9760 | 7038 | 40 | 2609 |
| Spirograph 2D | grid | 123 | 4024 | 1306 | 44 | 2603 |
| spotlights / rotation 3D | grid | 123 | 5599 | 2840 | 52 | 2626 |
| Spring Colors | strip | 123 | 3217 | 591 | 37 | 2526 |
| Stacker | strip | 123 | 3989 | 1234 | 47 | 2633 |
| Stairmaster 2D | grid | 123 | 4189 | 1506 | 45 | 2573 |
| Starfield 2D | grid | 123 | 4277 | 1582 | 37 | 2584 |
| StarGen polar 2D | grid | 123 | 5896 | 3179 | 43 | 2608 |
| Static Christmas Lights - 4 Colors | strip | 124 | 3150 | 532 | 37 | 2519 |
| static random colors | strip | 123 | 4479 | 1808 | 35 | 2566 |
| Sun rays through trees | grid | 123 | 7269 | 4530 | 63 | 2604 |
| Sunrise | strip | 123 | 3692 | 875 | 45 | 2606 |
| Sunrise 2D | grid | 123 | 3912 | 1249 | 37 | 2557 |
| Sunrise Alarm Clock | strip | 123 | 7644 | 4918 | 36 | 2614 |
| Sunset | strip | 123 | 4871 | 2185 | 45 | 2578 |
| Swirlpool 2D | grid | 123 | 3930 | 1198 | 53 | 2609 |
| Synchronized Random Numbers | strip | 50 | 20210 | 17559 | 41 | 2542 |
| Tetrix 2D | grid | 123 | 4232 | 1566 | 40 | 2559 |
| Three Red Pixels (array) | strip | 123 | 3331 | 690 | 34 | 2546 |
| Thunderstorm | strip | 123 | 3720 | 1036 | 44 | 2575 |
| Time Flies 2D | grid | 123 | 3161 | 477 | 48 | 2569 |
| tixy | grid | 123 | 5740 | 2963 | 40 | 2650 |
| Traffic | grid | 123 | 4881 | 2199 | 40 | 2576 |
| tree setup pattern | grid | 124 | 3439 | 811 | 38 | 2529 |
| Tunnel of Squares 2D | grid | 123 | 5114 | 2414 | 45 | 2592 |
| TV Simulator | strip | 123 | 3432 | 775 | 36 | 2558 |
| Twinkle | strip | 123 | 3958 | 1299 | 37 | 2555 |
| twinkle (2) | strip | 123 | 3626 | 980 | 35 | 2549 |
| Twinkling Classic Xmas Strands | strip | 122 | 5603 | 2886 | 37 | 2612 |
| twinkly stars | strip | 123 | 3360 | 717 | 37 | 2545 |
| TwoColorHSVMix | strip | 123 | 4435 | 1740 | 43 | 2583 |
| Typing Heatmap 2D | grid | 122 | 4699 | 1982 | 40 | 2594 |
| Unstable Orbits 2D | grid | 123 | 4335 | 1636 | 46 | 2583 |
| Upward waves 3D using accelerometer | cloud | 123 | 4432 | 1717 | 48 | 2602 |
| US Flag | strip | 123 | 5298 | 2530 | 53 | 2649 |
| US Flag 2D | grid | 123 | 5597 | 2854 | 45 | 2630 |
| Utility: Palettes | strip | 123 | 7374 | 4671 | 43 | 2570 |
| UtilityColorTemp | strip | 123 | 3199 | 501 | 45 | 2587 |
| Voronoi 2D | grid | 120 | 8214 | 5551 | 41 | 2556 |
| wanderedges | strip | 123 | 4753 | 2057 | 43 | 2582 |
| wanderers | grid | 123 | 5460 | 2801 | 37 | 2559 |
| Wavy Bands | grid | 123 | 5007 | 2329 | 36 | 2568 |
| White Rainbows | strip | 123 | 3967 | 1296 | 44 | 2564 |
| Wichmann–Hill PRNG | strip | 123 | 4567 | 1845 | 44 | 2605 |
| XmasFlies | strip | 123 | 4852 | 2170 | 39 | 2571 |
| xorcery 2D/3D | grid | 123 | 5475 | 2783 | 43 | 2579 |
| zoom kaleidoscope | grid | 123 | 6309 | 3532 | 60 | 2638 |

# Hardware soak + benchmark — 2026-11-02

*Device 192.168.0.142 (Seengreat unit 2, 2x2 chain), firmware = master 7c4d7704 (reads v0.1.40), 16384 px, brightness 10 — the first soak of the 2026-11-01 library overhaul (150 patterns). The 2026-09-06 report it replaces was unit 1 at 4096 px, before the JIT: 299 patterns, 115 errors, median 7 fps. The one reboot (after Color Sequence, CoreSw, 1,897 ring DMA restarts) matches the #936 pattern; DMA restarts grew ~40–60 per pattern all through the run.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 150 patterns: **149 clean**, 1 with errors, 105 under 30 fps.
- fps at 16384 px (the count the sweep ran at): median **19**, p10 10, p90 42.
- lowest heap_free seen while soaking: 44988 bytes.
- **1 device crash** (0 unreachable after a push, **1 reboot** caught by the fences/slot oracle).
  - rebooted: at "color twinkle bounce" (since "Color Sequence"; reset Some(CoreSw), slot ota_1).

## fps vs pixel count (rainbow reference)

skipped (panel board).

## Errors

| pattern | kind | problem |
|---|---|---|
| color twinkle bounce | strip | rebooted (reset Some(CoreSw), slot ota_1) |

## Slowest (< 30 fps at 16384 px)

| pattern | kind | fps |
|---|---|---:|
| HUD Ticker | grid | 7 |
| Meadow | grid | 8 |
| Topo Map | grid | 8 |
| Lotus Bloom | grid | 9 |
| Oil Slick | grid | 9 |
| Portal | grid | 9 |
| Reaction Diffusion | grid | 9 |
| 80s Kid Show | grid | 10 |
| Interference | grid | 10 |
| Kaleidoscope | grid | 10 |
| Loop Ring | grid | 10 |
| Neon Veins | grid | 10 |
| Physarum | grid | 10 |
| Pool Caustics | grid | 10 |
| Pulse Train | grid | 10 |
| Thunderstorm | grid | 10 |
| Bokeh | grid | 11 |
| Dire Spider | grid | 11 |
| Geometry Morph | grid | 11 |
| Halftone | grid | 11 |
| Snowfall | grid | 11 |
| Spiral Twirls | grid | 11 |
| Wavy Bands | grid | 11 |
| Wireframe | grid | 11 |
| Cellular Automata | grid | 12 |
| Chladni Plate | grid | 12 |
| Jellyfish | grid | 12 |
| Julia Morph | grid | 12 |
| Light Thief | grid | 12 |
| Quasicrystal | grid | 12 |
| Spiral Galaxy | grid | 12 |
| Dot Globe | grid | 13 |
| Edge Burst | grid | 13 |
| Fractal Flower | grid | 13 |
| Lava Lamp | grid | 13 |
| Ocean | grid | 13 |
| Strange Attractor | grid | 13 |
| Night City | grid | 14 |
| Shadebobs | grid | 14 |
| Twister | grid | 14 |
| Aurora 2D | grid | 15 |
| Butterfly | grid | 15 |
| Coronal Mass Ejection | grid | 15 |
| Fire | grid | 15 |
| Fire Eye | grid | 15 |
| Fireflies | grid | 15 |
| Holiday Star | grid | 15 |
| Plucked String | grid | 15 |
| Pong Clock | grid | 15 |
| Rainbow Melt | grid | 15 |
| Raindrops | grid | 15 |
| Searchlights | grid | 15 |
| Soap | grid | 15 |
| Analog Clock | grid | 16 |
| Candle | grid | 16 |
| Classic Christmas Strands | strip | 16 |
| Color Sequence | strip | 16 |
| Copper Bars | grid | 16 |
| Meteor Shower | grid | 16 |
| Notification Ripple | grid | 16 |
| Rain on Glass | grid | 16 |
| Spectrum Field | grid | 16 |
| Voronoi | grid | 16 |
| Bouncing Columns | grid | 17 |
| Line Dancer | grid | 17 |
| Marquee | grid | 17 |
| Plasma | grid | 17 |
| Radiant Bloom | grid | 17 |
| Touch Glow | grid | 17 |
| Tunnel | grid | 17 |
| Wander Edges | grid | 17 |
| Tetrix | grid | 18 |
| Gears | grid | 19 |
| Glittering Jewels | grid | 19 |
| tixy | grid | 19 |
| Bessel Chaos | grid | 21 |
| Blinky Eyes | grid | 21 |
| DNA Helix | grid | 21 |
| Opposites | grid | 21 |
| Spectrum Analyzer | grid | 21 |
| God Rays | grid | 22 |
| Newton's Cradle | grid | 22 |
| Rotozoomer | grid | 22 |
| Saber | grid | 22 |
| Xorcery | grid | 22 |
| Candy Stripes | grid | 23 |
| Double Pendulum | grid | 23 |
| TV Simulator | grid | 23 |
| Curl Flow | grid | 24 |
| Fast Pulse | grid | 24 |
| Gravity Wells | grid | 24 |
| Seasons Tree | grid | 24 |
| Frogger | grid | 25 |
| Heart | grid | 25 |
| Police Lights | grid | 25 |
| Waving Flag | grid | 25 |
| Asteroids | grid | 26 |
| Energy Duel | grid | 26 |
| Pendulum Wave | grid | 26 |
| Starfield | grid | 26 |
| Rainbow | grid | 27 |
| Millipede | grid | 28 |
| Sunrise Alarm | strip | 28 |
| Bouncy Boxes | grid | 29 |
| Truchet Weave | grid | 29 |

## All results

| pattern | kind | fps | heap largest | jit | drops | frame µs | vm µs | pipe µs | out µs |
|---|---|---:|---:|---|---|---:|---:|---:|---:|
| _Fairies | any | 48 | 51168 | native | 0/0/0 (late 3482 torn 2222 dma_rs 706 lcd_rs 0 dma_err 0) | 6784 | 2380 | 22 | 13694 |
| 80s Kid Show | grid | 10 | 49860 | native | 1/0/0 (late 3530 torn 2259 dma_rs 745 lcd_rs 0 dma_err 0) | 102472 | 98557 | 116 | 15748 |
| Analog Clock | grid | 16 | 48560 | native | 1/0/0 (late 3602 torn 2305 dma_rs 791 lcd_rs 0 dma_err 0) | 62386 | 58348 | 110 | 17384 |
| Asteroids | grid | 26 | 48332 | native | 1/0/0 (late 3635 torn 2383 dma_rs 840 lcd_rs 0 dma_err 0) | 38273 | 33643 | 131 | 18928 |
| Aurora 2D | grid | 15 | 47928 | native | 1/0/0 (late 3691 torn 2449 dma_rs 902 lcd_rs 0 dma_err 0) | 69072 | 65413 | 113 | 16962 |
| Bessel Chaos | grid | 21 | 47460 | native | 1/0/0 (late 3705 torn 2486 dma_rs 938 lcd_rs 0 dma_err 0) | 46963 | 43338 | 98 | 15377 |
| Bit Rain | grid | 36 | 48796 | native | 1/0/0 (late 3793 torn 2517 dma_rs 1003 lcd_rs 0 dma_err 0) | 21721 | 17025 | 71 | 17138 |
| Blinky Eyes | grid | 21 | 49536 | native | 2/0/0 (late 3818 torn 2566 dma_rs 1079 lcd_rs 0 dma_err 0) | 46885 | 43420 | 85 | 15072 |
| Block Clock | grid | 30 | 49176 | native | 2/0/0 (late 3902 torn 2598 dma_rs 1149 lcd_rs 0 dma_err 0) | 32430 | 27865 | 108 | 17748 |
| Boids | grid | 36 | 50260 | native | 2/0/0 (late 3965 torn 2622 dma_rs 1212 lcd_rs 0 dma_err 0) | 25943 | 21509 | 102 | 15799 |
| Bokeh | grid | 11 | 50584 | native | 3/0/0 (late 4003 torn 2631 dma_rs 1253 lcd_rs 0 dma_err 0) | 96622 | 92387 | 114 | 17507 |
| Bouncing Balls | grid | 36 | 48812 | native | 3/0/0 (late 4061 torn 2664 dma_rs 1291 lcd_rs 0 dma_err 0) | 26795 | 22610 | 90 | 17479 |
| Bouncing Columns | grid | 17 | 51412 | native | 4/0/0 (late 4087 torn 2681 dma_rs 1344 lcd_rs 0 dma_err 0) | 59846 | 55800 | 91 | 16361 |
| Bouncy Boxes | grid | 29 | 50428 | native | 4/0/0 (late 4130 torn 2700 dma_rs 1383 lcd_rs 0 dma_err 0) | 33465 | 29323 | 88 | 17367 |
| Breakout | grid | 37 | 46968 | native | 4/0/0 (late 4210 torn 2824 dma_rs 1448 lcd_rs 0 dma_err 0) | 22598 | 18312 | 92 | 16256 |
| Bulk Sprite Scroll 2D | grid | 31 | 51616 | native | 5/0/0 (late 4217 torn 2867 dma_rs 1511 lcd_rs 0 dma_err 0) | 31386 | 27537 | 63 | 14069 |
| Butterfly | grid | 15 | 48540 | native | 5/0/0 (late 4271 torn 2933 dma_rs 1555 lcd_rs 0 dma_err 0) | 66077 | 61600 | 114 | 16737 |
| Candle | grid | 16 | 49464 | native | 5/0/0 (late 4334 torn 2972 dma_rs 1602 lcd_rs 0 dma_err 0) | 62576 | 58585 | 123 | 17497 |
| Candy Stripes | grid | 23 | 49552 | native | 5/0/0 (late 4357 torn 2999 dma_rs 1633 lcd_rs 0 dma_err 0) | 43900 | 40230 | 84 | 14566 |
| Cellular Automata | grid | 12 | 48788 | native | 5/0/0 (late 4390 torn 3043 dma_rs 1665 lcd_rs 0 dma_err 0) | 81787 | 77782 | 108 | 16567 |
| Chladni Plate | grid | 12 | 49912 | native | 5/0/0 (late 4432 torn 3085 dma_rs 1700 lcd_rs 0 dma_err 0) | 88137 | 83479 | 122 | 16598 |
| Chomp Chase | grid | 32 | 47308 | native | 5/0/0 (late 4472 torn 3188 dma_rs 1747 lcd_rs 0 dma_err 0) | 30118 | 25249 | 128 | 17193 |
| Circle Packing | grid | 46 | 49776 | native | 6/0/0 (late 4528 torn 3209 dma_rs 1819 lcd_rs 0 dma_err 0) | 10866 | 6575 | 49 | 14309 |
| Classic Christmas Strands | strip | 16 | 50328 | native | 7/0/0 (late 4582 torn 3232 dma_rs 1875 lcd_rs 0 dma_err 0) | 64270 | 60716 | 71 | 15981 |
| Color Sequence | strip | 16 | 48016 | native | 7/0/0 (late 4634 torn 3266 dma_rs 1897 lcd_rs 0 dma_err 0) | 64375 | 60854 | 0 | 0 |
| color twinkle bounce | strip | — | 46004 | native | 0/0/0 (late 11 torn 27 dma_rs 21 lcd_rs 0 dma_err 0) | — | — | — | — |
| Color Wash | grid | 39 | 41884 | native | 0/0/0 (late 39 torn 81 dma_rs 80 lcd_rs 0 dma_err 0) | 18896 | 14657 | 55 | 14395 |
| Copper Bars | grid | 16 | 44744 | native | 1/0/0 (late 105 torn 110 dma_rs 125 lcd_rs 0 dma_err 0) | 64890 | 61194 | 115 | 14937 |
| Coronal Mass Ejection | grid | 15 | 42676 | native | 1/0/0 (late 146 torn 195 dma_rs 158 lcd_rs 0 dma_err 0) | 65644 | 61271 | 109 | 15847 |
| Crosstown Traffic | grid | 42 | 44444 | native | 1/0/0 (late 178 torn 228 dma_rs 212 lcd_rs 0 dma_err 0) | 15961 | 11699 | 63 | 15089 |
| Crystal Growth | grid | 47 | 44372 | native | 2/0/0 (late 232 torn 285 dma_rs 277 lcd_rs 0 dma_err 0) | 9638 | 4991 | 43 | 13931 |
| Curl Flow | grid | 24 | 45304 | native | 3/0/0 (late 260 torn 324 dma_rs 328 lcd_rs 0 dma_err 0) | 41344 | 36924 | 176 | 18231 |
| Day Ribbon | any | 41 | 44828 | native | 3/0/0 (late 294 torn 403 dma_rs 426 lcd_rs 0 dma_err 0) | 16432 | 11592 | 57 | 14918 |
| Digital Rain | grid | 45 | 45232 | native | 4/0/0 (late 313 torn 449 dma_rs 503 lcd_rs 0 dma_err 0) | 9667 | 5670 | 48 | 14564 |
| Dire Spider | grid | 11 | 44168 | native | 5/0/0 (late 362 torn 517 dma_rs 552 lcd_rs 0 dma_err 0) | 94593 | 90862 | 122 | 17195 |
| DNA Helix | grid | 21 | 45348 | native | 5/0/0 (late 375 torn 562 dma_rs 592 lcd_rs 0 dma_err 0) | 47090 | 42823 | 115 | 18171 |
| Doom Fire | grid | 41 | 44584 | native | 5/0/0 (late 399 torn 590 dma_rs 652 lcd_rs 0 dma_err 0) | 16440 | 11910 | 63 | 15611 |
| Dot Globe | grid | 13 | 45804 | native | 7/0/0 (late 461 torn 630 dma_rs 717 lcd_rs 0 dma_err 0) | 77672 | 73488 | 106 | 17364 |
| Double Pendulum | grid | 23 | 45140 | native | 7/0/0 (late 495 torn 654 dma_rs 755 lcd_rs 0 dma_err 0) | 43751 | 39579 | 102 | 17205 |
| Edge Burst | grid | 13 | 44980 | native | 7/0/0 (late 525 torn 679 dma_rs 791 lcd_rs 0 dma_err 0) | 76729 | 73204 | 101 | 15839 |
| Embers | grid | 31 | 44396 | native | 7/0/0 (late 545 torn 715 dma_rs 836 lcd_rs 0 dma_err 0) | 31977 | 27506 | 108 | 17739 |
| Energy Duel | grid | 26 | 43692 | native | 7/0/0 (late 594 torn 818 dma_rs 894 lcd_rs 0 dma_err 0) | 37219 | 32864 | 110 | 18016 |
| Falling Sand | grid | 35 | 43752 | native | 8/0/0 (late 652 torn 863 dma_rs 941 lcd_rs 0 dma_err 0) | 27947 | 22863 | 91 | 16845 |
| Fast Pulse | grid | 24 | 45612 | native | 9/0/0 (late 688 torn 875 dma_rs 987 lcd_rs 0 dma_err 0) | 41102 | 37649 | 66 | 14486 |
| Fibonacci Clock | grid | 39 | 45492 | native | 9/0/0 (late 724 torn 921 dma_rs 1046 lcd_rs 0 dma_err 0) | 20258 | 15964 | 62 | 14963 |
| Fire | grid | 15 | 43220 | native | 10/0/0 (late 775 torn 983 dma_rs 1103 lcd_rs 0 dma_err 0) | 68615 | 64665 | 114 | 16599 |
| Fire Eye | grid | 15 | 45068 | native | 10/0/0 (late 860 torn 1017 dma_rs 1148 lcd_rs 0 dma_err 0) | 68075 | 63539 | 101 | 15263 |
| Fireflies | grid | 15 | 44848 | native | 10/0/0 (late 893 torn 1069 dma_rs 1190 lcd_rs 0 dma_err 0) | 65893 | 61991 | 115 | 18762 |
| Fireworks | grid | 35 | 42992 | native | 10/0/0 (late 934 torn 1162 dma_rs 1242 lcd_rs 0 dma_err 0) | 22682 | 18421 | 94 | 16247 |
| Fractal Flower | grid | 13 | 45384 | native | 11/0/0 (late 977 torn 1181 dma_rs 1281 lcd_rs 0 dma_err 0) | 75988 | 72150 | 92 | 18901 |
| Frogger | grid | 25 | 40892 | native | 11/0/0 (late 1040 torn 1354 dma_rs 1321 lcd_rs 0 dma_err 0) | 38908 | 34590 | 110 | 18382 |
| Gears | grid | 19 | 45800 | native | 11/0/0 (late 1065 torn 1391 dma_rs 1354 lcd_rs 0 dma_err 0) | 53927 | 50113 | 93 | 16930 |
| Geometry Morph | grid | 11 | 44356 | native | 11/0/0 (late 1105 torn 1405 dma_rs 1388 lcd_rs 0 dma_err 0) | 89723 | 86213 | 95 | 15947 |
| Glittering Jewels | grid | 19 | 45444 | native | 11/0/0 (late 1135 torn 1442 dma_rs 1428 lcd_rs 0 dma_err 0) | 53433 | 49504 | 98 | 17881 |
| GlowFlow | grid | 30 | 44032 | native | 11/0/0 (late 1176 torn 1496 dma_rs 1457 lcd_rs 0 dma_err 0) | 31969 | 27943 | 113 | 16789 |
| God Rays | grid | 22 | 45628 | native | 12/0/0 (late 1222 torn 1517 dma_rs 1505 lcd_rs 0 dma_err 0) | 45997 | 41851 | 112 | 15981 |
| Gravity Wells | grid | 24 | 44956 | native | 12/0/0 (late 1240 torn 1559 dma_rs 1562 lcd_rs 0 dma_err 0) | 40355 | 36022 | 111 | 18112 |
| Halftone | grid | 11 | 45804 | native | 12/0/0 (late 1274 torn 1596 dma_rs 1605 lcd_rs 0 dma_err 0) | 97348 | 93053 | 116 | 18919 |
| Heart | grid | 25 | 44112 | native | 12/0/0 (late 1295 torn 1607 dma_rs 1644 lcd_rs 0 dma_err 0) | 40186 | 36747 | 74 | 14460 |
| Holiday Star | grid | 15 | 45580 | native | 12/0/0 (late 1310 torn 1650 dma_rs 1676 lcd_rs 0 dma_err 0) | 66965 | 62743 | 112 | 15662 |
| HUD Ticker | grid | 7 | 45836 | native | 12/0/0 (late 1344 torn 1698 dma_rs 1721 lcd_rs 0 dma_err 0) | 159124 | 154459 | 130 | 19614 |
| Infinite Snake | grid | 39 | 42920 | native | 12/0/0 (late 1403 torn 1744 dma_rs 1782 lcd_rs 0 dma_err 0) | 16450 | 12252 | 83 | 15713 |
| Interference | grid | 10 | 44128 | native | 13/0/0 (late 1433 torn 1791 dma_rs 1846 lcd_rs 0 dma_err 0) | 108279 | 104833 | 110 | 16583 |
| Jellyfish | grid | 12 | 45328 | native | 13/0/0 (late 1456 torn 1825 dma_rs 1876 lcd_rs 0 dma_err 0) | 82666 | 78729 | 256 | 15432 |
| Julia Morph | grid | 12 | 44456 | native | 13/0/0 (late 1473 torn 1853 dma_rs 1905 lcd_rs 0 dma_err 0) | 87310 | 83188 | 97 | 16046 |
| Kaleidoscope | grid | 10 | 42564 | native | 13/0/0 (late 1513 torn 1920 dma_rs 1939 lcd_rs 0 dma_err 0) | 108927 | 105265 | 116 | 16511 |
| Lamp | grid | 47 | 43420 | native | 13/0/0 (late 1542 torn 1959 dma_rs 1980 lcd_rs 0 dma_err 0) | 7658 | 3746 | 20 | 13940 |
| Lava Lamp | grid | 13 | 43008 | native | 15/0/0 (late 1603 torn 2046 dma_rs 2012 lcd_rs 0 dma_err 0) | 77652 | 74025 | 126 | 15980 |
| Life | grid | 35 | 41584 | native | 15/0/0 (late 1634 torn 2106 dma_rs 2053 lcd_rs 0 dma_err 0) | 27470 | 22711 | 70 | 16501 |
| Light Organ | grid | 37 | 45136 | native | 16/0/0 (late 1678 torn 2167 dma_rs 2107 lcd_rs 0 dma_err 0) | 25294 | 20784 | 93 | 15868 |
| Light Thief | grid | 12 | 43356 | native | 17/0/0 (late 1715 torn 2247 dma_rs 2168 lcd_rs 0 dma_err 0) | 84163 | 80347 | 241 | 15951 |
| Line Dancer | grid | 17 | 45240 | native | 17/0/0 (late 1748 torn 2293 dma_rs 2213 lcd_rs 0 dma_err 0) | 59832 | 56606 | 76 | 15504 |
| Loop Ring | grid | 10 | 44724 | native | 17/0/0 (late 1786 torn 2328 dma_rs 2258 lcd_rs 0 dma_err 0) | 99886 | 95905 | 104 | 16285 |
| Lotus Bloom | grid | 9 | 44844 | native | 17/0/0 (late 1811 torn 2388 dma_rs 2285 lcd_rs 0 dma_err 0) | 113219 | 109261 | 123 | 17511 |
| Marbling | grid | 38 | 41544 | native | 17/0/0 (late 1824 torn 2422 dma_rs 2344 lcd_rs 0 dma_err 0) | 24621 | 19524 | 55 | 15158 |
| Marquee | grid | 17 | 43880 | native | 18/0/0 (late 1864 torn 2461 dma_rs 2392 lcd_rs 0 dma_err 0) | 58851 | 54711 | 111 | 18338 |
| Maze | grid | 38 | 43948 | native | 18/0/0 (late 1915 torn 2517 dma_rs 2464 lcd_rs 0 dma_err 0) | 18312 | 13619 | 86 | 14829 |
| Meadow | grid | 8 | 45416 | native | 19/0/0 (late 1939 torn 2564 dma_rs 2531 lcd_rs 0 dma_err 0) | 130586 | 126237 | 118 | 15225 |
| Meteor Shower | grid | 16 | 42980 | native | 19/0/0 (late 2024 torn 2627 dma_rs 2572 lcd_rs 0 dma_err 0) | 63750 | 60187 | 124 | 18736 |
| Millipede | grid | 28 | 45356 | native | 19/0/0 (late 2042 torn 2672 dma_rs 2614 lcd_rs 0 dma_err 0) | 34992 | 30392 | 106 | 17834 |
| Moiré Gallery | grid | 41 | 44476 | native | 19/0/0 (late 2108 torn 2710 dma_rs 2700 lcd_rs 0 dma_err 0) | 15812 | 11339 | 69 | 15289 |
| Moon Phase | grid | 48 | 44460 | native | 20/0/0 (late 2133 torn 2758 dma_rs 2757 lcd_rs 0 dma_err 0) | 6289 | 1982 | 30 | 13614 |
| Mystify | grid | 38 | 45624 | native | 22/0/0 (late 2181 torn 2811 dma_rs 2826 lcd_rs 0 dma_err 0) | 22532 | 17768 | 72 | 15820 |
| Neon Veins | grid | 10 | 45132 | native | 23/0/0 (late 2215 torn 2865 dma_rs 2864 lcd_rs 0 dma_err 0) | 102605 | 98936 | 108 | 15798 |
| Newton's Cradle | grid | 22 | 45860 | native | 23/0/0 (late 2277 torn 2904 dma_rs 2905 lcd_rs 0 dma_err 0) | 45751 | 41825 | 105 | 14948 |
| Night City | grid | 14 | 45432 | native | 23/0/0 (late 2296 torn 2940 dma_rs 2947 lcd_rs 0 dma_err 0) | 70779 | 66912 | 93 | 15427 |
| Notification Ripple | grid | 16 | 45332 | native | 23/0/0 (late 2322 torn 2994 dma_rs 2997 lcd_rs 0 dma_err 0) | 62103 | 58031 | 116 | 17722 |
| Ocean | grid | 13 | 43792 | native | 23/0/0 (late 2406 torn 3038 dma_rs 3032 lcd_rs 0 dma_err 0) | 78599 | 74519 | 116 | 15881 |
| Oil Slick | grid | 9 | 44520 | native | 23/0/0 (late 2463 torn 3067 dma_rs 3069 lcd_rs 0 dma_err 0) | 111380 | 107383 | 106 | 15600 |
| Opposites | grid | 21 | 45380 | native | 23/0/0 (late 2492 torn 3114 dma_rs 3094 lcd_rs 0 dma_err 0) | 46871 | 42808 | 73 | 15288 |
| Orbits | grid | 39 | 44432 | native | 23/0/0 (late 2542 torn 3212 dma_rs 3154 lcd_rs 0 dma_err 0) | 16448 | 12128 | 81 | 14940 |
| Pendulum Wave | grid | 26 | 46088 | native | 24/0/0 (late 2568 torn 3251 dma_rs 3207 lcd_rs 0 dma_err 0) | 37289 | 33061 | 108 | 17596 |
| Physarum | grid | 10 | 44172 | native | 24/0/0 (late 2589 torn 3294 dma_rs 3251 lcd_rs 0 dma_err 0) | 106743 | 102930 | 115 | 16861 |
| Pipes | grid | 47 | 44700 | native | 24/0/0 (late 2633 torn 3333 dma_rs 3303 lcd_rs 0 dma_err 0) | 9352 | 5249 | 36 | 14097 |
| Plasma | grid | 17 | 45196 | native | 25/0/0 (late 2681 torn 3378 dma_rs 3346 lcd_rs 0 dma_err 0) | 57583 | 53764 | 81 | 15481 |
| Plucked String | grid | 15 | 45736 | native | 25/0/0 (late 2720 torn 3409 dma_rs 3384 lcd_rs 0 dma_err 0) | 69673 | 66127 | 132 | 16914 |
| Police Lights | grid | 25 | 45472 | native | 25/0/0 (late 2750 torn 3443 dma_rs 3424 lcd_rs 0 dma_err 0) | 36864 | 32784 | 68 | 16190 |
| Pong Clock | grid | 15 | 45496 | native | 25/0/0 (late 2768 torn 3466 dma_rs 3478 lcd_rs 0 dma_err 0) | 65285 | 61564 | 119 | 18976 |
| Pool Caustics | grid | 10 | 45040 | native | 25/0/0 (late 2813 torn 3509 dma_rs 3509 lcd_rs 0 dma_err 0) | 107664 | 103684 | 116 | 15422 |
| Portal | grid | 9 | 43304 | native | 25/0/0 (late 2841 torn 3558 dma_rs 3537 lcd_rs 0 dma_err 0) | 111676 | 107419 | 100 | 16068 |
| Pulse Train | grid | 10 | 42956 | native | 25/0/0 (late 2863 torn 3587 dma_rs 3562 lcd_rs 0 dma_err 0) | 105743 | 101907 | 98 | 15898 |
| Quasicrystal | grid | 12 | 44400 | native | 25/0/0 (late 2893 torn 3612 dma_rs 3585 lcd_rs 0 dma_err 0) | 85814 | 82378 | 105 | 16459 |
| Radar | grid | 30 | 44980 | native | 25/0/0 (late 2940 torn 3660 dma_rs 3628 lcd_rs 0 dma_err 0) | 32231 | 27972 | 155 | 17492 |
| Radiant Bloom | grid | 17 | 44836 | native | 25/0/0 (late 2995 torn 3683 dma_rs 3666 lcd_rs 0 dma_err 0) | 58070 | 54733 | 81 | 15551 |
| Rain on Glass | grid | 16 | 45372 | native | 25/0/0 (late 3057 torn 3714 dma_rs 3693 lcd_rs 0 dma_err 0) | 62211 | 57805 | 116 | 16365 |
| Rainbow | grid | 27 | 42476 | native | 25/0/0 (late 3101 torn 3772 dma_rs 3740 lcd_rs 0 dma_err 0) | 36333 | 32430 | 93 | 15059 |
| Rainbow Melt | grid | 15 | 41260 | native | 25/0/0 (late 3147 torn 3833 dma_rs 3773 lcd_rs 0 dma_err 0) | 69157 | 65606 | 96 | 15479 |
| Raindrops | grid | 15 | 43376 | native | 25/0/0 (late 3202 torn 3939 dma_rs 3800 lcd_rs 0 dma_err 0) | 66177 | 62709 | 108 | 16981 |
| Reaction Diffusion | grid | 9 | 45336 | native | 25/0/0 (late 3234 torn 3968 dma_rs 3831 lcd_rs 0 dma_err 0) | 122349 | 118080 | 87 | 16720 |
| Rocket Launch | grid | 42 | 44120 | native | 25/0/0 (late 3266 torn 4022 dma_rs 3894 lcd_rs 0 dma_err 0) | 14622 | 10313 | 58 | 15187 |
| Rotozoomer | grid | 22 | 45340 | native | 26/0/0 (late 3321 torn 4064 dma_rs 3958 lcd_rs 0 dma_err 0) | 46113 | 42593 | 100 | 15338 |
| Saber | grid | 22 | 44988 | native | 26/0/0 (late 3336 torn 4107 dma_rs 3985 lcd_rs 0 dma_err 0) | 45576 | 41391 | 110 | 17156 |
| Scanner | grid | 42 | 44504 | native | 26/0/0 (late 3356 torn 4151 dma_rs 4043 lcd_rs 0 dma_err 0) | 14414 | 10261 | 57 | 15159 |
| Searchlights | grid | 15 | 44784 | native | 27/0/0 (late 3406 torn 4224 dma_rs 4096 lcd_rs 0 dma_err 0) | 69565 | 64937 | 107 | 17006 |
| Seasons Tree | grid | 24 | 45312 | native | 27/0/0 (late 3439 torn 4300 dma_rs 4139 lcd_rs 0 dma_err 0) | 41189 | 35886 | 111 | 15876 |
| Shadebobs | grid | 14 | 45320 | native | 27/0/0 (late 3473 torn 4329 dma_rs 4180 lcd_rs 0 dma_err 0) | 71883 | 67857 | 86 | 16293 |
| Snowfall | grid | 11 | 45188 | native | 27/0/0 (late 3522 torn 4376 dma_rs 4204 lcd_rs 0 dma_err 0) | 92290 | 88230 | 140 | 15927 |
| Soap | grid | 15 | 44980 | native | 27/0/0 (late 3548 torn 4421 dma_rs 4232 lcd_rs 0 dma_err 0) | 66942 | 63012 | 77 | 15670 |
| Sorting | grid | 41 | 45272 | native | 27/0/0 (late 3626 torn 4448 dma_rs 4288 lcd_rs 0 dma_err 0) | 20059 | 15896 | 40 | 14767 |
| Sound Rays | grid | 31 | 45124 | native | 28/0/0 (late 3649 torn 4491 dma_rs 4347 lcd_rs 0 dma_err 0) | 31028 | 26840 | 108 | 17129 |
| Sparkler | any | 36 | 45772 | native | 29/0/0 (late 3691 torn 4534 dma_rs 4390 lcd_rs 0 dma_err 0) | 27484 | 23035 | 79 | 16443 |
| Sparks | any | 44 | 45900 | native | 30/0/0 (late 3727 torn 4550 dma_rs 4452 lcd_rs 0 dma_err 0) | 11603 | 7358 | 37 | 14748 |
| Spectrum Analyzer | grid | 21 | 44484 | native | 31/0/0 (late 3764 torn 4618 dma_rs 4493 lcd_rs 0 dma_err 0) | 47404 | 43807 | 108 | 16132 |
| Spectrum Field | grid | 16 | 44064 | native | 31/0/0 (late 3810 torn 4676 dma_rs 4535 lcd_rs 0 dma_err 0) | 61652 | 57757 | 117 | 17182 |
| Spiral Galaxy | grid | 12 | 42192 | native | 31/0/0 (late 3857 torn 4713 dma_rs 4570 lcd_rs 0 dma_err 0) | 82363 | 77987 | 129 | 15183 |
| Spiral Twirls | grid | 11 | 43444 | native | 31/0/0 (late 3894 torn 4766 dma_rs 4613 lcd_rs 0 dma_err 0) | 96809 | 93319 | 112 | 16717 |
| Spirograph | grid | 44 | 43944 | native | 31/0/0 (late 3953 torn 4803 dma_rs 4664 lcd_rs 0 dma_err 0) | 10656 | 5961 | 58 | 14718 |
| Stacker | any | 46 | 45196 | native | 39/0/0 (late 3992 torn 4829 dma_rs 4721 lcd_rs 0 dma_err 0) | 8790 | 4715 | 31 | 14598 |
| Starburst | any | 44 | 44980 | native | 41/0/0 (late 4032 torn 4872 dma_rs 4784 lcd_rs 0 dma_err 0) | 9515 | 5484 | 42 | 14701 |
| Starfield | grid | 26 | 44576 | native | 42/0/0 (late 4065 torn 4921 dma_rs 4836 lcd_rs 0 dma_err 0) | 38349 | 33980 | 105 | 17511 |
| Strange Attractor | grid | 13 | 44540 | native | 42/0/0 (late 4134 torn 4951 dma_rs 4881 lcd_rs 0 dma_err 0) | 77572 | 73973 | 96 | 16417 |
| Sunrise Alarm | strip | 28 | 42668 | native | 42/0/0 (late 4165 torn 5017 dma_rs 4923 lcd_rs 0 dma_err 0) | 34515 | 30836 | 105 | 17816 |
| Tetrix | grid | 18 | 44840 | native | 42/0/0 (late 4252 torn 5052 dma_rs 4967 lcd_rs 0 dma_err 0) | 55226 | 50861 | 74 | 17013 |
| Thunderstorm | grid | 10 | 42620 | native | 42/0/0 (late 4301 torn 5167 dma_rs 5009 lcd_rs 0 dma_err 0) | 104984 | 100704 | 114 | 17716 |
| tixy | grid | 19 | 44808 | native | 42/0/0 (late 4321 torn 5203 dma_rs 5039 lcd_rs 0 dma_err 0) | 53111 | 48723 | 101 | 16834 |
| Topo Map | grid | 8 | 42840 | native | 42/0/0 (late 4373 torn 5245 dma_rs 5073 lcd_rs 0 dma_err 0) | 126570 | 123075 | 107 | 15348 |
| Touch Glow | grid | 17 | 45060 | native | 42/0/0 (late 4424 torn 5265 dma_rs 5098 lcd_rs 0 dma_err 0) | 58459 | 54038 | 107 | 16218 |
| Truchet Weave | grid | 29 | 44956 | native | 42/0/0 (late 4438 torn 5293 dma_rs 5131 lcd_rs 0 dma_err 0) | 34411 | 30352 | 101 | 16229 |
| Tunnel | grid | 17 | 43820 | native | 43/0/0 (late 4501 torn 5331 dma_rs 5167 lcd_rs 0 dma_err 0) | 61012 | 57351 | 81 | 16771 |
| Turmites | grid | 48 | 45496 | native | 43/0/0 (late 4527 torn 5375 dma_rs 5207 lcd_rs 0 dma_err 0) | 6981 | 2838 | 30 | 13543 |
| TV Simulator | grid | 23 | 46152 | native | 44/0/0 (late 4561 torn 5388 dma_rs 5234 lcd_rs 0 dma_err 0) | 41174 | 36267 | 76 | 14665 |
| Twinkle | any | 46 | 43788 | native | 44/0/0 (late 4591 torn 5440 dma_rs 5280 lcd_rs 0 dma_err 0) | 10229 | 5970 | 40 | 14713 |
| Twister | grid | 14 | 46020 | native | 45/0/0 (late 4613 torn 5473 dma_rs 5309 lcd_rs 0 dma_err 0) | 72404 | 68639 | 104 | 14780 |
| Voronoi | grid | 16 | 45416 | native | 45/0/0 (late 4649 torn 5508 dma_rs 5355 lcd_rs 0 dma_err 0) | 63323 | 59402 | 106 | 16160 |
| Wander Edges | grid | 17 | 45468 | native | 45/0/0 (late 4665 torn 5548 dma_rs 5383 lcd_rs 0 dma_err 0) | 58169 | 54516 | 108 | 15990 |
| Waving Flag | grid | 25 | 41984 | native | 45/0/0 (late 4716 torn 5655 dma_rs 5412 lcd_rs 0 dma_err 0) | 39811 | 36256 | 90 | 14971 |
| Wavy Bands | grid | 11 | 45680 | native | 46/0/0 (late 4750 torn 5680 dma_rs 5438 lcd_rs 0 dma_err 0) | 92498 | 88557 | 87 | 15690 |
| Wireframe | grid | 11 | 44856 | native | 46/0/0 (late 4807 torn 5747 dma_rs 5470 lcd_rs 0 dma_err 0) | 93272 | 88963 | 114 | 18369 |
| Word Clock | grid | 42 | 44848 | native | 46/0/0 (late 4846 torn 5798 dma_rs 5544 lcd_rs 0 dma_err 0) | 13163 | 8496 | 61 | 15484 |
| Xorcery | grid | 22 | 44760 | native | 48/0/0 (late 4912 torn 5820 dma_rs 5591 lcd_rs 0 dma_err 0) | 45573 | 42024 | 91 | 14853 |

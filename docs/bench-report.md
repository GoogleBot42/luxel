# Hardware soak + benchmark — 2026-11-02

*Device 192.168.0.183, firmware v0.1.40, 144 px ws2812, brightness 6.*
*Regenerate: `node tools/hw-bench.mjs <ip>` (≈45 min; runs every gallery pattern on the strip).*

## Summary

- 150 patterns: **150 clean**, 0 with errors, 3 under 30 fps.
- fps at 144 px (the count the sweep ran at): median **123**, p10 67, p90 123.
- lowest heap_free seen while soaking: 31824 bytes.

## fps vs pixel count (rainbow reference)

| pixels | fps |
|---:|---:|
| 60 | 122 |
| 150 | 123 |
| 300 | 85 |
| 600 | 45 |
| 1024 | 27 |
| 2048 | 14 |

## Slowest (< 30 fps at 144 px)

| pattern | kind | fps |
|---|---|---:|
| Coronal Mass Ejection | grid | 14 |
| Kaleidoscope | grid | 21 |
| Fire Eye | grid | 29 |

## All results

| pattern | kind | fps | heap largest | jit | drops | frame µs | vm µs | pipe µs | out µs |
|---|---|---:|---:|---|---|---:|---:|---:|---:|
| _Fairies | any | 123 | 70512 | native | — | 5858 | 232 | 103 | 5485 |
| 80s Kid Show | grid | 64 | 67440 | interp | — | 15372 | 9603 | 120 | 5587 |
| Analog Clock | grid | 123 | 68416 | interp | — | 6933 | 1137 | 120 | 5612 |
| Asteroids | grid | 67 | 67472 | interp | — | 14639 | 8815 | 120 | 5620 |
| Aurora 2D | grid | 123 | 68884 | interp | — | 7301 | 1563 | 108 | 5588 |
| Bessel Chaos | grid | 123 | 67944 | native | — | 6870 | 1154 | 106 | 5555 |
| Bit Rain | grid | 88 | 32892 | interp | — | 11163 | 5453 | 126 | 5522 |
| Blinky Eyes | grid | 123 | 70484 | native | — | 6467 | 772 | 108 | 5531 |
| Block Clock | grid | 87 | 69392 | interp | — | 11327 | 5530 | 110 | 5621 |
| Boids | grid | 79 | 69548 | native | — | 12400 | 6669 | 109 | 5563 |
| Bokeh | grid | 60 | 70812 | native | — | 16410 | 10690 | 110 | 5556 |
| Bouncing Balls | grid | 123 | 68768 | native | — | 6475 | 751 | 103 | 5565 |
| Bouncing Columns | grid | 123 | 70472 | native | — | 7073 | 1326 | 118 | 5566 |
| Bouncy Boxes | grid | 123 | 71248 | native | — | 6868 | 1141 | 110 | 5553 |
| Breakout | grid | 120 | 63708 | interp | — | 7924 | 2098 | 127 | 5635 |
| Bulk Sprite Scroll 2D | grid | 123 | 72036 | native | — | 6553 | 788 | 111 | 5593 |
| Butterfly | grid | 80 | 44252 | interp | — | 12245 | 6505 | 105 | 5574 |
| Candle | grid | 86 | 70516 | interp | — | 11425 | 5787 | 92 | 5504 |
| Candy Stripes | grid | 123 | 67120 | native | — | 6365 | 687 | 98 | 5524 |
| Cellular Automata | grid | 122 | 40716 | native | — | 7184 | 1418 | 109 | 5601 |
| Chladni Plate | grid | 123 | 44304 | native | — | 7229 | 1467 | 112 | 5587 |
| Chomp Chase | grid | 123 | 62104 | interp | — | 7224 | 1469 | 114 | 5591 |
| Circle Packing | grid | 124 | 65604 | native | — | 6109 | 452 | 100 | 5512 |
| Classic Christmas Strands | strip | 123 | 70860 | native | — | 6582 | 901 | 98 | 5533 |
| Color Sequence | strip | 123 | 67848 | native | — | 6874 | 1142 | 102 | 5571 |
| color twinkle bounce | strip | 123 | 71504 | native | — | 6473 | 801 | 108 | 5514 |
| Color Wash | grid | 123 | 63276 | native | — | 7040 | 1305 | 104 | 5589 |
| Copper Bars | grid | 123 | 67540 | native | — | 6746 | 1007 | 100 | 5594 |
| Coronal Mass Ejection | grid | 14 | 27728 | interp | — | 71516 | 65607 | 117 | 5725 |
| Crosstown Traffic | grid | 123 | 68984 | native | — | 6208 | 490 | 104 | 5563 |
| Crystal Growth | grid | 121 | 35232 | interp | — | 6295 | 635 | 105 | 5514 |
| Curl Flow | grid | 50 | 65900 | native | — | 19918 | 14015 | 116 | 5732 |
| Day Ribbon | any | 121 | 68344 | interp | — | 8009 | 2247 | 120 | 5577 |
| Digital Rain | grid | 123 | 68992 | native | — | 6217 | 507 | 104 | 5552 |
| Dire Spider | grid | 80 | 57388 | interp | — | 12256 | 6486 | 116 | 5603 |
| DNA Helix | grid | 123 | 70740 | native | — | 7256 | 1472 | 108 | 5598 |
| Doom Fire | grid | 123 | 60432 | native | — | 7224 | 1451 | 99 | 5619 |
| Dot Globe | grid | 51 | 65128 | native | — | 19358 | 13626 | 109 | 5566 |
| Double Pendulum | grid | 69 | 68824 | native | — | 14387 | 8661 | 107 | 5560 |
| Edge Burst | grid | 123 | 66068 | native | — | 6895 | 1179 | 102 | 5578 |
| Embers | grid | 123 | 64168 | native | — | 7445 | 1721 | 107 | 5559 |
| Energy Duel | grid | 112 | 68836 | interp | — | 8665 | 2895 | 117 | 5601 |
| Falling Sand | grid | 121 | 46976 | interp | — | 7111 | 1401 | 112 | 5554 |
| Fast Pulse | grid | 123 | 71128 | native | — | 6404 | 715 | 113 | 5520 |
| Fibonacci Clock | grid | 123 | 71188 | native | — | 6834 | 1081 | 110 | 5566 |
| Fire | grid | 90 | 59212 | interp | — | 10904 | 5258 | 89 | 5498 |
| Fire Eye | grid | 29 | 31464 | native | — | 34986 | 29222 | 103 | 5605 |
| Fireflies | grid | 123 | 68044 | native | — | 6444 | 704 | 99 | 5588 |
| Fireworks | grid | 97 | 62360 | interp | — | 10024 | 4358 | 90 | 5530 |
| Fractal Flower | grid | 94 | 37828 | native | — | 10397 | 4631 | 111 | 5589 |
| Frogger | grid | 67 | 63068 | interp | — | 14688 | 8811 | 109 | 5717 |
| Gears | grid | 114 | 36868 | native | — | 8051 | 2235 | 121 | 5626 |
| Geometry Morph | grid | 85 | 65392 | native | — | 11487 | 5811 | 101 | 5522 |
| Glittering Jewels | grid | 123 | 70048 | native | — | 6517 | 789 | 103 | 5575 |
| GlowFlow | grid | 120 | 65472 | interp | — | 8088 | 2389 | 95 | 5560 |
| God Rays | grid | 87 | 34016 | native | — | 11238 | 5460 | 110 | 5610 |
| Gravity Wells | grid | 47 | 64656 | native | — | 21267 | 15494 | 111 | 5603 |
| Halftone | grid | 123 | 62312 | native | — | 7084 | 1302 | 122 | 5588 |
| Heart | grid | 123 | 71540 | native | — | 6331 | 676 | 107 | 5497 |
| Holiday Star | grid | 122 | 70984 | native | — | 7031 | 1256 | 102 | 5606 |
| HUD Ticker | grid | 123 | 71656 | native | — | 6989 | 1283 | 102 | 5568 |
| Infinite Snake | grid | 120 | 39232 | interp | — | 7563 | 1795 | 112 | 5608 |
| Interference | grid | 123 | 69296 | native | — | 7414 | 1707 | 102 | 5546 |
| Jellyfish | grid | 77 | 70636 | native | — | 12790 | 7040 | 116 | 5571 |
| Julia Morph | grid | 122 | 38988 | native | — | 7658 | 1901 | 112 | 5587 |
| Kaleidoscope | grid | 21 | 27728 | interp | — | 47949 | 42219 | 118 | 5563 |
| Lamp | grid | 123 | 59052 | native | — | 5999 | 311 | 99 | 5551 |
| Lava Lamp | grid | 58 | 48232 | interp | — | 17019 | 11343 | 106 | 5520 |
| Life | grid | 121 | 35424 | interp | — | 6710 | 956 | 115 | 5591 |
| Light Organ | grid | 123 | 70368 | native | — | 6495 | 782 | 103 | 5563 |
| Light Thief | grid | 121 | 67696 | interp | — | 8008 | 2232 | 106 | 5623 |
| Line Dancer | grid | 123 | 68076 | native | — | 7385 | 1670 | 106 | 5547 |
| Loop Ring | grid | 43 | 29828 | native | — | 23115 | 17406 | 103 | 5551 |
| Lotus Bloom | grid | 91 | 32016 | interp | — | 10812 | 5094 | 114 | 5554 |
| Marbling | grid | 123 | 51676 | native | — | 6723 | 986 | 102 | 5582 |
| Marquee | grid | 123 | 68680 | native | — | 6677 | 962 | 104 | 5569 |
| Maze | grid | 122 | 41880 | interp | — | 7221 | 1436 | 114 | 5588 |
| Meadow | grid | 50 | 68860 | native | — | 20047 | 14272 | 110 | 5579 |
| Meteor Shower | grid | 123 | 67748 | interp | — | 6992 | 1209 | 105 | 5633 |
| Millipede | grid | 123 | 70408 | native | — | 6518 | 813 | 100 | 5564 |
| Moiré Gallery | grid | 123 | 30724 | interp | — | 7480 | 1773 | 121 | 5536 |
| Moon Phase | grid | 123 | 49560 | native | — | 6282 | 560 | 107 | 5537 |
| Mystify | grid | 122 | 70740 | native | — | 7635 | 1914 | 115 | 5552 |
| Neon Veins | grid | 110 | 34420 | native | — | 8852 | 3115 | 106 | 5576 |
| Newton's Cradle | grid | 123 | 71680 | native | — | 6348 | 615 | 104 | 5587 |
| Night City | grid | 123 | 70388 | native | — | 5875 | 314 | 92 | 5440 |
| Notification Ripple | grid | 123 | 70988 | native | — | 6442 | 713 | 104 | 5572 |
| Ocean | grid | 81 | 69204 | interp | — | 12106 | 6449 | 94 | 5513 |
| Oil Slick | grid | 122 | 36664 | native | — | 7781 | 2088 | 102 | 5541 |
| Opposites | grid | 123 | 71104 | native | — | 6616 | 925 | 103 | 5541 |
| Orbits | grid | 123 | 70192 | interp | — | 7058 | 1352 | 108 | 5547 |
| Pendulum Wave | grid | 123 | 71716 | native | — | 6513 | 796 | 108 | 5561 |
| Physarum | grid | 106 | 30296 | interp | — | 8129 | 2414 | 113 | 5556 |
| Pipes | grid | 124 | 66176 | native | — | 5939 | 292 | 104 | 5509 |
| Plasma | grid | 123 | 51496 | native | — | 6886 | 1176 | 99 | 5565 |
| Plucked String | grid | 122 | 69236 | native | — | 7758 | 2001 | 109 | 5589 |
| Police Lights | grid | 123 | 71292 | native | — | 6131 | 432 | 97 | 5565 |
| Pong Clock | grid | 122 | 71380 | native | — | 7363 | 1516 | 117 | 5639 |
| Pool Caustics | grid | 123 | 45836 | native | — | 7514 | 1790 | 106 | 5570 |
| Portal | grid | 123 | 46884 | native | — | 7363 | 1634 | 105 | 5566 |
| Pulse Train | grid | 123 | 62300 | native | — | 6552 | 844 | 99 | 5572 |
| Quasicrystal | grid | 122 | 69600 | native | — | 7614 | 1923 | 102 | 5522 |
| Radar | grid | 118 | 69888 | native | — | 8279 | 2567 | 105 | 5542 |
| Radiant Bloom | grid | 123 | 65496 | native | — | 7558 | 1842 | 104 | 5548 |
| Rain on Glass | grid | 95 | 41288 | interp | — | 10347 | 4571 | 125 | 5605 |
| Rainbow | grid | 123 | 49332 | interp | — | 6455 | 760 | 106 | 5547 |
| Rainbow Melt | grid | 123 | 47380 | native | — | 6940 | 1225 | 98 | 5580 |
| Raindrops | grid | 122 | 48336 | interp | — | 7111 | 1368 | 109 | 5572 |
| Reaction Diffusion | grid | 73 | 34056 | native | — | 13521 | 7775 | 106 | 5584 |
| Rocket Launch | grid | 123 | 67612 | native | — | 6399 | 673 | 99 | 5577 |
| Rotozoomer | grid | 123 | 68760 | native | — | 6670 | 939 | 103 | 5563 |
| Saber | grid | 123 | 70472 | native | — | 6491 | 727 | 102 | 5624 |
| Scanner | grid | 123 | 70004 | native | — | 6262 | 537 | 100 | 5580 |
| Searchlights | grid | 122 | 70164 | native | — | 7802 | 2061 | 113 | 5560 |
| Seasons Tree | grid | 45 | 65324 | interp | — | 21895 | 16081 | 122 | 5625 |
| Shadebobs | grid | 123 | 54564 | native | — | 6842 | 1087 | 109 | 5585 |
| Snowfall | grid | 123 | 66436 | interp | — | 6877 | 1129 | 118 | 5584 |
| Soap | grid | 103 | 59620 | native | — | 9505 | 3795 | 97 | 5555 |
| Sorting | grid | 123 | 66132 | native | — | 6295 | 569 | 99 | 5583 |
| Sound Rays | grid | 123 | 68904 | native | — | 7188 | 1462 | 102 | 5578 |
| Sparkler | any | 123 | 70632 | native | — | 6580 | 875 | 102 | 5557 |
| Sparks | any | 123 | 70696 | native | — | 6272 | 585 | 105 | 5536 |
| Spectrum Analyzer | grid | 123 | 69404 | native | — | 6835 | 1104 | 102 | 5578 |
| Spectrum Field | grid | 123 | 47936 | native | — | 6973 | 1251 | 104 | 5567 |
| Spiral Galaxy | grid | 36 | 33020 | native | — | 28054 | 22266 | 108 | 5622 |
| Spiral Twirls | grid | 123 | 34968 | native | — | 7098 | 1362 | 114 | 5564 |
| Spirograph | grid | 123 | 69124 | native | — | 6494 | 763 | 110 | 5562 |
| Stacker | any | 123 | 70684 | native | — | 6184 | 487 | 110 | 5546 |
| Starburst | any | 123 | 69912 | native | — | 6219 | 520 | 101 | 5552 |
| Starfield | grid | 123 | 66556 | native | — | 6998 | 1285 | 106 | 5556 |
| Strange Attractor | grid | 53 | 53784 | native | — | 18630 | 12832 | 108 | 5628 |
| Sunrise Alarm | strip | 122 | 66624 | native | — | 7741 | 1965 | 107 | 5609 |
| Tetrix | grid | 118 | 50972 | interp | — | 8277 | 2585 | 110 | 5535 |
| Thunderstorm | grid | 105 | 52636 | interp | — | 9329 | 3636 | 90 | 5542 |
| tixy | grid | 123 | 58020 | native | — | 6541 | 819 | 104 | 5568 |
| Topo Map | grid | 93 | 33316 | interp | — | 10518 | 4796 | 118 | 5554 |
| Touch Glow | grid | 120 | 54384 | native | — | 7990 | 2241 | 99 | 5597 |
| Truchet Weave | grid | 101 | 50656 | native | — | 9749 | 4018 | 101 | 5572 |
| Tunnel | grid | 123 | 64296 | native | — | 6800 | 1086 | 101 | 5544 |
| Turmites | grid | 124 | 54632 | native | — | 5966 | 286 | 99 | 5535 |
| TV Simulator | grid | 123 | 72132 | native | — | 6258 | 560 | 102 | 5559 |
| Twinkle | any | 123 | 63504 | native | — | 6047 | 382 | 103 | 5516 |
| Twister | grid | 122 | 71664 | native | — | 7670 | 1919 | 115 | 5578 |
| Voronoi | grid | 112 | 42532 | native | — | 8738 | 3014 | 104 | 5557 |
| Wander Edges | grid | 123 | 66424 | native | — | 6809 | 1056 | 103 | 5603 |
| Waving Flag | grid | 82 | 64528 | interp | — | 11912 | 6147 | 109 | 5603 |
| Wavy Bands | grid | 118 | 69124 | native | — | 8192 | 2532 | 93 | 5513 |
| Wireframe | grid | 65 | 66116 | interp | — | 15299 | 9565 | 112 | 5581 |
| Word Clock | grid | 123 | 70268 | native | — | 6614 | 852 | 111 | 5583 |
| Xorcery | grid | 123 | 69708 | native | — | 6624 | 938 | 98 | 5529 |

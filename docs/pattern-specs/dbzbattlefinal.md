# DBZBattleFinal
kind: 2D
sensors: no

## What it looks like

An endless anime-style energy battle on a 2D panel. Two glowing "fighter" orbs — one warm-colored (golden/Saiyan-like) on the left half, one cool-colored (violet/rival-like) on the right half — dart back and forth along the horizontal axis at mid-height, bobbing up and down with a rapid nervous jitter. They repeatedly rush each other; when they collide near the middle, a white shockwave ring bursts outward from the impact point and both fighters are knocked apart. Occasionally a fighter retreats to its own edge, charges, and fires a small cyan energy projectile across the screen; if it hits the opponent, another shockwave ring erupts and the victim is knocked back. After a few clashes (a randomly chosen small count, one to a few), the battle enters a "finale": both fighters fire simultaneously, big impacts throw both fighters hard toward their walls, then they recover, drift back toward center, and a fresh round begins. The cycle never ends. Individual behavioral decisions change every fraction of a second to about a second, so the motion feels twitchy and combative; a full clash-to-finale round takes several seconds.

## State kept between frames

- Position and velocity (both axes) for each of the two fighters.
- For each fighter: an "intent" state (idle / charge toward opponent / back off / feint — dash in then out / retreat-and-fire), a countdown timer until the next intent is re-rolled, plus a per-fighter vertical-bob frequency and amplitude (re-randomized on each intent change and on each clash), and a "ready to fire" latch used by the retreat-and-fire intent.
- One projectile slot per fighter: active flag plus x/y position.
- A small fixed-size pool of shockwave rings (about eight slots), each storing origin x/y and age. Ring slots are recycled: spawning a new ring overwrites the oldest one. Inactive slots are marked with a huge sentinel age.
- A clash counter and a randomly re-rolled "clashes before finale" threshold; a finale phase state (fighting / finale-projectiles-in-flight / finale-recovery) with a recovery timer; a running total-time clock used to drive the vertical bobbing sine.

## Per-frame simulation (before rendering)

All motion is integrated with the real frame delta converted to seconds.

1. Age every active ring.
2. **Finale phase A (both projectiles flying):** projectiles travel horizontally toward the opposing fighter at several times the base speed. On proximity to the target (a small horizontal window scaled by the size control, and a small vertical window), the projectile dies, a ring spawns at the victim, and the victim gets a large randomized knockback velocity away from center plus random vertical velocity. Fighters integrate position with strong per-frame velocity damping (velocity multiplied by a bit under one each frame), and are clamped so the left fighter stays in the left ~half and the right fighter in the right ~half, with vertical clamps away from the edges. When both projectiles are gone, enter phase B with a short random timer.
3. **Finale phase B (recovery):** vertical positions ease back toward mid-height; if the fighters have drifted very far apart they accelerate gently back toward each other. When the timer expires, the round resets: clash counter cleared, a new random clash threshold rolled, intents cleared and re-rolled — positions are deliberately NOT reset, and if the gap is very wide both get a nudge toward center.
4. **Normal battle:** active projectiles fly (slightly slower than finale ones) with the same hit test; a hit spawns a ring, knocks the victim back with moderate random velocity, and forces the victim to re-roll intent shortly.
5. Each fighter's intent timer counts down; on expiry a new intent is rolled. The roll is gap-aware: if the fighters are already close, only back-off or feint are chosen (coin flip); otherwise a small chance (roughly one in eight) picks retreat-and-fire, a bit under half picks charge, and the rest splits between feint and back-off. Each roll also randomizes the intent duration (a fraction of a second to about a second) and the bob frequency/amplitude. A fighter already in retreat-and-fire keeps it until it fires.
6. Movement: each intent maps to a target horizontal velocity (charge = toward opponent at slightly-randomized full speed; back off = away at reduced speed; feint = toward opponent early in the timer then away; retreat-and-fire = fast toward own wall until near it, then set the ready latch and, once ready and no projectile active, spawn a projectile at the fighter's position and switch to charge). Actual velocity eases toward the target velocity (first-order smoothing over roughly a quarter second), then position integrates.
7. Vertical position is not integrated in normal play: it's set directly to mid-height plus a sine of (total time × that fighter's bob frequency) times its bob amplitude (scaled by the size control); the two fighters' sines are out of phase. Positions clamp to the panel with a small margin.
8. **Clash detection:** if the horizontal gap shrinks below a small threshold (scaled by the size control), spawn a ring at the midpoint, throw both fighters apart with a large randomized velocity, jitter both vertical positions, re-randomize both bob parameters toward faster/bigger, and increment the clash counter. Reaching the threshold triggers the finale (both projectiles spawn immediately at the fighters); otherwise both fighters get short timers and mostly-charge intents so the fight resumes quickly.

## Per-pixel rendering (2D, unit-square coordinates)

Priority-ordered, first match wins per pixel:

1. **Rings:** for each live ring (younger than its lifespan of roughly a second), compute distance from the pixel to the ring origin; the ring front expands at a moderate constant speed. If the pixel is within a thin band of the front, brightness = (remaining life fraction) × (closeness to the band center); take the max over all rings. If above a tiny floor, draw pure white at that brightness and stop.
2. **Fighters:** each fighter is drawn as a small saturated-at-the-edges disc: within a tiny core radius, its hue at reduced saturation and full brightness (a hot center — the warm fighter's core is less saturated than the cool one's, making it look whiter); between the core and an aura radius a few times larger, fully saturated hue with brightness falling linearly to zero at the aura edge (aura peaks at partial brightness). Both radii scale with the size control.
3. **Projectiles:** small solid discs in a fixed bright cyan, radius a bit smaller than a fighter core, also scaled by the size control.
4. Otherwise black.

## Colors

- Fighter one: warm hue adjustable from red through orange to yellow; whitish-hot core, saturated aura.
- Fighter two: cool hue adjustable through violet/purple/magenta; saturated core and aura.
- Shockwave rings: pure white.
- Projectiles: bright cyan.
- Background: black.

## Controls (all sliders)

- **Speed:** scales all movement/projectile speeds over roughly a 5:1 range; never zero.
- **Scale:** scales fighter/projectile sizes, bob amplitude, and the clash/hit distance thresholds over roughly a 4:1 range.
- **Saiyan color:** picks the warm fighter's hue within the red-to-yellow band.
- **Rival color:** picks the cool fighter's hue within the violet-to-magenta band.

## Layout assumptions

Pure normalized 2D (unit square); no pixel-count hardcoding. Only a 2D renderer is provided — a 1D or 3D fallback would need to be added for other layouts. The left/right clamping assumes a roughly landscape-usable square; nothing else is layout-specific.

## Non-obvious details

- The ring pool recycles the oldest slot, so heavy clashing gracefully drops the oldest shockwave rather than allocating.
- Fighter positions persist across rounds — only counters/intents reset — so each round starts from wherever the last finale left them, avoiding a visible "snap".
- The vertical bob being a direct function of total time (not integrated) keeps it perfectly smooth even while horizontal motion is chaotic.
- Ring brightness gates behind a small threshold before it occludes fighters, so faint ring tails don't black out the fighter discs.

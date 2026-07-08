# _Fairies
kind: 1D
sensors: no

## What it looks like
A dense field of stationary twinkles. Many colored points (a user-picked hue, magenta-pink by default) fade out slowly over a few seconds, each vanishing and instantly reappearing at a new random position at full brightness. Sprinkled among them, a much smaller number of pure white sparkles do the same but live only a fraction as long, giving quick glints on top of the slower colored shimmer. Nothing travels; the motion is entirely birth-fade-rebirth, like fireflies or fairy dust settling over the strip.

## Algorithm
Two spark populations sharing one machinery, distinguished by a per-spark type flag fixed at startup:
- "Fairies": population size about half the pixel count. Colored (the picked hue, fully saturated). Lifetime randomized around the base lifetime (roughly plus or minus a fifth).
- "Sparks": population size about a sixteenth of the pixel count. White (zero saturation), same hue slot but desaturated. Lifetime randomized to only about a fifth to two-fifths of the base — several times shorter.

Per-spark state: position (a pixel index, chosen uniformly at random), a life value that starts at one, its assigned lifetime in milliseconds, and the type flag. Two per-pixel buffers hold brightness and saturation.

Each frame, for every spark:
1. Decrease its life by frame-delta divided by its lifetime (so life reaches zero after exactly its lifetime, frame-rate independently).
2. If life has run out, respawn: pick a fresh random position, reset life to one, and roll a new randomized lifetime from the range for its type.
3. Write the spark's current life into the brightness buffer at its position, and write full saturation (fairy) or zero saturation (white spark) into the saturation buffer there.

The renderer reads the two buffers and outputs the picked hue, the buffered saturation, and the buffered brightness squared — squaring gives a gentle ease-out so twinkles linger dim before dying.

Non-obvious details worth preserving:
- The per-pixel buffers are never cleared. A respawned spark abandons its old pixel with a near-zero residual value that just stays there (invisibly dim) until some spark lands on that pixel again. This is harmless and avoids a clear pass.
- When two sparks occupy the same pixel, the later one in the update loop wins outright (no blending); white sparks are stored after the colored ones so they tend to override.
- Because population scales with pixel count, density looks the same on any strip length. No hardcoding.
- Despite lineage from a moving-sparks pattern, positions here never advance during a spark's life — it is purely a twinkle.

## Controls
- Color picker (hue/sat/value style), "primary color": sets the fairy hue (only the hue component is used; saturation is forced full, white sparks ignore it).
- Slider, "speed": maps across roughly a one-to-five range of base lifetime, from well under two seconds up to many seconds; lower values mean faster, busier twinkling.

## Timing
Default fairy lifetime is a few seconds; white sparks under a second. Respawn is immediate, so total lit-point count is constant.

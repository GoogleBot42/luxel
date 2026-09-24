# Built-in fonts

Three PSF2 blobs, `include_bytes!`d by `crates/luxel-core/src/text.rs`. They are
the *only* fonts Luxel has: identical on the device, in `luxel serve` and in the
playground, referenced everywhere by name (`font("tiny")`, `F regular …`).

| name | source font | cell | advance | glyphs | bytes |
|---|---|---|---|---|---|
| `tiny.psf` | Tom Thumb | 3×6 | 4 | 95 | 602 |
| `regular.psf` | X11 misc-fixed 5×7 | 5×7 | 6 | 95 | 697 |
| `large.psf` | Spleen 5×8 | 5×8 | 6 | 95 | 792 |

2,091 B total. Each blob is a 32-byte PSF2 header followed by
`95 × charsize` bytes of MSB-first row-packed glyphs for code points
`0x20..=0x7E` in order — no Unicode table, so glyph index is
`codepoint - 0x20` and lookup is one multiply. The advance is
`cell width + 1`, which is why Tom Thumb lands on its designed 4 px pitch.

## Conversion

`tools/fonts/bdf2psf2.py` (checked in; see docs/tools.md) does BDF → PSF2.
To reproduce these blobs exactly, from the repo root inside `nix develop`:

```sh
cd "$(mktemp -d)"
curl -O https://raw.githubusercontent.com/hzeller/rpi-rgb-led-matrix/master/fonts/tom-thumb.bdf
curl -O https://raw.githubusercontent.com/hzeller/rpi-rgb-led-matrix/master/fonts/5x7.bdf
curl -O https://raw.githubusercontent.com/fcambus/spleen/master/spleen-5x8.bdf
cd -
python3 tools/fonts/bdf2psf2.py "$OLDPWD/tom-thumb.bdf"    crates/luxel-core/fonts/tiny.psf
python3 tools/fonts/bdf2psf2.py "$OLDPWD/5x7.bdf"          crates/luxel-core/fonts/regular.psf
python3 tools/fonts/bdf2psf2.py "$OLDPWD/spleen-5x8.bdf"   crates/luxel-core/fonts/large.psf
```

The cell comes from each BDF's own `FONTBOUNDINGBOX`; `--width`/`--height`
override it. `crates/luxel-core/src/text.rs`'s `fonts_are_well_formed_psf2`
and `total_font_bytes` tests re-check the header, the glyph count and the
size on every `cargo test`.

## Provenance and licences

All three are permissive and compatible with this crate's Apache-2.0
licence; licences were read at the primary source, not at an aggregator
(`docs/design/webui-v2/research/comparative-research.md` §F records two
widely repeated licence claims that are wrong).

### `tiny.psf` — Tom Thumb (3×5 ink in a 4 px pitch)

Brian J. Swetland and Vassilii Khachaturov's 3×5 font with Robey Pointer's
readability modifications (<https://robey.lag.net/2010/01/23/tiny-monospace-font.html>).
BDF taken from hzeller/rpi-rgb-led-matrix `fonts/tom-thumb.bdf`. Licence as
quoted in Adafruit-GFX-Library `Fonts/TomThumb.h`, the canonical carrier of
the grant — **BSD-3-Clause**:

> The original 3x5 font is licensed under the 3-clause BSD license:
>
> Copyright 1999 Brian J. Swetland
> Copyright 1999 Vassilii Khachaturov
> Portions (of vt100.c/vt100.h) copyright Dan Marks
>
> All rights reserved.
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions
> are met:
> 1. Redistributions of source code must retain the above copyright
>    notice, this list of conditions, and the following disclaimer.
> 2. Redistributions in binary form must reproduce the above copyright
>    notice, this list of conditions, and the following disclaimer in the
>    documentation and/or other materials provided with the distribution.
> 3. The name of the authors may not be used to endorse or promote products
>    derived from this software without specific prior written permission.
>
> THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
> IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
> OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
> IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
> INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
> NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
> DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
> THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
> (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
> THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
>
> Modifications to Tom Thumb for improved readability are from Robey Pointer […]
> The original author does not have any objection to relicensing of Robey
> Pointer's modifications (in this file) in a more permissive license.

(The BDF we convert carries `COPYRIGHT "MIT"` in its header — a downstream
relabel. BSD-3-Clause above is the grant actually made by the authors, so
that is the one recorded here; both are permissive and Apache-2.0
compatible.)

### `regular.psf` — X11 misc-fixed 5×7

Markus Kuhn's Unicode extensions of the classic X11 `-Misc-Fixed-*` terminal
fonts (ucs-fonts, <https://www.cl.cam.ac.uk/~mgk25/ucs-fonts.html>), BDF taken
from hzeller/rpi-rgb-led-matrix `fonts/5x7.bdf`. **Public domain**, stated in
the BDF header itself:

> `COPYRIGHT "Public domain font.  Share and enjoy."`

and restated in the package README:

> By sending me extensions to these fonts, you agree that the resulting
> improved font files will remain in the public domain for everyone's free use.

### `large.psf` — Spleen 5×8

Frederic Cambus, <https://www.cambus.net/spleen-monospaced-bitmap-fonts/>,
source <https://github.com/fcambus/spleen>. **BSD-2-Clause**
(`SPDX-License-Identifier: BSD-2-Clause` in the BDF header):

> Copyright (c) 2018-2026, Frederic Cambus
> All rights reserved.
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are met:
>
>   * Redistributions of source code must retain the above copyright
>     notice, this list of conditions and the following disclaimer.
>
>   * Redistributions in binary form must reproduce the above copyright
>     notice, this list of conditions and the following disclaimer in the
>     documentation and/or other materials provided with the distribution.
>
> THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
> AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
> IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
> ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS
> BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
> CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
> SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
> INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
> CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
> ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
> POSSIBILITY OF SUCH DAMAGE.

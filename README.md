# Pixler

A fully open-source, live-codable LED controller — write LED patterns in a JS-like
scripting language in a web IDE and watch them run instantly on real hardware.

Pixler reimplements the ideas of the (closed-source) Pixel Blaze firmware, clean-room,
from the vendor's publicly documented pattern language and protocols:

- **Pattern-language source compatibility** with Pixel Blaze — the 200+ community
  patterns (`.epe`) should just work.
- **One portable core** (`libpixler`, C11, 16.16 fixed point): the same compiler + VM runs
  on the ESP32 firmware, in the browser via WASM (instant hardware-free preview), and
  natively in CI (conformance tests, fuzzing).
- **Open peripheral compatibility**: Pixel Blaze's MIT-licensed sensor board and output
  expander protocols are adopted verbatim.
- **No cloud, no app** — everything works on a device in AP mode.

Status: **planning**. See [docs/PLAN.md](docs/PLAN.md) for the full plan (architecture,
requirements, milestones) and [docs/research/](docs/research/) for the research it's
built on.

Licensing (planned): core library, IDE, and CLI under Apache-2.0; device firmware under
GPL-3.0-or-later.

*Pixel Blaze is a trademark of its owner; Pixler is an independent project compatible
with the Pixel Blaze pattern language.*

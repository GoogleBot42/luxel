# Luxel pattern library

Clean-room reimplementations of the Pixel Blaze community corpus. The
scraped corpus has unknown licensing, so none of its code appears here.
Instead, every pattern in this directory was produced by a two-party
firewall:

1. A **describer** read the original and wrote a prose-only functional
   specification — behavior, algorithm, colors, controls, timing — with
   no code, no identifier names, and no copied numeric constants.
2. An **implementer** who never saw the original wrote fresh Luxel code
   from that specification alone.

File conventions:

- First line: `// name: <Display Name>` — the pattern browser reads this.
- A provenance comment noting the clean-room origin.
- Patterns using a 2D buffer simulate on a 16×16 virtual canvas sampled
  by normalized coordinates in `render2D`, so any map works.
- Sound/motion patterns bind sensors the PB way (`export var
  frequencyData`, etc.); the engine stubs them with zeros until real
  peripherals land, so they run dark rather than erroring.

The curated teaching set lives in `examples/`; this directory is the
bulk library that feeds the playground's pattern browser via
`web/tools/gen-gallery.mjs`.

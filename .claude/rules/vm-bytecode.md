---
paths:
  - "crates/luxel-core/src/vm.rs"
  - "crates/luxel-core/src/bytecode.rs"
---

- The `BUILTINS` table in `vm.rs` is APPEND-ONLY: the array index IS the
  runtime builtin id ("Order is the builtin id — append only"). Bytecode
  blobs store builtin *names* in a per-blob import table and resolve them to
  runtime ids at load (see `bytecode.rs`), which is exactly why appending is
  safe — and why removing or renaming an entry breaks every stored pattern
  that imports that name. Never reorder, remove, or rename; only append.
- Appending a new builtin does NOT require a bytecode format-version bump.
  Only format changes do. A version mismatch makes the device reply with
  `"code":"bc-version"`, and the web UI auto-recompiles from source in
  response — see `firmware/src/server.rs` and `web/src/App.svelte`'s handling
  of that code.
- Format depth (blob layout, import-table encoding, decode/validate rules)
  lives in docs/spec/bytecode.md; VM semantics (opcodes, builtin dispatch)
  live in docs/spec/vm.md. Read those before changing either file.

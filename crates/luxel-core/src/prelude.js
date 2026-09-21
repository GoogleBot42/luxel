// Luxel prelude — the six higher-order array/pixel helpers, written in the
// pattern language itself (Gitea #626, docs/jit-design.md §4).
//
// These were `Vm::call_builtin` arms until LXBC v6. They are now ordinary
// pattern functions: the compiler links in only the ones a program actually
// uses (transitively), and on the wire and on the device nothing knows they
// were ever special. Where the call site hands over a literal lambda or a
// named function, the compiler clones the helper for that callback and calls
// it with `CallFn`, so the callback keeps typed parameters.
//
// Semantics are the ones the deleted arms had, argument for argument —
// callback argument order, which array is returned, the length rule in
// `arrayMapTo`, the strictly-greater comparator test in `arraySortBy`.
// `crates/luxel-core/tests/prelude.rs` pins them against golden data
// recorded from the builtins before they were removed. Change nothing here
// without re-running it.
//
// Array lengths are immutable in this VM (nothing resizes an array in
// place), so hoisting `.length` out of the loop condition is exact and
// saves an opcode per element.

function arrayForEach(a, fn) {
  var n = a.length
  var i = 0
  while (i < n) {
    fn(a[i], i, a)
    i = i + 1
  }
  return a
}

function arrayMutate(a, fn) {
  var n = a.length
  var i = 0
  while (i < n) {
    a[i] = fn(a[i], i, a)
    i = i + 1
  }
  return a
}

// Stops at the SHORTER of the two arrays, and hands the callback the SOURCE
// array as its third argument (not the destination). Returns the
// destination. Both lengths are read up front so a non-array destination
// fails even when the source is empty, as the builtin's two-array guard did.
function arrayMapTo(src, dst, fn) {
  var n = src.length
  var m = dst.length
  var i = 0
  while (i < n && i < m) {
    dst[i] = fn(src[i], i, src)
    i = i + 1
  }
  return dst
}

// `acc` is the `init` parameter: with no third argument it starts at 0, and
// an empty array returns it untouched.
function arrayReduce(a, fn, acc) {
  var n = a.length
  var i = 0
  while (i < n) {
    acc = fn(acc, a[i], i, a)
    i = i + 1
  }
  return acc
}

// In-place insertion sort. `cmp(x, y) > 0` means "x sorts after y", so equal
// keys never swap and the sort is stable (Pixel Blaze documents its own sort
// as NOT stable, which leaves us free either way; ours is stable and was
// stable as a builtin too).
function arraySortBy(a, cmp) {
  var n = a.length
  var i = 1
  while (i < n) {
    var key = a[i]
    var j = i
    while (j > 0 && cmp(a[j - 1], key) > 0) {
      a[j] = a[j - 1]
      j = j - 1
    }
    a[j] = key
    i = i + 1
  }
  return a
}

// `pixelCoord(i, axis)` is the one builtin this needed: the mapped
// coordinate of pixel `i` on axis 0/1/2 with the current transform applied,
// which is what the old `mapPixels` arm computed per pixel. Returns 0 (the
// builtin's value, not a return) — the old arm returned 0 too.
function mapPixels(fn) {
  var i = 0
  while (i < pixelCount) {
    fn(i, pixelCoord(i, 0), pixelCoord(i, 1), pixelCoord(i, 2))
    i = i + 1
  }
}

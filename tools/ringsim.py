"""Descriptor-ring state simulator for the Luxel HUB75 atomic swap (Gitea #395).

Models the GDMA walking two 254-descriptor rings, the driver's swap() arming a
flip by rewriting the running ring's tail `next`, and the frame-count ISR
landing it and restoring the old tail. Ticks are one descriptor each.

The invariant under test: `suc_eof` sits only on a ring's last descriptor, so
every EOF-to-EOF interval must be exactly N ticks. Anything shorter means the
engine entered a ring off its head -- the mechanism that loses a displayed
frame while write_frame still reports success.
"""
import random

N = 254            # descriptors per ring
COMPOSE = 216      # 7.4 ms at 34.2 us/descriptor
ISR_LAT_MAX = 8    # ticks the EOF ISR can be delayed (WiFi, critical sections)

class Sim:
    def __init__(self, seed, prefetch=1, isr_lat_max=ISR_LAT_MAX, fast_margin=3):
        self.rnd = random.Random(seed)
        self.prefetch = prefetch          # how many descriptors ahead `next` is read
        self.isr_lat_max = isr_lat_max
        self.fast_margin = fast_margin
        # next[] as absolute descriptor ids; ring r occupies [r*N, r*N+N)
        self.next = [i + 1 for i in range(2 * N)]
        self.next[N - 1] = 0              # ring0 tail -> ring0 head
        self.next[2 * N - 1] = N          # ring1 tail -> ring1 head
        self.pos = 0                      # descriptor being transmitted
        self.t = 0
        self.eof_queue = []               # (fire_tick,) pending ISR runs
        self.eof_pending = False          # raised, ISR not yet run
        # driver state
        self.ring = 0
        self.armed = False
        self.flip_from = 0
        self.eofs = 0
        self.needed = 2
        self.swap_done = True
        self.last_eof_t = None
        self.short = []
        self.long = []
        self.passes = []

    def head(self, r): return r * N
    def tail(self, r): return r * N + N - 1

    # --- the driver's swap(), mirroring firmware/patches isr.rs -------------
    def do_swap(self):
        frm, to = self.ring, 1 - self.ring
        # close target ring on itself
        self.next[self.tail(to)] = self.head(to)
        # THE SWAP: running ring's tail -> other ring's head
        self.next[self.tail(frm)] = self.head(to)
        # decide the landing rule from a probe taken AFTER the store
        cur = self.pos
        idx = cur - self.head(frm)
        needed = 2
        if (not self.eof_pending) and 0 <= idx and idx + self.fast_margin <= N:
            needed = 1
        self.ring, self.flip_from, self.eofs, self.needed = to, frm, 0, needed
        self.armed, self.swap_done = True, False

    # --- the frame-count ISR ------------------------------------------------
    def do_isr(self):
        self.eof_pending = False
        if not self.armed:
            return
        self.eofs += 1
        landed = self.eofs >= self.needed
        if not landed:
            lo, hi = self.head(self.ring), self.head(self.ring) + N
            landed = lo <= self.pos < hi
        if not landed:
            return
        # restore the ring we left so it is self-contained again
        self.next[self.tail(self.flip_from)] = self.head(self.flip_from)
        self.armed, self.swap_done = False, True

    def step(self):
        # The engine reads `next` `prefetch` descriptors before it needs it.
        if self.prefetch > 1 and (self.pos % N) == N - self.prefetch:
            self.latched = self.next[self.tail(self.pos // N)]
        finishing = self.pos
        is_tail = (finishing % N) == N - 1
        nxt = self.next[finishing]
        if self.prefetch > 1 and is_tail and hasattr(self, "latched"):
            nxt = self.latched
        self.pos = nxt
        self.t += 1
        if is_tail:
            self.eof_pending = True
            self.eof_queue.append(self.t + self.rnd.randint(0, self.isr_lat_max))
            if self.last_eof_t is not None:
                dt = self.t - self.last_eof_t
                self.passes.append(dt)
                if dt < N: self.short.append((self.t, dt, self.armed, self.needed))
                if dt > N: self.long.append((self.t, dt))
            self.last_eof_t = self.t
        while self.eof_queue and self.eof_queue[0] <= self.t:
            self.eof_queue.pop(0); self.do_isr()

def run(seed, frames=4000, **kw):
    s = Sim(seed, **kw)
    compose_left, composing = 0, False
    while len(s.passes) < frames:
        if not composing and s.swap_done:
            composing, compose_left = True, COMPOSE
        if composing:
            compose_left -= 1
            if compose_left <= 0:
                composing = False
                s.do_swap()
        s.step()
    return s

if __name__ == "__main__":
    for label, kw in [("baseline (prefetch=1)", {}),
                      ("prefetch=2", {"prefetch": 2}),
                      ("prefetch=8", {"prefetch": 8}),
                      ("isr latency up to 300 ticks", {"isr_lat_max": 300}),
                      ("fast_margin=1 (no safety margin)", {"fast_margin": 1})]:
        tot_s = tot_l = 0; ex = None
        for seed in range(60):
            s = run(seed, **kw)
            tot_s += len(s.short); tot_l += len(s.long)
            if s.short and ex is None: ex = (seed, s.short[:3])
        print(f"{label:34s} short={tot_s:5d}  long={tot_l:5d}" + (f"  e.g. seed {ex[0]}: {ex[1]}" if ex else ""))


# ---------------------------------------------------------------------------
# The row-major ring driver (Gitea #856, docs/hub75-ring-design.md §3, §4, §6)
#
# Mirrors crates/luxel-hub75/src/ring.rs: a ring of N slots, one row pair
# each, emitted row-major (ENTRY + E emissions per slot); an absolute
# emission counter `a` names slot a % N and row pair a % rows; the DMA's
# counter abs_dma is (ring wraps counted by the slot-EOF ISR) × N + the slot
# its position probe reports — and the probe can run one descriptor AHEAD
# (prefetch), which is what GUARD = 2 covers. Two packers claim out of one
# queue word (frame choice in the top bit, decided by the claimer of a pass's
# row 0) and pack a claim only while 1 <= a - abs_dma <= N - GUARD AND the
# slots left before the beam reaches it cover the packer's own worst pack
# time (`until_late`); otherwise the claim is SKIPPED and counted late (§7):
# the slot then shows the row it already held, at that row's own address —
# stale, never a mixed-address row — and the driver backs the schedule off
# after enough of them.
#
# Invariants under test:
#   * no slot is ever written while the DMA is reading it (true position),
#   * every emitted slot carries the row pair a % rows,
#   * every row of a pass reads the same frame buffer (frame-atomic),
#   * with enough slack there are no lates; with too little there are lates
#     (skips) and STILL no writes under the beam.
class RingSim:
    def __init__(self, seed, n=6, rows=32, E=9, eof_every=1, isr_lat_max=8,
                 prefetch=1, guard=2, packers=2, pack_ticks=(3, 6),
                 hold_every=0, hold_ticks=0, render_ticks=200):
        self.rnd = random.Random(seed)
        self.n, self.rows, self.E = n, rows, E
        self.eof_every, self.isr_lat_max = eof_every, isr_lat_max
        self.prefetch, self.guard = prefetch, guard
        self.pack_ticks = pack_ticks
        self.hold_every, self.hold_ticks = hold_every, hold_ticks
        self.render_ticks = render_ticks
        self.t = 0
        self.abs_true = 0            # slot the DMA is reading
        self.tick_in_slot = 0
        self.eofs = 0                # EOFs the ISR has serviced
        self.eof_queue = []          # ISR fire ticks
        self.newest_buf = 0
        # claim word: (buf, a); the ring is pre-filled with rows 0..n
        self.word = (0, n)
        self.filled = {a: (0, 0) for a in range(n)}   # a -> (buf, done tick)
        self.packers = [{"busy_until": 0, "claim": None, "start": 0} for _ in range(packers)]
        self.hold_until = 0
        self.violations, self.lates, self.claims, self.skipped = 0, 0, 0, 0
        self.emitted = []            # (a, buf) per emitted slot, in order
        self.stalls = 0              # ticks a packer was idle with nothing fillable

    # -- the pure rules, as ring.rs spells them --------------------------
    def abs_known(self):
        wraps = (self.eofs * self.eof_every) // self.n
        probe = self.abs_true + (1 if self.prefetch > 1 and self.tick_in_slot >= self.E - self.prefetch + 1 else 0)
        return wraps * self.n + probe % self.n

    def fillable(self, a, abs_dma):
        d = a - abs_dma
        return 1 <= d and d + self.guard <= self.n

    def in_time(self, a, abs_dma):
        # slots left before the beam reaches `a`, minus the one it may be in,
        # against the packer's worst pack — the §7 skip rule
        return (a - abs_dma - 1) * self.E >= self.pack_ticks[1]

    def claim_next(self):
        buf, a = self.word
        if a % self.rows == 0:
            buf = self.newest_buf
        self.word = (buf, a + 1)
        return buf, a

    # -- one tick ----------------------------------------------------------
    def step(self):
        self.t += 1
        if self.t % self.render_ticks == 0:
            self.newest_buf ^= 1
        # the DMA: one emission per tick, a slot every E ticks
        self.tick_in_slot += 1
        if self.tick_in_slot == self.E:
            self.tick_in_slot = 0
            self.abs_true += 1
            f = self.filled.get(self.abs_true)
            if f is None or f[1] > self.t:
                self.lates += 1
                self.emitted.append((self.abs_true, None))
            else:
                self.emitted.append((self.abs_true, f[0]))
            if self.abs_true % self.eof_every == 0:
                self.eof_queue.append(self.t + self.rnd.randint(0, self.isr_lat_max))
        while self.eof_queue and self.eof_queue[0] <= self.t:
            self.eof_queue.pop(0)
            self.eofs += 1
        # core 0's WiFi hold
        if self.hold_every and self.t % self.hold_every == 0:
            self.hold_until = self.t + self.hold_ticks
        # the packers
        abs_dma = self.abs_known()
        for i, p in enumerate(self.packers):
            if p["claim"] is not None:
                if p["busy_until"] <= self.t:
                    buf, a = p["claim"]
                    if a % self.n == self.abs_true % self.n:
                        self.violations += 1
                    self.filled[a] = (buf, self.t)
                    p["claim"] = None
                continue
            if i == 0 and self.t < self.hold_until:
                continue
            buf, a = self.word
            if not self.fillable(a, abs_dma):
                self.stalls += 1
                continue
            if not self.in_time(a, abs_dma):
                # too late to pack it safely: give the claim up (it stays
                # stale in the ring) and count it, as the driver would
                self.claim_next()
                self.skipped += 1
                continue
            buf, a = self.claim_next()
            self.claims += 1
            if a % self.n == self.abs_true % self.n:
                self.violations += 1
            p["claim"] = (buf, a)
            p["busy_until"] = self.t + self.rnd.randint(*self.pack_ticks)

    def run(self, slots=4000):
        while len(self.emitted) < slots:
            self.step()
        return self

    def check(self):
        # every emitted slot is the right row pair (by construction of `a`),
        # and every row of a pass read one buffer
        passes = {}
        for a, buf in self.emitted:
            if buf is None:
                continue
            passes.setdefault(a // self.rows, set()).add(buf)
        mixed = sum(1 for s in passes.values() if len(s) > 1)
        return mixed


def ring_table():
    rows = []
    cases = [
        ("3 ms-ish ring, quiet",          dict(n=6, E=9)),
        ("eof every 2nd slot",            dict(n=6, E=9, eof_every=2)),
        ("prefetching probe",             dict(n=6, E=9, prefetch=2)),
        ("isr latency up to a slot",      dict(n=6, E=9, isr_lat_max=9)),
        ("core 0 held 2 slots every 100", dict(n=6, E=9, hold_every=100, hold_ticks=18)),
        ("stock E=128, n=4",              dict(n=4, E=128, pack_ticks=(30, 60))),
        ("whole-frame ring n=32",         dict(n=32, E=9)),
        ("one packer, tight ring",        dict(n=3, E=9, packers=1, pack_ticks=(3, 6))),
        ("too slow: packs cost a slot",   dict(n=4, E=9, packers=1, pack_ticks=(9, 12))),
        ("held longer than the slack",    dict(n=4, E=9, packers=1, hold_every=60, hold_ticks=40)),
        ("guard 1 + prefetch (unsafe)",   dict(n=6, E=9, prefetch=2, guard=1, pack_ticks=(8, 9))),
        ("isr later than the slack",      dict(n=4, E=9, isr_lat_max=40, pack_ticks=(3, 6))),
    ]
    for label, kw in cases:
        v = l = m = c = k = 0
        for seed in range(20):
            s = RingSim(seed, **kw).run()
            v += s.violations; l += s.lates; m += s.check(); c += s.claims; k += s.skipped
        rows.append((label, kw, v, l, m, c, k))
    return rows


if __name__ == "__main__":
    print()
    print("row-major ring (Gitea #856): writes under the beam / late slots / mixed-frame passes, 20 seeds x 4000 slots")
    bad = 0
    for label, kw, v, l, m, c, k in ring_table():
        unsafe = kw.get("guard", 2) < 2
        flag = ""
        if m: flag += " MIXED-FRAME"
        if v and not unsafe: flag += " WRITE-UNDER-BEAM"
        if unsafe and not v: flag += " (expected violations, saw none)"
        if flag: bad += 1
        print(f"{label:32s} under-beam={v:5d} late={l:5d} skipped={k:5d} mixed={m:3d}{flag}")
    if bad:
        raise SystemExit(f"{bad} ring case(s) broke an invariant")

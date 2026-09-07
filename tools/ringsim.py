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

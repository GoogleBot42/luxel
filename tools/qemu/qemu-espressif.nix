# Espressif's QEMU fork (esp32/esp32s3 machine models), packaged for the
# firmware-emulation spike — see docs/research/qemu-emulation-spike.md for
# what works, what's patched and why, and the one remaining blocker.
# Build: nix build .#qemu-espressif   (flake output, nixpkgs pinned by
# flake.lock — this is the one tools/qemu/takeover-test.py resolves).
# Standalone, against whatever <nixpkgs> is on NIX_PATH — a different store
# path, so prefer the flake unless you have a reason not to:
#   nix build --impure --expr 'import ./tools/qemu/qemu-espressif.nix {}'
{ pkgs ? import <nixpkgs> {} }:
let
  keycodemapdb = pkgs.fetchFromGitHub {
    owner = "qemu"; repo = "keycodemapdb";
    rev = "HEAD";
    hash = "sha256-TDhUF9wxicYezlcJqzXvkrKTt9YjKYiQaHCKNpO6vIU=";
  };
  softfloat = pkgs.fetchFromGitHub {
    owner = "qemu"; repo = "berkeley-softfloat-3"; rev = "HEAD";
    hash = "sha256-Yflpx+mjU8mD5biClNpdmon24EHg4aWBZszbOur5VEA=";
  };
  testfloat = pkgs.fetchFromGitHub {
    owner = "qemu"; repo = "berkeley-testfloat-3"; rev = "HEAD";
    hash = "sha256-LcaBi7gXPEvXyR3YXOcsIlV50ZFSNtI9LXzikaOQbPk=";
  };
  slirpsrc = pkgs.libslirp.src;
in
((pkgs.qemu.override {
  hostCpuTargets = [ "xtensa-softmmu" ];
  guestAgentSupport = false; numaSupport = false; seccompSupport = false;
  alsaSupport = false; pulseSupport = false; pipewireSupport = false;
  sdlSupport = false; jackSupport = false; gtkSupport = false;
  vncSupport = false; smartcardSupport = false; spiceSupport = false;
  ncursesSupport = false; usbredirSupport = false; xenSupport = false;
  cephSupport = false; glusterfsSupport = false; openGLSupport = false;
  rutabagaSupport = false; virglSupport = false; libiscsiSupport = false;
  smbdSupport = false; tpmSupport = false; uringSupport = false;
  canokeySupport = false; capstoneSupport = false; enableDocs = false;
}).overrideAttrs (o: {
  pname = "qemu-espressif";
  version = "9.2.2-esp-20260417";
  src = pkgs.fetchzip {
    url = "https://github.com/espressif/qemu/releases/download/esp-develop-9.2.2-20260417/qemu-esp_develop_9.2.2_20260417-src.tar.xz";
    sha256 = "126631yqvc0s8hj2vb08kakg2npvn2q5vi1hw7nymj6f95h7ab6w";
  };
  # QEMU's esp32 model never implemented DPORT_{PRO,APP}_INTR_STATUS_REG_0..2
  # (0x3FF000EC / 0x3FF000F8) — the per-source pending bitmap that esp-hal's
  # `InterruptStatus::current()` dispatches level-triggered peripheral
  # interrupts from. Reads returned 0, so no handler ran, nothing acked the
  # peripheral, and any esp-rtos guest wedged in an endless level-1 interrupt
  # storm. (ESP-IDF is unaffected: _xt_lowint1 dispatches off the Xtensa
  # INTERRUPT sreg instead.) Too much C for substituteInPlace — carried as a
  # real patch. Also makes a CPU interrupt line shared by several matrix
  # sources deassert only when the last of them drops.
  #
  # And two esp32_timg bugs that only bite a non-IDF guest: the model gated the
  # timer's LEVEL interrupt on TIMG_INT_ENA, but on ESP32 silicon that register
  # is inert and TIMG_Tx_LEVEL_INT_EN in the timer config gates it (esp-hal
  # leaves INT_ENA at 0 forever, so the scheduler tick never fired); and an
  # alarm value already behind the counter silently disarmed instead of firing
  # immediately (espressif/qemu#69).
  #
  # esp32s3-bbpll-cal-done: the esp32s3 machine models no analog-master
  # register block, so esp-hal's BBPLL self-calibration wait
  # (I2C_ANA_MST.ANA_CONF0 bit 24) never completes and EVERY esp-hal guest
  # hangs inside esp_hal::init, before esp-println exists — an app that the
  # bootloader loads and verifies and that then prints nothing at all. That
  # is what kept the 16 MB Seengreat image off the emulator (Gitea #634).
  #
  # esp32s3-appcpu-stall: the esp32s3 machine's cpu-stall handler is an empty
  # stub and SYSTEM_CORE_1_CONTROL_0 is unmodelled, so the APP CPU free-ran
  # from power-on and printed a Guru Meditation on core 1 while core 0 was
  # still booting. The classic esp32 machine has always stalled it properly.
  #
  # esp-timg-div-by-zero: three SIGFPEs in the shared C3/S3 timer-group model,
  # each reachable from an ordinary guest register write (the TIMG_Tx_DIVIDER
  # one fires during esp_rtos::start on every S3 boot).
  #
  # esp32s3-cache-mmu-bounds: a guest MMU entry naming a flash page past the
  # end of the part made the S3 cache model write 64 KiB out of bounds and
  # SIGSEGV. Bound the page and the table index.
  #
  # esp32s3-cpenable-reset: the esp32s3 half of the CPENABLE fix the
  # substituteInPlace below applies to esp32.c — without it every esp-hal S3
  # guest double-faults out of its first FP instruction and boot-loops.
  #
  # m25p80-luxel-fault-injection: opt-in write-fault injection for the
  # takeover reboot-to-retry test (LUXEL_FLAKY_WRITE env var, inert when
  # unset) — takeover-test.py --inject-fault. Harness-side per the
  # isolation rule; the guest image stays byte-identical to shipping.
  patches = [
    ./patches/esp32-dport-intr-status.patch
    ./patches/esp32-timg-level-int.patch
    ./patches/m25p80-luxel-fault-injection.patch
    ./patches/esp32s3-bbpll-cal-done.patch
    ./patches/esp32s3-appcpu-stall.patch
    ./patches/esp-timg-div-by-zero.patch
    ./patches/esp32s3-cache-mmu-bounds.patch
    ./patches/esp32s3-cpenable-reset.patch
  ];
  buildInputs = (o.buildInputs or []) ++ [ pkgs.libgcrypt pkgs.libslirp ];
  configureFlags = (o.configureFlags or []) ++ [ "--enable-gcrypt" ];
  postPatch = (o.postPatch or "") + ''
    # GitHub tarballs don't vendor meson wrap subprojects
    rm -rf subprojects/keycodemapdb
    cp -r ${keycodemapdb} subprojects/keycodemapdb
    chmod -R u+w subprojects/keycodemapdb
    rm -rf subprojects/libslirp
    cp -r ${slirpsrc} subprojects/libslirp
    chmod -R u+w subprojects/libslirp
    rm -rf subprojects/berkeley-softfloat-3 subprojects/berkeley-testfloat-3
    cp -r ${softfloat} subprojects/berkeley-softfloat-3
    cp -r ${testfloat} subprojects/berkeley-testfloat-3
    chmod -R u+w subprojects/berkeley-softfloat-3 subprojects/berkeley-testfloat-3
    for w in berkeley-softfloat-3 berkeley-testfloat-3; do
      if [ -d subprojects/packagefiles/$w ]; then
        cp -r subprojects/packagefiles/$w/* subprojects/$w/
      fi
    done
    # The esp32 RSA model asserts on an early-boot guest init sweep that
    # real hardware tolerates — make it ignore malformed ops instead.
    # the FP test suite doesn't build against a current testfloat — and we
    # don't run it
    echo "" > tests/fp/meson.build
    substituteInPlace hw/misc/esp32_rsa.c \
      --replace-fail "assert(s->rsa_mult_mode_reg >= 8 && s->rsa_mult_mode_reg < 16);" \
                     "if (!(s->rsa_mult_mode_reg >= 8 && s->rsa_mult_mode_reg < 16)) return;" \
      --replace-fail "gcry_mpi_powm(z, x, y, m);" \
                     "if (gcry_mpi_cmp_ui(m, 0)) gcry_mpi_powm(z, x, y, m);" \
      --replace-fail "if (!gcry_mpi_invm(s->cache.rinv, r, m)) {" \
                     "if (gcry_mpi_cmp_ui(m, 0) == 0 || !gcry_mpi_invm(s->cache.rinv, r, m)) {" \
      --replace-fail "size_t n_bytes = (s->rsa_modexp_mode_reg + 1) * 64;" \
                     "size_t n_bytes = (s->rsa_modexp_mode_reg + 1) * 64; if (n_bytes > ESP32_RSA_MEM_BLK_SIZE) return;"
    # same class of bug in the AES model: garbage mode.bits overruns the
    # key copy and the AES round computation
    substituteInPlace hw/misc/esp32_aes.c \
      --replace-fail "    AES_KEY aes_key;" \
                     "    AES_KEY aes_key; if (s->mode.bits != 128 && s->mode.bits != 192 && s->mode.bits != 256) { s->aes_idle_reg = 1; return; }"
    # ESP32 silicon comes out of reset with CPENABLE=0xff (all coprocessors
    # enabled), but generic xtensa cpu_reset only does that under
    # CONFIG_USER_ONLY, so in system mode the first FP instruction traps
    # Cp0Disabled — and xtensa-lx-rt's save_context re-faults spilling
    # f0..f15, turning it into a silent double exception. Re-apply the
    # silicon reset value on both cores. Ref espressif/qemu#154 and the
    # unmerged PR #155 (which only fixes esp32s3).
    substituteInPlace hw/xtensa/esp32.c \
      --replace-fail "        cpu_reset(CPU(&s->cpu[0]));" \
                     "        cpu_reset(CPU(&s->cpu[0]));
            s->cpu[0].env.sregs[CPENABLE] = 0xff;" \
      --replace-fail "        cpu_reset(CPU(&s->cpu[1]));" \
                     "        cpu_reset(CPU(&s->cpu[1]));
            s->cpu[1].env.sregs[CPENABLE] = 0xff;"
  '';
  # espressif's tree force-enables slirp while the meson dep lookup can
  # fail silently — hand the paths straight to cc/ld
  # nixpkgs' postInstall links qemu-kvm -> qemu-system-x86_64, which an
  # xtensa-only build doesn't have
  postInstall = (o.postInstall or "") + ''
    rm -f $out/bin/qemu-kvm
  '';
  env = {
    NIX_CFLAGS_COMPILE = "-I${pkgs.libslirp}/include/slirp";
    NIX_LDFLAGS = "-L${pkgs.libslirp}/lib -lslirp";
  };
  doCheck = false; doInstallCheck = false;
}))

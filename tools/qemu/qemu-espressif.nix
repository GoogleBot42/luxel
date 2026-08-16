# Espressif's QEMU fork (esp32/esp32s3 machine models), packaged for the
# firmware-emulation spike — see docs/research/qemu-emulation-spike.md for
# what works, what's patched and why, and the one remaining blocker.
# Build: nix build --impure --expr 'import ./tools/qemu/qemu-espressif.nix {}'
# TODO: wire into flake.nix as a proper output if/when the takeover
# emulation harness lands (repo norm: toolchains are flake derivations).
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
  patches = [];
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

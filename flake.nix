{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = args:
    args.flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import args.nixpkgs) {
          inherit system;
        };

        runtimeDeps = [
          pkgs.openssl
          pkgs.openssl.dev
        ];

        LD_LIBRARY_PATH = "/run/opengl-driver/lib/:${pkgs.lib.makeLibraryPath runtimeDeps}";

        devShellPkgs = [
          pkgs.cargo-deny
          pkgs.cargo-bloat
          pkgs.cargo-machete
          pkgs.cargo-flamegraph
          pkgs.cargo-udeps
          pkgs.rustfmt
          pkgs.pkg-config
          pkgs.just
          pkgs.cmake
        ] ++ runtimeDeps;

        # Tells Cargo that it should use Wine to run tests.
        # (https://doc.rust-lang.org/cargo/reference/config.html#targettriplerunner)
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER = pkgs.writeShellScript "wine-wrapper" ''
          if [ -z "''${WINEPREFIX+x}" ]; then
            export WINEPREFIX="''${XDG_CACHE_HOME:-$HOME/.cache}/wine-cargo-test-prefix/"
          fi
          echo "Launching $@ with $(command -v wine64) in $WINEPREFIX"

          exec wine $@
        '';
        self = {
          devShells.default = self.devShells.rustup-dev;

          devShells.rustup-dev = pkgs.stdenv.mkDerivation {
            inherit CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER LD_LIBRARY_PATH;
            name = "rustup-dev-shell";

            # Unset flags set by Nix that assume target architecture to allow cross-compilation
            shellHook = ''
              export CC=
              export NIX_CFLAGS_COMPILE=
              export NIX_CFLAGS_COMPILE_FOR_TARGET=
            '';

            depsBuildBuild = with pkgs; [
              pkg-config
            ];

            nativeBuildInputs = with pkgs; [
              mold
              lld
              bubblewrap
            ];

            GLIBC_PATH = "${pkgs.glibc_multi}/lib";

            buildInputs = with pkgs; [
              glibc_multi
              rustup
              libunwind
              pkgsCross.mingwW64.stdenv.cc
            ] ++ devShellPkgs;
          };
        };
      in
      self
    );
}

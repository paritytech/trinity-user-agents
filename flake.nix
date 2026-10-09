{
  description = "TrUAPI development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        stableToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
        };

        nightlyToolchain = pkgs.rust-bin.selectLatestNightlyWith
          (toolchain: toolchain.default);
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            nodejs_22
            yarn
            bun
            wasm-pack
            wasm-bindgen-cli
            binaryen
            git
            pkg-config
            cargo-deny
          ];

          shellHook = ''
            rustup_home="''${RUSTUP_HOME:-''$HOME/.rustup}"
            host_triple="$(${stableToolchain}/bin/rustc --print host-tuple)"
            toolchains_dir="$rustup_home/toolchains"
            mkdir -p "$toolchains_dir"

            # rustup >= 1.28 rejects channel-like names for `rustup toolchain
            # link`, so link the pinned rust-overlay toolchains directly under
            # the official channel directory names. `cargo +stable` and
            # `cargo +nightly` then dispatch to the pinned store paths instead
            # of a network download.
            #
            # Note: toolchains linked to read-only store paths cannot carry a
            # dist manifest, so manifest-dependent introspection (`rustup
            # show`, `rustup target list --toolchain <channel>`) errors with
            # "Missing manifest in toolchain". Use `rustup toolchain list`,
            # `rustup show active-toolchain`, and `rustc +<channel> --version`
            # instead; compilation and `+<channel>` dispatch are unaffected.
            for name in stable nightly; do
              if [ "$name" = stable ]; then src="${stableToolchain}"; else src="${nightlyToolchain}"; fi
              dir="$toolchains_dir/$name-$host_triple"
              [ -L "$dir" ] || rm -rf -- "$dir"
              ln -sfn "$src" "$dir"
            done

            rustup default stable
          '';
        };
      });
}

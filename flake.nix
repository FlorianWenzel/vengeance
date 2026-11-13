{
  description = "Vengeance: a self-hosted security inbox";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    git-hooks,
    ...
  }: let
    inherit (nixpkgs) lib;
    forEachSystem = f:
      lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ] (system: f nixpkgs.legacyPackages.${system});

    # The Go release line go.mod asks for, or the newest Go when nixpkgs' copy of that line
    # is older than the requested patch release.
    goFor = pkgs: let
      line = lib.findFirst (lib.hasPrefix "go ") "go 0" (lib.splitString "\n" (builtins.readFile ./go.mod));
      want = lib.removePrefix "go " line;
      attr = "go_" + lib.concatStringsSep "_" (lib.take 2 (lib.splitVersion want));
    in
      if pkgs ? ${attr} && lib.versionAtLeast pkgs.${attr}.version want
      then pkgs.${attr}
      else pkgs.go;

    # The wasm-bindgen CLI has to match the crate version in viz/Cargo.lock exactly.
    wasmBindgenFor = pkgs: let
      inherit (lib.findFirst (p: p.name == "wasm-bindgen") {version = "0";} (lib.importTOML ./viz/Cargo.lock).package) version;
      attr = "wasm-bindgen-cli_" + lib.replaceStrings ["."] ["_"] version;
    in
      assert lib.assertMsg (pkgs ? ${attr})
      "viz/Cargo.lock pins wasm-bindgen ${version}, which this nixpkgs does not ship yet: run `nix flake update`";
        pkgs.${attr};

    # Rewrites vendorHash in package.nix. Shared by the git hook, the nix-vendor-hash
    # workflow and `nix run .#vendor-hash`.
    vendorHashFor = pkgs:
      pkgs.writeShellApplication {
        name = "vendor-hash";
        runtimeInputs = [pkgs.git pkgs.gnused];
        text = ''
          cd "$(git rev-parse --show-toplevel)"
          # Building with the current hash proves nothing: Nix reuses a cached result even
          # after go.sum changed. So fetch with a fake hash and read the real one from the report.
          if log=$(nix --extra-experimental-features 'nix-command flakes' build --no-link .#vengeance.vendorHashProbe 2>&1); then
            echo "vendor-hash: the probe build was expected to report a hash mismatch" >&2
            exit 1
          fi
          hash=$(sed -n 's|^ *got: *\(sha256-[A-Za-z0-9+/=]*\).*|\1|p' <<<"$log" | head -n 1)
          if [ -z "$hash" ]; then
            echo "$log" >&2
            exit 1
          fi
          sed -i "s|vendorHash = \"sha256-[A-Za-z0-9+/=]*\";|vendorHash = \"$hash\";|" package.nix
        '';
      };

    hooksConfig = pkgs:
      import ./git-hooks.nix {
        inherit pkgs;
        go = goFor pkgs;
        vendor-hash = vendorHashFor pkgs;
      };
    hooksFor = pkgs: git-hooks.lib.${pkgs.stdenv.hostPlatform.system}.run;
  in {
    formatter = forEachSystem (pkgs: pkgs.alejandra);

    packages = forEachSystem (pkgs: let
      vengeance = pkgs.callPackage ./package.nix {
        go = goFor pkgs;
        wasm-bindgen-cli = wasmBindgenFor pkgs;
        version = self.shortRev or self.dirtyShortRev or "dev";
      };
    in {
      default = vengeance;
      inherit vengeance;
      inherit (vengeance.passthru) viz web;
    });

    apps = forEachSystem (pkgs: {
      vendor-hash = {
        type = "app";
        program = lib.getExe (vendorHashFor pkgs);
        meta.description = "Refresh vendorHash in package.nix after go.mod or go.sum changed";
      };
    });

    checks = forEachSystem (pkgs: let
      inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) vengeance;
      module = self.nixosModules.default;
    in
      {
        # The build runs go vet, the Go tests (against PostgreSQL on Linux), the renderer
        # tests and the UI type check.
        package = vengeance;

        pre-commit = hooksFor pkgs (lib.recursiveUpdate (hooksConfig pkgs) {
          # These two need the network: `clippy` below lints, the package build checks the hash.
          hooks.clippy.enable = false;
          hooks.vendor-hash.enable = false;
        });

        # The renderer for the machine running the tests and for the browser
        clippy = vengeance.viz.overrideAttrs (old: {
          pname = "vengeance-blueprint-clippy";
          nativeBuildInputs = old.nativeBuildInputs ++ [pkgs.clippy];
          buildPhase = ''
            cd viz
            cargo clippy --offline --all-targets -- -D warnings
            cargo clippy --offline --target wasm32-unknown-unknown -- -D warnings
          '';
          doCheck = false;
          installPhase = "touch $out";
        });
      }
      // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        nixos-module = import ./nixos-test.nix {inherit pkgs module;};
        e2e = import ./e2e/check.nix {inherit pkgs module;};
      });

    nixosModules.default = import ./nixos-module.nix self;

    devShells = forEachSystem (pkgs: let
      wasm-bindgen-cli = wasmBindgenFor pkgs;
      hooks = hooksFor pkgs (hooksConfig pkgs);
    in {
      default = pkgs.mkShell {
        packages = with pkgs;
          [
            # Go server
            (goFor pkgs)
            gopls

            # Vue UI and the e2e suite
            nodejs_24
            prettier

            # WASM blueprint renderer (viz/)
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            lld
            wasm-bindgen-cli

            # psql for the compose stack's database
            postgresql_17

            # Nix tooling
            alejandra
            deadnix
            statix
          ]
          ++ hooks.enabledPackages;
        env = {
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          # scripts/build-viz.sh uses this instead of searching PATH and ~/.cargo/bin
          WASM_BINDGEN = lib.getExe wasm-bindgen-cli;
        };
        inherit (hooks) shellHook;
      };
    });
  };
}

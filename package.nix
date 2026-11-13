{
  lib,
  stdenv,
  buildGoModule,
  buildNpmPackage,
  importNpmLock,
  rustPlatform,
  cargo,
  rustc,
  lld,
  nodejs_24,
  postgresql_17,
  postgresqlTestHook,
  # Chosen in flake.nix to match go.mod and viz/Cargo.lock
  go,
  wasm-bindgen-cli,
  version ? "dev",
}: let
  fs = lib.fileset;
  inherit (stdenv.hostPlatform) isLinux;

  # WASM blueprint renderer, built by the same script everyone else uses
  viz = stdenv.mkDerivation {
    pname = "vengeance-blueprint";
    inherit ((lib.importTOML ./viz/Cargo.toml).package) version;

    src = fs.toSource {
      root = ./.;
      fileset = fs.unions [
        ./viz/Cargo.toml
        ./viz/Cargo.lock
        ./viz/src
        ./viz/fixtures
        ./scripts/build-viz.sh
      ];
    };

    # Cargo.lock drives the fetches, so there is no dependency hash to maintain
    cargoDeps = rustPlatform.importCargoLock {lockFile = ./viz/Cargo.lock;};
    cargoRoot = "viz";

    nativeBuildInputs = [
      cargo
      rustc
      lld
      rustPlatform.cargoSetupHook
      wasm-bindgen-cli
    ];
    # No binaryen on purpose: with it the script runs `wasm-opt --all-features`, and current
    # binaryen then emits an import encoding that browsers reject ("Invalid import kind").
    # The container image and the release binaries are built without wasm-opt as well.

    buildPhase = ''
      runHook preBuild
      (cd viz && bash ../scripts/build-viz.sh)
      runHook postBuild
    '';

    # The renderer's unit tests run on the build machine, not in WASM
    doCheck = true;
    checkPhase = ''
      runHook preCheck
      (cd viz && cargo test --offline)
      runHook postCheck
    '';

    installPhase = ''
      runHook preInstall
      cp -r web/src/viz/pkg $out
      runHook postInstall
    '';
  };

  # Vue single-page app, embedded into the Go binary
  web = buildNpmPackage {
    pname = "vengeance-web";
    inherit (lib.importJSON ./web/package.json) version;

    src = fs.toSource {
      root = ./web;
      fileset = fs.difference ./web (fs.unions [
        (fs.maybeMissing ./web/node_modules)
        (fs.maybeMissing ./web/dist)
        (fs.maybeMissing ./web/src/viz/pkg)
        ./web/embed.go
      ]);
    };

    nodejs = nodejs_24;
    # package-lock.json drives the fetches, so there is no dependency hash to maintain
    npmDeps = importNpmLock {npmRoot = ./web;};
    inherit (importNpmLock) npmConfigHook;

    preBuild = ''
      mkdir -p src/viz
      cp -r ${viz} src/viz/pkg
      chmod -R u+w src/viz/pkg
    '';

    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
  };
in
  (buildGoModule.override {inherit go;}) (finalAttrs: {
    pname = "vengeance";
    inherit version;

    src = fs.toSource {
      root = ./.;
      fileset = fs.unions [
        ./go.mod
        ./go.sum
        ./cmd
        ./internal
        ./web/embed.go
      ];
    };

    # The one hash in this repo. It changes with go.sum and is refreshed by the vendor-hash
    # git hook and the nix-vendor-hash workflow, or by hand: `nix run .#vendor-hash`
    vendorHash = "sha256-WA+boiUE/zGcogWoYGNUgRdzF82jh4FdMak4WvQC9xA=";
    # Fetching the modules needs neither the UI nor a name that changes with every commit
    overrideModAttrs = _: {
      name = "vengeance-go-modules";
      preBuild = "";
    };

    subPackages = ["cmd/vengeance"];
    env.CGO_ENABLED = 0;
    ldflags = [
      "-s"
      "-w"
      "-X main.version=${version}"
    ];

    # web/embed.go embeds web/dist
    preBuild = ''
      mkdir -p web/dist
      cp -r ${web}/. web/dist/
      chmod -R u+w web/dist
    '';

    # Vet and the whole test suite, like the `go` CI job but without -race (cgo is off).
    # On Linux the database tests run against a throwaway PostgreSQL; elsewhere they skip.
    nativeCheckInputs = lib.optionals isLinux [postgresql_17 postgresqlTestHook];
    postgresqlTestUserOptions = "LOGIN SUPERUSER";
    __darwinAllowLocalNetworking = true;
    checkPhase = ''
      runHook preCheck
      ${lib.optionalString isLinux ''export VENGEANCE_TEST_DATABASE_URL="postgres:///$PGDATABASE?host=$PGHOST&user=$PGUSER"''}
      go vet ./...
      go test -p 1 -count=1 ./...
      runHook postCheck
    '';

    passthru = {
      inherit viz web;
      # The module fetch with a hash that cannot match, so that building it reports the
      # real one. Used by `nix run .#vendor-hash`.
      vendorHashProbe = finalAttrs.finalPackage.goModules.overrideAttrs (_: {outputHash = lib.fakeHash;});
    };

    meta = {
      description = "Self-hosted security inbox";
      homepage = "https://github.com/FlorianWenzel/vengeance";
      license = lib.licenses.agpl3Only;
      mainProgram = "vengeance";
    };
  })

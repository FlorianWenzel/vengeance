# The Playwright suite against the whole stack in a NixOS VM: Vengeance through its NixOS
# module, PostgreSQL, Dex and Mailpit. The counterpart of `docker compose up` + `npx playwright test`.
{
  pkgs,
  module,
}: let
  inherit (pkgs) lib;
  fs = lib.fileset;

  # The suite plus the scanner reports it uploads (../../internal/ingest/testdata)
  src = fs.toSource {
    root = ../.;
    fileset = fs.unions [
      ./package.json
      ./playwright.config.ts
      ./tests
      ./demo
      ../internal/ingest/testdata
    ];
  };

  # @playwright/test exactly as locked in package-lock.json
  nodeModules = pkgs.importNpmLock.buildNodeModules {
    npmRoot = ./.;
    nodejs = pkgs.nodejs_24;
  };

  # The suite's own config, with Chromium from nixpkgs instead of Playwright's download.
  # That keeps this check independent of which Playwright version the lockfile pins.
  config = pkgs.writeText "playwright.nix.config.ts" ''
    import base from './playwright.config';

    export default {
      ...base,
      reporter: 'list',
      use: {
        ...base.use,
        launchOptions: {
          ...base.use?.launchOptions,
          executablePath: '${lib.getExe pkgs.chromium}',
          // WebGL for the blueprint without a GPU: software rendering, see xvfb-run below
          args: [...(base.use?.launchOptions?.args ?? []), '--enable-unsafe-swiftshader'],
        },
      },
    };
  '';
in
  pkgs.testers.runNixOSTest {
    name = "vengeance-e2e";

    nodes.machine = {
      imports = [module];

      virtualisation = {
        memorySize = 4096;
        cores = 4;
      };

      # The OIDC issuer has to be the same URL for the server and the browser.
      networking.hosts."127.0.0.1" = ["dex"];

      # Dex with the accounts and client the suite logs in with, shared with compose.yml
      systemd.services.dex = {
        wantedBy = ["multi-user.target"];
        serviceConfig = {
          ExecStart = "${lib.getExe' pkgs.dex-oidc "dex"} serve ${../deploy/dev/dex.yaml}";
          DynamicUser = true;
        };
      };

      # Defaults are what the suite expects: UI and API on 8025, SMTP on 1025
      services.mailpit.instances.default = {};

      services.vengeance = {
        enable = true;
        settings = {
          VENGEANCE_BASE_URL = "http://localhost:8080";
          VENGEANCE_OIDC_ISSUER = "http://dex:5556/dex";
          VENGEANCE_OIDC_CLIENT_ID = "vengeance";
          VENGEANCE_OIDC_ALLOWED_EMAILS = ["admin@example.com"];
          VENGEANCE_SMTP_HOST = "127.0.0.1";
          VENGEANCE_SMTP_PORT = 1025;
          VENGEANCE_SMTP_TLS = "none";
          VENGEANCE_SMTP_FROM = "Vengeance <vengeance@example.com>";
          VENGEANCE_KEV_URL = "off";
        };
        # Test only: secrets in the Nix store are fine here and nowhere else.
        secretFiles = {
          VENGEANCE_SECRET = pkgs.writeText "vengeance-secret" "test-only-secret-0123456789abcdef0123";
          VENGEANCE_OIDC_CLIENT_SECRET = pkgs.writeText "vengeance-oidc-secret" "dev-secret";
        };
      };

      environment.systemPackages = [pkgs.nodejs_24 pkgs.xvfb-run];
      fonts.packages = [pkgs.dejavu_fonts];
    };

    testScript = ''
      machine.wait_for_unit("dex.service")
      machine.wait_for_unit("mailpit-default.service")
      machine.wait_for_unit("vengeance.service")
      machine.wait_until_succeeds("curl -fsS http://127.0.0.1:8080/readyz")
      machine.wait_until_succeeds("curl -fsS http://dex:5556/dex/.well-known/openid-configuration")

      machine.succeed(
          "cp -r ${src} /tmp/src && chmod -R u+w /tmp/src",
          "ln -s ${nodeModules}/node_modules /tmp/src/e2e/node_modules",
          "cp ${config} /tmp/src/e2e/playwright.nix.config.ts",
      )
      # Chromium stays headless, but its software WebGL needs an X display to initialise.
      status, output = machine.execute(
          "cd /tmp/src/e2e && HOME=/tmp xvfb-run -a node node_modules/@playwright/test/cli.js test -c playwright.nix.config.ts 2>&1"
      )
      print(output)
      assert status == 0, "the Playwright suite failed"
    '';
  }

# VM test: the NixOS module runs Vengeance against a local PostgreSQL with peer authentication.
{
  pkgs,
  module,
}:
pkgs.testers.runNixOSTest {
  name = "vengeance-module";

  nodes.machine = {
    imports = [module];
    services.vengeance = {
      enable = true;
      settings = {
        VENGEANCE_BASE_URL = "http://localhost:8080";
        # Discovery only happens at login, so the issuer does not have to exist here.
        VENGEANCE_OIDC_ISSUER = "http://idp.invalid";
        VENGEANCE_OIDC_CLIENT_ID = "vengeance";
        VENGEANCE_OIDC_ALLOWED_EMAILS = ["admin@example.com"];
        VENGEANCE_KEV_URL = "off";
      };
      # Test only: a secret in the Nix store is fine here and nowhere else.
      secretFiles.VENGEANCE_SECRET = pkgs.writeText "vengeance-secret" "test-only-secret-0123456789abcdef0123";
    };
  };

  testScript = ''
    machine.wait_for_unit("vengeance.service")
    machine.wait_until_succeeds("curl -fsS http://127.0.0.1:8080/readyz")
    # The embedded UI is served, and the API is behind login
    machine.succeed("curl -fsS http://127.0.0.1:8080/ | grep -q '<title>'")
    machine.succeed("test $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8080/api/v1/me) = 401")
    # Migrations ran as the peer-authenticated role
    machine.succeed("sudo -u postgres psql -d vengeance -tAc 'select count(*) from goose_db_version' | grep -qv '^0$'")
    # Settings are plain environment, the secret is only a credential
    machine.succeed("systemctl show vengeance -p Environment | grep -q VENGEANCE_BASE_URL")
    machine.fail("systemctl show vengeance -p Environment | grep -q VENGEANCE_SECRET")
    print(machine.succeed("systemd-analyze security vengeance.service | tail -1"))
  '';
}

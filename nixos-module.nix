self: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.vengeance;

  # The server reads only environment variables: lists are comma-separated, booleans "true"/"false"
  toEnv = value:
    if lib.isList value
    then lib.concatStringsSep "," value
    else if lib.isBool value
    then lib.boolToString value
    else toString value;

  # The static binary cannot look up the DynamicUser's name, so the role is spelled out.
  localDatabaseUrl = "postgres:///vengeance?host=/run/postgresql&user=vengeance";

  # Never in `settings`: those values end up world-readable in the Nix store.
  secretNames = [
    "VENGEANCE_SECRET"
    "VENGEANCE_OIDC_CLIENT_SECRET"
    "VENGEANCE_SMTP_PASSWORD"
  ];

  # The server has no *_FILE variables, so systemd hands the files over as credentials and
  # this wrapper exports them. They never appear in the unit's Environment=.
  start = pkgs.writeShellScript "vengeance-start" ''
    set -eu
    ${lib.concatMapStringsSep "\n" (name: ''export ${name}="$(< "$CREDENTIALS_DIRECTORY/${name}")"'') (lib.attrNames cfg.secretFiles)}
    exec ${lib.getExe cfg.package} serve
  '';
in {
  options.services.vengeance = {
    enable = lib.mkEnableOption "Vengeance, a self-hosted security inbox";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "vengeance.packages.\${system}.default";
      description = "The Vengeance package to run.";
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = ''
        Address to listen on. Vengeance speaks plain HTTP and trusts `X-Forwarded-For`, so
        keep it behind a TLS-terminating reverse proxy.
      '';
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "Port to listen on.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open `port` in the firewall.";
    };

    database.createLocally = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Run PostgreSQL on this host with a `vengeance` database and role, and connect over
        the Unix socket with peer authentication. Disable it to use another database and
        provide `VENGEANCE_DATABASE_URL` through `settings` (no password in the URL) or
        `secretFiles` (URL with a password).
      '';
    };

    settings = lib.mkOption {
      description = ''
        Environment variables for `vengeance serve`, as documented in the README. Lists are
        joined with commas. Anything the server understands can be set here, also variables
        this module does not know about. Secrets belong in `secretFiles`.
      '';
      example = lib.literalExpression ''
        {
          VENGEANCE_BASE_URL = "https://vengeance.example.com";
          VENGEANCE_OIDC_ISSUER = "https://auth.example.com";
          VENGEANCE_OIDC_CLIENT_ID = "vengeance";
          VENGEANCE_OIDC_ALLOWED_DOMAINS = ["example.com"];
          VENGEANCE_SMTP_HOST = "smtp.example.com";
          VENGEANCE_SMTP_FROM = "Vengeance <vengeance@example.com>";
        }
      '';
      default = {};
      type = lib.types.submodule {
        freeformType = lib.types.attrsOf (lib.types.nullOr (lib.types.oneOf [
          lib.types.str
          lib.types.int
          lib.types.bool
          (lib.types.listOf lib.types.str)
        ]));

        options = {
          VENGEANCE_BASE_URL = lib.mkOption {
            type = lib.types.str;
            example = "https://vengeance.example.com";
            description = "Public URL of this instance. Used for OIDC redirects, cookies and links in notifications.";
          };
          VENGEANCE_OIDC_ISSUER = lib.mkOption {
            type = lib.types.str;
            example = "https://auth.example.com";
            description = "OIDC issuer URL.";
          };
          VENGEANCE_OIDC_CLIENT_ID = lib.mkOption {
            type = lib.types.str;
            example = "vengeance";
            description = "OIDC client ID.";
          };
          VENGEANCE_OIDC_ALLOWED_EMAILS = lib.mkOption {
            type = lib.types.listOf lib.types.str;
            default = [];
            description = "Email addresses allowed to sign in.";
          };
          VENGEANCE_OIDC_ALLOWED_DOMAINS = lib.mkOption {
            type = lib.types.listOf lib.types.str;
            default = [];
            description = "Email domains allowed to sign in.";
          };
          VENGEANCE_OIDC_ALLOW_ALL = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Let in everyone the provider authenticates. Only when the provider itself restricts who can sign in.";
          };
        };
      };
    };

    secretFiles = lib.mkOption {
      type = lib.types.attrsOf lib.types.path;
      default = {};
      example = lib.literalExpression ''
        {
          VENGEANCE_SECRET = config.age.secrets.vengeance-secret.path;
          VENGEANCE_OIDC_CLIENT_SECRET = config.age.secrets.vengeance-oidc.path;
        }
      '';
      description = ''
        Environment variables whose values are read from files when the service starts, for
        secrets. `VENGEANCE_SECRET` (session key, at least 32 characters) is required. The
        files are read as root and must not live in the Nix store.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.secretFiles ? VENGEANCE_SECRET;
        message = "services.vengeance.secretFiles.VENGEANCE_SECRET is required: a file holding the session secret (at least 32 characters).";
      }
      {
        assertion = lib.all (name: !(cfg.settings ? ${name})) secretNames;
        message = "services.vengeance.settings must not contain ${lib.concatStringsSep ", " secretNames}: use services.vengeance.secretFiles so they stay out of the Nix store.";
      }
      {
        assertion = cfg.settings.VENGEANCE_OIDC_ALLOW_ALL || cfg.settings.VENGEANCE_OIDC_ALLOWED_EMAILS != [] || cfg.settings.VENGEANCE_OIDC_ALLOWED_DOMAINS != [];
        message = "services.vengeance.settings: set VENGEANCE_OIDC_ALLOWED_EMAILS or VENGEANCE_OIDC_ALLOWED_DOMAINS (or VENGEANCE_OIDC_ALLOW_ALL if the provider itself restricts who can sign in).";
      }
      {
        assertion = (cfg.settings.VENGEANCE_DATABASE_URL or null) != null || cfg.secretFiles ? VENGEANCE_DATABASE_URL;
        message = "services.vengeance: without database.createLocally, set VENGEANCE_DATABASE_URL in settings or secretFiles.";
      }
    ];

    services.vengeance.settings = {
      VENGEANCE_LISTEN = lib.mkDefault "${cfg.listenAddress}:${toString cfg.port}";
      VENGEANCE_DATABASE_URL = lib.mkIf cfg.database.createLocally (lib.mkDefault localDatabaseUrl);
    };

    services.postgresql = lib.mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = ["vengeance"];
      ensureUsers = [
        {
          name = "vengeance";
          ensureDBOwnership = true;
        }
      ];
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [cfg.port];

    systemd.services.vengeance = {
      description = "Vengeance security inbox";
      wantedBy = ["multi-user.target"];
      wants = ["network-online.target"];
      # postgresql.target is reached once the database and role exist, not just the server.
      after = ["network-online.target"] ++ lib.optional cfg.database.createLocally "postgresql.target";
      requires = lib.optional cfg.database.createLocally "postgresql.target";
      environment = lib.mapAttrs (_: toEnv) (lib.filterAttrs (_: value: value != null) cfg.settings);

      serviceConfig = {
        ExecStart = start;
        Restart = "on-failure";
        RestartSec = 5;
        LoadCredential = lib.mapAttrsToList (name: path: "${name}:${path}") cfg.secretFiles;

        # All state lives in PostgreSQL. The user name has to match the database role for
        # peer authentication.
        DynamicUser = true;
        User = "vengeance";
        Group = "vengeance";

        CapabilityBoundingSet = "";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        # AF_UNIX for the PostgreSQL socket
        RestrictAddressFamilies = ["AF_INET" "AF_INET6" "AF_UNIX"];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = ["@system-service" "~@privileged"];
        UMask = "0077";
      };
    };
  };
}

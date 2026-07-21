{
  debug = true;
  perSystem = { config, lib, pkgs, self', ... }:
    let
      developmentToml = lib.importTOML ../../../config/development.toml;
      databaseUrl =
        "postgres://${developmentToml.database.username}:${developmentToml.database.password}@${developmentToml.database.host}:${toString developmentToml.database.port}/${developmentToml.database.dbname}";
      opensslEnv = ''
        export OPENSSL_DIR="${pkgs.openssl.dev}"
        export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
        export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
        export OPENSSL_ROOT_DIR="${pkgs.openssl.dev}"
        export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.postgresql}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        if [ -z "''${DATABASE_URL:-}" ] && [ -f .env ]; then
          export DATABASE_URL="$(sed -n 's/^DATABASE_URL=//p' .env | tail -n1)"
        fi
        export DATABASE_URL="''${DATABASE_URL:-${databaseUrl}}"
      '';
    in
    {
      devShells = {
        default =
          pkgs.mkShell {
            name = "card-vault-shell";
            meta.description = "Environment for Hyperswitch Card Vault development";
            inputsFrom = [
              config.pre-commit.devShell
              self'.devShells.rust
            ];
            packages = with pkgs; [
              diesel-cli
              just
              jq
              openssl
              pkg-config
              postgresql # for libpq
              protobuf
              awscli2
            ];
            shellHook = ''
              ${opensslEnv}
            '';
          };
        dev = pkgs.mkShell {
          name = "card-vault-dev-shell";
          meta.description = "Environment for Card Vault development and CI parity checks";
          inputsFrom = [ self'.devShells.default ];
          packages = with pkgs; [
            cargo-watch
            cargo-hack
            cargo-nextest
            nixd
            typos
            yq-go
          ];
          shellHook = ''
            echo 1>&2 "Ready to work on hyperswitch-card-vault!"
            rustc --version
            ${opensslEnv}
          '';
        };
        qa = pkgs.mkShell {
          name = "card-vault-loadtest-shell";
          meta.description = "Environment for Card Vault load testing";
          inputsFrom = [ self'.devShells.dev ];
          packages = with pkgs; [
            k6
            parallel
          ];
        };
      };
    };
}

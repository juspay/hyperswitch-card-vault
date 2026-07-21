{ inputs, lib, ... }:
{
  imports = [
    inputs.process-compose-flake.flakeModule
  ];
  perSystem = { ... }: {
    /* For running external services
        - Postgres
    */
    process-compose."ext-services" =
      let
        developmentToml = lib.importTOML (inputs.self + /config/development.toml);
        databaseName = developmentToml.database.dbname;
        databaseUser = developmentToml.database.username;
        databasePass = developmentToml.database.password;
        databasePort = developmentToml.database.port;
      in
      {
        imports = [ inputs.services-flake.processComposeModules.default ];
        # Card Vault caching is in-memory (`moka`), so only Postgres is required locally.

        /* Postgres
            - Create an user and grant all privileges
            - Create a database
        */
        services.postgres."p1" = {
          enable = true;
          port = databasePort;
          initialScript = {
            before = "CREATE USER ${databaseUser} WITH PASSWORD '${databasePass}' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN;";
            after = "GRANT ALL PRIVILEGES ON DATABASE ${databaseName} to ${databaseUser};";
          };
          initialDatabases = [
            { name = databaseName; }
          ];
        };
      };
  };
}

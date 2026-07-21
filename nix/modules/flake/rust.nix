{ inputs, ... }:
{
  imports = [
    inputs.rust-flake.flakeModules.default
    inputs.rust-flake.flakeModules.nixpkgs
  ];
  perSystem = { pkgs, lib, ... }:
    let
      cargoToml = lib.importTOML (inputs.self + /Cargo.toml);
      cargoMsrv =
        if cargoToml ? workspace
        then cargoToml.workspace.package.rust-version
        else cargoToml.package.rust-version;
      rustChannel =
        if builtins.match "^[0-9]+\\.[0-9]+$" cargoMsrv != null
        then "${cargoMsrv}.0"
        else cargoMsrv;
    in
    {
      rust-project.toolchain = pkgs.rust-bin.fromRustupToolchain {
        channel = rustChannel;
      };
    };
}

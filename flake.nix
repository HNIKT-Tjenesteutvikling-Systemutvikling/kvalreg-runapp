{
  description = "Sets up environment for running the application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    pre-commit-hooks-nix = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.pre-commit-hooks-nix.flakeModule ];

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        { config, pkgs, ... }:
        {
          packages.runapp = pkgs.callPackage ./package.nix { };
          packages.default = config.packages.runapp;

          apps.runapp = {
            type = "app";
            program = "${config.packages.runapp}/bin/runapp";
            meta = { inherit (config.packages.runapp.meta) description; };
          };
          apps.default = config.apps.runapp;

          formatter = pkgs.nixfmt;

          pre-commit.settings.settings.rust.check.cargoDeps = pkgs.rustPlatform.importCargoLock {
            lockFile = ./Cargo.lock;
          };

          pre-commit.settings.hooks = {
            clippy = {
              enable = true;
              packageOverrides = { inherit (pkgs) cargo clippy; };
              settings.denyWarnings = true;
            };
            rustfmt = {
              enable = true;
              packageOverrides = { inherit (pkgs) cargo rustfmt; };
            };
            deadnix.enable = true;
            nixfmt.enable = true;
            statix.enable = true;
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ config.pre-commit.devShell ];
            packages = with pkgs; [
              cargo
              clippy
              rust-analyzer
              rustc
              rustfmt
            ];
          };
        };
    };
}

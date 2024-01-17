{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    { self
    , nixpkgs
    , flake-utils
    , ...
    }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;
      };

      runapp = pkgs.rustPlatform.buildRustPackage rec {
        pname = "runapp";
        version = "0.1.0";
        src = ./.;
        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        buildInputs = [ pkgs.rustc ];
      };
    in
    {
      packages.runapp = runapp;
      packages.default = self.packages.${system}.runapp;
      apps.runapp = {
        type = "app";
        program = "${self.packages.${system}.runapp}/bin/runapp";
      };

      apps.default = self.apps.${system}.runapp;
      defaultPackage = self.packages.${system}.default;

      devShell =
        let
          generateEditorConfig = pkgs.writeShellScriptBin "generateEditorConfig" ''
            if [ ! -f .editorconfig ]; then
              echo "root = true" > .editorconfig
              echo "" >> .editorconfig
              echo "[*]" >> .editorconfig
              echo "end_of_line = lf" >> .editorconfig
              echo "insert_final_newline = true" >> .editorconfig
              echo "indent_style = space" >> .editorconfig
              echo "tab_width = 4" >> .editorconfig
              echo "charset = utf-8" >> .editorconfig
              echo "" >> .editorconfig
              echo "[*.{yaml,yml,html,js,json}]" >> .editorconfig
              echo "indent_style = space" >> .editorconfig
              echo "indent_size = 2" >> .editorconfig
              echo "" >> .editorconfig
              echo "[*.{md,nix}]" >> .editorconfig
              echo "indent_style = space" >> .editorconfig
              echo "indent_size = 2" >> .editorconfig
            fi
          '';
        in
        pkgs.mkShell {
          name = "bloggen-2.0-dev";
          buildInputs = with pkgs; [
            rustc
            cargo
            rust-analyzer
          ];
          shellHook = ''
            ${generateEditorConfig}/bin/generateEditorConfig
          '';
        };
    });
}

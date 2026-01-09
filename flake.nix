{
  description = "KOReader sync server with extended annotation support";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  nixConfig = {
    extra-substituters = [ "https://solastrius.cachix.org" ];
    extra-trusted-public-keys = [ "solastrius.cachix.org-1:MawFli42h9VuWjlURZvxDG+M/tfUbELRwU+QN/6VvdM=" ];
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        commonArgs = {
          pname = "kosync-server";
          version = "0.1.0";
          src = ./server;
          cargoLock.lockFile = ./server/Cargo.lock;
          meta = with pkgs.lib; {
            description = "KOReader sync server with extended annotation support";
            homepage = "https://github.com/SolAstrius/kosync-rs";
            license = licenses.agpl3Only;
            mainProgram = "kosync-server";
          };
        };
      in
      {
        packages = {
          default = pkgs.rustPlatform.buildRustPackage commonArgs;
          static = pkgs.pkgsStatic.rustPlatform.buildRustPackage commonArgs;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = with pkgs; [ cargo rustc rust-analyzer clippy rustfmt ];
        };
      }
    );
}

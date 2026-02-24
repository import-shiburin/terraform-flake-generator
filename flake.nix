{
  description = "terraform-flake-generator build environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        muslPkgs = pkgs.pkgsCross.musl64;
      in
      {
        packages.default = pkgs.callPackage ./default.nix { };

        packages.static = (muslPkgs.callPackage ./default.nix { }).overrideAttrs {
          RUSTFLAGS = "-C target-feature=+crt-static";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
          ];
        };
      }
    ) // {
      overlays.default = final: prev: {
        tfg = final.callPackage ./default.nix { };
      };

      nixosModules.default = import ./nix/nixos-module.nix self;

      homeManagerModules.default = import ./nix/hm-module.nix self;
      homeModules.default = import ./nix/hm-module.nix self;
    };
}

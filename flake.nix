{
  description = "0xc000022070's greeter - a ctOS-flavored ratatui frontend for greetd";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
  in {
    packages = forAllSystems (pkgs: let
      xgreeter = pkgs.callPackage ./nix/package.nix {};
    in {
      inherit xgreeter;
      default = xgreeter;
    });

    overlays.default = _: prev: {
      xgreeter = prev.callPackage ./nix/package.nix {};
    };

    nixosModules.default = import ./nix/nixos-module.nix self;
    nixosModules.xgreeter = self.nixosModules.default;

    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        packages = [pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer];
      };
    });

    formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
  };
}

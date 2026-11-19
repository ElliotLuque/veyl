{
  description = "Veyl: un shell para Hyprland sin barra, con islas que salen de los bordes de la pantalla";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    # Veyl está hecho sobre Quickshell 0.2.1; el de nixpkgs va por delante.
    # Con su propio nixpkgs: con el Qt de ahora no compila.
    quickshell.url = "git+https://git.outfoxxed.me/quickshell/quickshell?rev=41828c4180fb921df7992a5405f5ff05d2ac2fff";
  };

  outputs =
    {
      self,
      nixpkgs,
      quickshell,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      each = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      lib = {
        # Veyl hecho con los paquetes de quien lo instala (su `allowUnfree`,
        # sus overlays); `acceleration`, como en veyld/package.nix.
        package =
          pkgs:
          {
            acceleration ? null,
          }:
          let
            veyld = pkgs.callPackage ./veyld/package.nix { inherit acceleration; };
          in
          pkgs.callPackage ./nix/package.nix {
            inherit veyld;
            quickshell = quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;
          };
      };

      packages = each (pkgs: rec {
        veyl = self.lib.package pkgs { };
        veyld = veyl.veyld;
        default = veyl;
      });

      overlays.default = final: _: {
        veyl = self.lib.package final { };
      };

      homeManagerModules = {
        veyl = import ./nix/home-manager.nix self;
        default = self.homeManagerModules.veyl;
      };

      nixosModules = {
        veyl = import ./nix/nixos.nix self;
        default = self.nixosModules.veyl;
      };

      # Para compilar veyld y pasar tests/check.sh.
      devShells = each (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.veyld ];
          packages = with pkgs; [
            clippy
            rustfmt
            python3
            nodejs
            nixfmt
          ];
        };
      });

      formatter = each (pkgs: pkgs.nixfmt-tree);
    };
}

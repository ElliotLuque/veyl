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
        # sus overlays); `acceleration` y `cudaCapabilities`, como en veyld/package.nix.
        package =
          pkgs:
          {
            acceleration ? null,
            cudaCapabilities ? null,
          }:
          let
            veyld = pkgs.callPackage ./veyld/package.nix {
              inherit acceleration cudaCapabilities;
            };
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

      checks = each (
        pkgs:
        let
          inherit (pkgs) lib;
          hm = lib.evalModules {
            specialArgs = {
              inherit pkgs;
              osConfig = {
                programs.nh = {
                  enable = true;
                  flake = "/home/elliot/dotfiles/nixdots";
                };
              };
            };
            modules = [
              (
                { lib, ... }:
                {
                  options = {
                    home.packages = lib.mkOption { default = [ ]; };
                    xdg.configFile = lib.mkOption {
                      default = { };
                      type = lib.types.attrsOf lib.types.anything;
                    };
                    systemd.user.services = lib.mkOption {
                      default = { };
                      type = lib.types.attrsOf lib.types.anything;
                    };
                    programs.nh = {
                      enable = lib.mkOption { default = false; };
                      flake = lib.mkOption { default = null; };
                    };
                    wayland.windowManager.hyprland.systemd.enable = lib.mkOption { default = true; };
                  };
                }
              )
              self.homeManagerModules.veyl
              {
                _file = "/nix/store/aaaa-source/modules/desktop/veyl.nix, via option flake.modules.homeManager.veyl";
                programs.veyl = {
                  enable = true;
                  package = pkgs.hello;
                  systemd.enable = true;
                  settings.scheme = "living-code-night";
                  schemes.mio = {
                    bg = "#05080b";
                  };
                };
              }
            ];
          };
          hmManifest = builtins.fromJSON hm.config.xdg.configFile."veyl/nix.json".text;
          nixos = lib.evalModules {
            specialArgs = { inherit pkgs; };
            modules = [
              (
                { lib, ... }:
                {
                  options = {
                    environment.systemPackages = lib.mkOption { default = [ ]; };
                    environment.etc = lib.mkOption {
                      default = { };
                      type = lib.types.attrsOf lib.types.anything;
                    };
                    programs.nh = {
                      enable = lib.mkOption { default = true; };
                      flake = lib.mkOption { default = "/etc/nixos"; };
                    };
                  };
                }
              )
              self.nixosModules.veyl
              {
                _file = "/nix/store/aaaa-source/configuration.nix";
                programs.veyl = {
                  enable = true;
                  package = pkgs.hello;
                  settings.quiet = true;
                };
              }
            ];
          };
          nixosManifest = builtins.fromJSON nixos.config.environment.etc."veyl/nix.json".text;
        in
        {
          module-eval =
            assert hmManifest.flake == "/home/elliot/dotfiles/nixdots";
            assert hmManifest.rebuild == "nh os switch /home/elliot/dotfiles/nixdots";
            assert hmManifest.settings == { scheme = "living-code-night"; };
            assert hm.config.systemd.user.services.veyl.Unit.PartOf == [ "hyprland-session.target" ];
            assert hm.config.xdg.configFile ? "veyl/schemes/mio.json";
            assert nixosManifest.flake == "/etc/nixos";
            assert nixosManifest.rebuild == "nh os switch /etc/nixos";
            assert nixosManifest.settings == { quiet = true; };
            pkgs.runCommand "veyl-module-eval" { } "touch $out";
        }
      );

      formatter = each (pkgs: pkgs.nixfmt-tree);
    };
}

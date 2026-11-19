# programs.veyl para NixOS: lo mismo que el de Home Manager, para toda la
# máquina. Lo declarado va a /etc/veyl/nix.json y vale para cada usuario
# (si alguien lo tiene también en Home Manager, gana el suyo).
flake:

{
  config,
  lib,
  pkgs,
  options,
  ...
}:

let
  inherit (lib) mkOption mkEnableOption types;
  cfg = config.programs.veyl;
  common = import ./settings.nix { inherit lib pkgs; };
  rebuild =
    if config.programs.nh.enable or false then "nh os switch" else "sudo nixos-rebuild switch";
in
{
  options.programs.veyl = common.options // {
    enable = mkEnableOption "Veyl, el shell";

    package = mkOption {
      type = types.package;
      default = flake.lib.package pkgs { inherit (cfg) acceleration; };
      defaultText = lib.literalMD "el de este flake, con `acceleration`";
      description = "El paquete de Veyl.";
    };

    acceleration = mkOption {
      type = types.nullOr (
        types.enum [
          "cuda"
          "vulkan"
        ]
      );
      default = null;
      description = ''
        Dónde transcribe veyld la voz: sin nada, en la CPU; `cuda` (NVIDIA;
        necesita `allowUnfree`) o `vulkan`, en la GPU.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    environment.etc."veyl/nix.json".text = common.manifest {
      inherit cfg rebuild;
      options = options.programs.veyl;
    };
  };
}

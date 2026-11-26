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
  nhFlake = config.programs.nh.flake or null;
  rebuild =
    if config.programs.nh.enable or false then "nh os switch" else "sudo nixos-rebuild switch";
in
{
  options.programs.veyl = common.options // {
    enable = mkEnableOption "Veyl, el shell";

    package = mkOption {
      type = types.package;
      default = flake.lib.package pkgs { inherit (cfg) acceleration cudaCapabilities; };
      defaultText = lib.literalMD "el de este flake, con `acceleration` y `cudaCapabilities`";
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

    cudaCapabilities = mkOption {
      type = types.nullOr (types.listOf types.str);
      default = null;
      example = [ "8.9" ];
      description = ''
        Arquitecturas CUDA para compilar `veyld` cuando `acceleration = "cuda"`
        (por ejemplo `[ "8.9" ]` para una RTX 40). Sin fijar, usa las de
        `pkgs.cudaPackages` (mucho más lento de compilar).
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    environment.etc."veyl/nix.json".text = common.manifest {
      inherit cfg rebuild;
      defaultFlake = nhFlake;
      options = options.programs.veyl;
    };
  };
}

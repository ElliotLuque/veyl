# programs.veyl para Home Manager.
#
#   programs.veyl = {
#     enable = true;
#     settings = { scheme = "living-code-frost"; voice.speed = 1.3; };
#   };
#
# Instala `veyl` (con su Quickshell y su veyld), deja lo declarado en
# ~/.config/veyl/nix.json para que Veyl lo aplique y sepa dónde escribir lo
# que guardes desde los ajustes, y los esquemas propios en
# ~/.config/veyl/schemes. Arrancarlo es cosa del compositor (o de
# `systemd.enable`).
flake:

{
  config,
  lib,
  pkgs,
  options,
  osConfig ? null,
  ...
}:

let
  inherit (lib) mkOption mkEnableOption types;
  cfg = config.programs.veyl;
  common = import ./settings.nix { inherit lib pkgs; };
  nh = config.programs.nh.enable or false;
  nixos = osConfig != null;
  nhFlake =
    if (config.programs.nh.flake or null) != null then
      config.programs.nh.flake
    else if nixos then
      osConfig.programs.nh.flake or null
    else
      null;
  hyprlandSystemd = config.wayland.windowManager.hyprland.systemd.enable or false;
  rebuild =
    if nixos then
      (
        if nh || (osConfig.programs.nh.enable or false) then "nh os switch" else "sudo nixos-rebuild switch"
      )
    else if nh then
      "nh home switch"
    else
      "home-manager switch";
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

    schemes = mkOption {
      type = types.attrsOf (pkgs.formats.json { }).type;
      default = { };
      example = lib.literalExpression ''{ mio = { bg = "#05080b"; accent = "#00a692"; /* … */ }; }'';
      description = "Esquemas de color propios, uno por nombre (los roles en `schemes/`).";
    };

    systemd = {
      enable = mkEnableOption "arrancar Veyl como servicio de usuario con la sesión gráfica";
      target = mkOption {
        type = types.str;
        default = if hyprlandSystemd then "hyprland-session.target" else "graphical-session.target";
        defaultText = lib.literalMD "`hyprland-session.target` si `wayland.windowManager.hyprland.systemd.enable`, si no `graphical-session.target`";
        description = "Con qué objetivo de systemd arranca y se para.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile = {
      "veyl/nix.json".text = common.manifest {
        inherit cfg rebuild;
        defaultFlake = nhFlake;
        options = options.programs.veyl;
      };
    }
    // lib.mapAttrs' (
      name: scheme: lib.nameValuePair "veyl/schemes/${name}.json" { text = builtins.toJSON scheme; }
    ) cfg.schemes;

    systemd.user.services.veyl = lib.mkIf cfg.systemd.enable {
      Unit = {
        Description = "Veyl, el shell";
        PartOf = [ cfg.systemd.target ];
        After = [ cfg.systemd.target ];
      };
      Service = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
        RestartSec = 2;
      };
      Install.WantedBy = [ cfg.systemd.target ];
    };
  };
}

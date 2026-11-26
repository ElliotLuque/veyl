# Lo que se puede declarar de Veyl, igual para Home Manager y para NixOS:
# las mismas claves que settings.json (ver el README). Las conocidas llevan
# su tipo; cualquier otra pasa tal cual. Lo que no se declara queda en manos
# de los ajustes de Veyl.
#
# Lo declarado se aplica al reconstruir: Veyl lo ve llegar (aunque esté en
# marcha) y lo pone encima de lo que hubiera. Lo que cambies luego en los
# ajustes y no coincida con esto, los ajustes te lo enseñan para guardarlo
# aquí o deshacerlo.
{ lib, pkgs }:

let
  inherit (lib) mkOption types;
  json = (pkgs.formats.json { }).type;
  option =
    type: description:
    mkOption {
      type = types.nullOr type;
      default = null;
      inherit description;
    };
in
{
  options = {
    settings = mkOption {
      default = { };
      example = lib.literalExpression ''
        {
          scheme = "living-code-frost";
          wallpaper = "''${inputs.living-code}/night/switzerland.png";
          webSearch = "https://google.com/search?q=%s";
          brain.provider = "claude-code";
          voice = { speed = 1.3; volume = 0.4; };
          monitors."HDMI-A-1" = { mode = "3840x2160@60.00"; scale = 1.5; position = "0x0"; };
        }
      '';
      description = ''
        Los ajustes de Veyl, como en `settings.json`. Los que se dejan sin
        declarar se cambian solo desde Veyl.
      '';
      type = types.submodule {
        freeformType = json;
        options = {
          scheme = option types.str "El esquema de color (el nombre de un archivo de `schemes/` o de `~/.config/veyl/schemes/`).";
          wallpaper = option (types.either types.path types.str) "El fondo: una imagen, a pantalla completa; vacío, el color de fondo del esquema.";
          webSearch = option types.str "Dónde busca la entrada lo que no encuentra, con `%s`; vacío, sin búsqueda web.";
          terminal = option types.str "La terminal para las aplicaciones de terminal; vacío, `$TERMINAL` o kitty.";
          weekNumbers = option types.bool "El almanaque, con los números de semana.";
          quiet = option types.bool "No molestar.";
          journal = option types.bool "Si veyld lleva el diario de lo que miras.";
          llm = option types.str "La orden que contesta lo que la gramática no entiende (lo dicho, por la entrada estándar).";
          notificationDays = option types.ints.unsigned "Los días que se guardan las notificaciones; 0, siempre.";
          lockAfter = option types.ints.unsigned "Minutos sin tocar nada tras los que se bloquea; 0, nunca.";
          calendarTarget = option types.str "Dónde se apunta lo nuevo: el nombre de un calendario de Google; vacío, en Veyl.";
          eventWarning = option types.ints.unsigned "Minutos antes de un evento en que avisa la isla de la hora; 0, nunca.";
          voice = option json "La voz: `{ model, mic, speak, speed, volume, voz, engine, unload }`.";
          brain = option json "Quién contesta lo que la gramática no entiende: `{ provider, model, url }`.";
          input = option json "Teclado y ratón, para Hyprland: `{ kb_layout, kb_options, repeat_rate, repeat_delay, sensitivity, natural_scroll, touchpad }`.";
          monitors = option (types.attrsOf json) "Las pantallas, para Hyprland: `{ <salida> = { mode, scale, position }; }`.";
        };
      };
    };

    nix = {
      file = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "/home/tú/dotfiles/modules/veyl.nix";
        description = ''
          El archivo en el que Veyl escribe lo que guardas desde sus ajustes.
          Sin fijar, el primero en el que se declara `programs.veyl` (con un
          flake, el mismo camino dentro de `nix.flake`).
        '';
      };

      flake = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "/home/tú/dotfiles";
        description = ''
          La carpeta de tu flake. Sin fijar, se busca en `$NH_FLAKE`,
          `$FLAKE`, `/etc/nixos` y `~/.config/home-manager`.
        '';
      };

      rebuild = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "nh os switch";
        description = "Con qué se reconstruye tras guardar; sin fijar, la orden de siempre de NixOS o de Home Manager.";
      };
    };
  };

  # El manifiesto que lee Veyl (veyld/src/nix.rs).
  manifest =
    {
      cfg,
      options,
      rebuild,
      defaultFlake ? null,
    }:
    let
      settings = lib.filterAttrsRecursive (_: value: value != null) cfg.settings;
      files = lib.unique (
        map builtins.unsafeDiscardStringContext (options.settings.files ++ options.enable.files)
      );
      flake = if cfg.nix.flake != null then cfg.nix.flake else defaultFlake;
    in
    builtins.toJSON {
      inherit settings files flake;
      inherit (cfg.nix) file;
      rebuild =
        if cfg.nix.rebuild != null then
          cfg.nix.rebuild
        else
        # nh lleva el flake detrás; los demás, con --flake.
        if flake == null then
          rebuild
        else if lib.hasPrefix "nh " rebuild then
          "${rebuild} ${flake}"
        else
          "${rebuild} --flake ${flake}";
    };
}

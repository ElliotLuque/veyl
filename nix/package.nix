# Veyl, el shell: lo que lee Quickshell (los QML, los esquemas, las fuentes)
# en share/veyl y `veyl` en bin, que lo arranca con el Quickshell y el veyld
# de este paquete. Los programas de los que tira (el portapapeles, las
# capturas, las cuentas…) van detrás de los del sistema en el PATH.
{
  lib,
  stdenvNoCC,
  makeWrapper,
  python3,
  quickshell,
  veyld,
  jq,
  wl-clipboard,
  qalculate-gtk,
  grim,
  slurp,
  hyprpicker,
  fd,
  playerctl,
  brightnessctl,
  libnotify,
  xdg-utils,
  glib,
}:

let
  tools = [
    jq
    wl-clipboard
    qalculate-gtk
    grim
    slurp
    hyprpicker
    fd
    playerctl
    brightnessctl
    libnotify
    xdg-utils
    glib # gdbus
    python3
  ];
in
stdenvNoCC.mkDerivation {
  pname = "veyl";
  inherit (veyld) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../shell.qml
      ../Tokens.qml
      ../base
      ../dials
      ../state
      ../schemes
      ../fonts
      ../logos
      ../data/symbols.json
      ../data/cards.json
      ../bin
    ];
  };

  nativeBuildInputs = [ makeWrapper ];
  buildInputs = [ python3 ];

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/veyl $out/bin
    cp -r . $out/share/veyl
    patchShebangs $out/share/veyl/bin
    makeWrapper $out/share/veyl/bin/veyl $out/bin/veyl \
      --prefix PATH : ${
        lib.makeBinPath [
          quickshell
          veyld
        ]
      } \
      --suffix PATH : ${lib.makeBinPath tools}
    ln -s ${lib.getExe veyld} $out/bin/veyld
    runHook postInstall
  '';

  passthru = { inherit quickshell veyld; };

  meta = {
    description = "Un shell para Hyprland sin barra: islas que salen de los bordes de la pantalla";
    mainProgram = "veyl";
    platforms = lib.platforms.linux;
  };
}

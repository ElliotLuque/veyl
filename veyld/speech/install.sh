#!/bin/sh
# Instala la voz de Veyl con XTTS-v2 (veyl voice get xtts):
#
#   - en veyld/.tools/: Python 3.12, sox, la libstdc++ que piden las ruedas de
#     PyTorch y un entorno con PyTorch (CUDA si hay NVIDIA) y coqui-tts;
#   - en ~/.local/share/veyl/models/xtts-v2/: el modelo (~1,8 GB).
#
# El modelo es de Coqui y su licencia (CPML, https://coqui.ai/cpml) solo
# permite el uso no comercial: ejecutar esto es aceptarla.
set -eu
cd "$(dirname "$0")/.."
tools=$(pwd)/.tools
models=${XDG_DATA_HOME:-$HOME/.local/share}/veyl/models/xtts-v2
mkdir -p "$tools" "$models"

echo "La voz XTTS-v2 es de Coqui, con licencia CPML: solo uso no comercial (https://coqui.ai/cpml)."

[ -e "$tools/python" ] || nix build -o "$tools/python" nixpkgs#python312
[ -e "$tools/sox" ] || nix build -o "$tools/sox" nixpkgs#sox
[ -e "$tools/cc-lib-lib" ] || nix build -o "$tools/cc-lib" nixpkgs#stdenv.cc.cc.lib  # deja cc-lib-lib

if [ ! -x "$tools/xtts/bin/python" ]; then
    # Con NVIDIA, PyTorch para CUDA; si no, para la CPU (mucho más lenta).
    if [ -e /run/opengl-driver/lib/libcuda.so.1 ]; then index=https://download.pytorch.org/whl/cu128; else index=https://download.pytorch.org/whl/cpu; fi
    nix shell nixpkgs#uv -c sh -c "uv venv -q -p '$tools/python/bin/python3.12' '$tools/xtts' && uv pip install -q -p '$tools/xtts/bin/python' --index-url $index --extra-index-url https://pypi.org/simple torch torchaudio 'coqui-tts[codec]' six"
fi

for file in config.json model.pth vocab.json speakers_xtts.pth mel_stats.pth LICENSE.txt; do
    [ -s "$models/$file" ] && continue
    echo "Trayendo $file"
    curl -L --fail --progress-bar -o "$models/$file.part" "https://huggingface.co/tts-hub/XTTS-v2/resolve/main/$file"
    mv "$models/$file.part" "$models/$file"
done
echo "Listo: la voz de Veyl es XTTS-v2 (veyld speak hola, para oírla)."

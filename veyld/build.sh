#!/bin/sh
# Compila veyld en target/release (el que arranca Veyl si no hay uno en $PATH).
#
# Con una NVIDIA (está el controlador de CUDA), con la transcripción en la
# GPU: el kit de CUDA y el compilador que acepta se traen de nixpkgs a
# .cuda/ (que los ancla para que no los borre el recolector de Nix) y el
# binario los encuentra ahí, y al controlador en /run/opengl-driver/lib.
# `./build.sh cpu` compila sin GPU aunque la haya.
#
# Necesita cargo, rustc, cmake y libclang en el entorno (ver el README).
set -eu
cd "$(dirname "$0")"
here=$(pwd)

# La voz de Veyl la pone piper; se trae a .tools/ (veyld lo busca ahí si no está en $PATH).
command -v piper >/dev/null || [ -e .tools/piper ] || nix build -o .tools/piper nixpkgs#piper-tts

if [ "${1:-}" = cpu ] || [ ! -e /run/opengl-driver/lib/libcuda.so.1 ]; then
    exec cargo build --release --locked
fi

unfree='with import <nixpkgs> { config.allowUnfree = true; };'
[ -e .cuda/toolkit ] || nix build --impure -o .cuda/toolkit --expr "$unfree cudaPackages.cudatoolkit"
[ -e .cuda/cc ] || nix build --impure -o .cuda/cc --expr "$unfree cudaPackages.backendStdenv.cc"
# whisper-rs-sys enlaza con libculibos, que CUDA 13 ya no trae y que solo
# hace falta con cuBLAS estático: una vacía basta.
if [ ! -e .cuda/lib/libculibos.a ]; then
    mkdir -p .cuda/lib
    ar rcs .cuda/lib/libculibos.a
fi

# Solo la arquitectura de esta tarjeta («8.9» → 89): compila en minutos, no en horas.
arch=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>/dev/null | head -1 | tr -d '.' || true)

export PATH="$here/.cuda/toolkit/bin:$PATH"
export CUDA_PATH="$here/.cuda/toolkit"
export CMAKE_CUDA_COMPILER="$here/.cuda/toolkit/bin/nvcc"
export CMAKE_CUDA_HOST_COMPILER="$here/.cuda/cc/bin/g++"
export CMAKE_CUDA_ARCHITECTURES="${arch:-native}"
# El nvcc del kit de Nix no busca las cabeceras de cudart solo.
export NVCC_PREPEND_FLAGS="-I$here/.cuda/toolkit/include"
export CPATH="$here/.cuda/toolkit/include${CPATH:+:$CPATH}"
export RUSTFLAGS="-L $here/.cuda/toolkit/lib -L $here/.cuda/toolkit/lib/stubs -L $here/.cuda/lib -C link-arg=-Wl,-rpath,$here/.cuda/toolkit/lib:/run/opengl-driver/lib"
exec cargo build --release --locked --features cuda

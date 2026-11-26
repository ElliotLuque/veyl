# veyld como paquete de Nix: pkgs.callPackage ./veyld/package.nix { }
#
# La voz se transcribe en la CPU; para la GPU, `acceleration = "cuda"`
# (NVIDIA; necesita `allowUnfree`, y `cudaCapabilities = [ "8.9" ]` acorta
# mucho la compilación) o `acceleration = "vulkan"`.
{
  lib,
  rustPlatform,
  runCommandCC,
  stdenv,
  cmake,
  makeWrapper,
  piper-tts,
  pipewire,
  wireplumber,
  curl,
  playerctl,
  xdg-utils,
  sox,
  sqlite,
  acceleration ? null,
  cudaCapabilities ? null,
  cudaPackages ? null,
  autoAddDriverRunpath ? null,
  shaderc ? null,
  vulkan-headers ? null,
  vulkan-loader ? null,
}:

assert lib.elem acceleration [
  null
  "cuda"
  "vulkan"
];

let
  cuda = acceleration == "cuda";
  vulkan = acceleration == "vulkan";
  # whisper-rs-sys enlaza con libculibos, que CUDA 13 ya no trae y que solo
  # hace falta con cuBLAS estático: una vacía basta.
  culibos = runCommandCC "culibos-vacia" { } ''
    mkdir -p $out/lib
    ${stdenv.cc.targetPrefix}ar rcs $out/lib/libculibos.a
  '';
  runtimeTools = [
    piper-tts
    pipewire
    wireplumber
    curl
    playerctl
    xdg-utils
    sox
  ];
in
rustPlatform.buildRustPackage {
  pname = "veyld";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./grammar
      ./speech
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  buildFeatures = lib.optional cuda "cuda" ++ lib.optional vulkan "vulkan";
  # whisper.cpp se compila con CMake desde su build.rs, no aquí.
  dontUseCmakeConfigure = true;
  nativeBuildInputs = [
    cmake
    makeWrapper
    rustPlatform.bindgenHook
  ]
  ++ lib.optionals cuda [
    cudaPackages.cuda_nvcc
    autoAddDriverRunpath
  ]
  ++ lib.optional vulkan shaderc;
  # rusqlite trae su SQLite, pero el enlazador lo pide igual.
  buildInputs = [
    sqlite
  ]
  ++ lib.optionals cuda [
    cudaPackages.cuda_cudart
    (cudaPackages.cccl or cudaPackages.cuda_cccl)
    cudaPackages.libcublas
  ]
  ++ lib.optionals vulkan [
    vulkan-headers
    vulkan-loader
  ];
  env = lib.optionalAttrs cuda {
    CMAKE_CUDA_ARCHITECTURES =
      if cudaCapabilities != null then
        lib.concatMapStringsSep ";" (cap: builtins.replaceStrings [ "." ] [ "" ] cap) cudaCapabilities
      else
        cudaPackages.flags.cmakeCudaArchitecturesString;
    # libcuda.so es del controlador: para enlazar basta su sustituto.
    RUSTFLAGS = "-L ${lib.getLib cudaPackages.cuda_cudart}/lib/stubs -L ${culibos}/lib";
  };

  # Esta mira las aplicaciones instaladas (firefox, spotify): en el sandbox no hay.
  checkFlags = [ "--skip=daemon::tests::search_and_play" ];

  # Piper, utilidades de audio/red y los scripts de XTTS-v2 (sin depender del árbol de fuentes).
  postInstall = ''
    install -Dm644 speech/xtts.py $out/share/veyld/speech/xtts.py
    install -Dm755 speech/install.sh $out/share/veyld/speech/install.sh
    wrapProgram $out/bin/veyld \
      --suffix PATH : ${lib.makeBinPath runtimeTools} \
      --set-default VEYLD_SHARE "$out/share/veyld" \
      --set-default VEYLD_CC_LIB "${stdenv.cc.cc.lib}/lib"
  '';

  meta = {
    description = "El daemon de Veyl: el diario de lo que miras, los agentes en marcha, las órdenes que no necesitan un modelo y la voz";
    mainProgram = "veyld";
    platforms = lib.platforms.linux;
  };
}

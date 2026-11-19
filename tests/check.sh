#!/bin/sh
# Comprobaciones sin arrancar Veyl ni tocar el estado del usuario.
set -eu
cd "$(dirname "$0")/.."

cargo fmt --manifest-path veyld/Cargo.toml -- --check
cargo clippy --manifest-path veyld/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path veyld/Cargo.toml --locked
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v
node --test tests/state.test.cjs
sh -n bin/veyl
python3 - <<'PY'
from pathlib import Path
for name in ("bin/veyl-clip",):
    compile(Path(name).read_text(), name, "exec")
PY

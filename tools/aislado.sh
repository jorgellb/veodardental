#!/bin/sh
# Ejecuta una orden con HOME y las carpetas XDG en un directorio temporal,
# para que ni el programa ni sus pruebas lean o escriban los datos, ajustes,
# certificados o copias de este ordenador.
#   tools/aislado.sh cargo test
#   VEODARDENTAL_AISLADO=/ruta/fija tools/aislado.sh ./target/debug/veodardental --screenshot x.png
set -eu
R="${VEODARDENTAL_AISLADO:-$(mktemp -d "${TMPDIR:-/tmp}/veodardental-aislado.XXXXXX")}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$R/home"
export XDG_CONFIG_HOME="$R/config" XDG_DATA_HOME="$R/datos" XDG_CACHE_HOME="$R/cache" XDG_STATE_HOME="$R/estado"
# el programa también admite su propia raíz aislada (ver src/paths.rs)
export VEODARDENTAL_RAIZ="$R/veodardental"
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME" "$VEODARDENTAL_RAIZ"
exec "$@"

#!/bin/sh
# Mediciones de rendimiento con clínicas sintéticas (T-F6-07, T-INI-08,
# T-F6-08), siempre en carpetas aisladas: nunca toca los datos de este
# ordenador. Guarda el resultado con la versión del programa en
# docs/autonomia/mediciones/.
#   tools/medir.sh                      # clínica grande (8 000 pacientes), 3 clínicas, 8 puestos × 20
#   tools/medir.sh --escala 0.2 --clinicas 2 --puestos 4 --guardados 10
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT="docs/autonomia/mediciones/v${VERSION}.md"
tools/aislado.sh cargo build --locked
tools/aislado.sh ./target/debug/veodardental --medir --salida "$OUT" "$@"
echo "Guardado en $OUT (y ${OUT%.md}.json)"

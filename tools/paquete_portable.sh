#!/bin/sh
# Paquete portable de Linux para cualquier distribución (.tar.gz): el programa,
# su icono, el lanzador y los scripts para instalarlo y desinstalarlo.
#   tools/paquete_portable.sh [carpeta de salida]   (por defecto, dist)
# Usa el programa ya compilado en target/release (cargo build --release).
set -eu
cd "$(dirname "$0")/.."
out="${1:-dist}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
name="VeodarDental-$version-linux-$arch"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/$name/iconos" "$out"
cp target/release/veodardental "$tmp/$name/"
cp packaging/linux/veodardental.desktop packaging/linux/instalar.sh packaging/linux/desinstalar.sh "$tmp/$name/"
cp assets/iconos/icono-32.png assets/iconos/icono-128.png assets/iconos/icono-256.png assets/iconos/icono-512.png "$tmp/$name/iconos/"
cat > "$tmp/$name/LEEME.txt" <<TXT
VeodarDental $version para Linux ($arch), para cualquier distribución.

Instalar solo para este usuario:            ./instalar.sh
Instalar para todos (en /opt):              sudo ./instalar.sh --sistema
Desinstalar:                                ./desinstalar.sh   (o sudo ./desinstalar.sh --sistema)

Los datos de la clínica no se guardan aquí y no se borran al desinstalar.
TXT
tar -C "$tmp" -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"

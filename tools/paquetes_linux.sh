#!/bin/sh
# Crea los paquetes de Linux a partir del programa ya compilado con
# tools/compilar_linux.sh (el de target/release), todos con el mismo programa:
#   .deb          Debian, Ubuntu, Mint, Pop!_OS...
#   .rpm          Fedora, Red Hat, Rocky, Alma, openSUSE
#   .pkg.tar.zst  Arch, Manjaro, EndeavourOS
#   .AppImage     un solo fichero, sin instalar
#   .tar.gz       portable, para cualquier distribución (con instalar.sh)
#   tools/paquetes_linux.sh      → dist/
# Necesita cargo-packager, cargo-generate-rpm y Docker (el paquete de Arch se
# construye en un contenedor de Arch, con su propia herramienta).
set -eu
cd "$(dirname "$0")/.."
bin=target/release/veodardental
if [ ! -x "$bin" ]; then
  echo "Falta $bin: compílelo antes con tools/compilar_linux.sh" >&2
  exit 1
fi
mkdir -p dist

# .deb, AppImage y lo que necesita el paquete de Arch (PKGBUILD y su .tar.gz)
cargo packager --release --formats deb,appimage,pacman
# .rpm
cargo generate-rpm -o dist/
# portable
tools/paquete_portable.sh dist

# Arch: makepkg convierte el PKGBUILD en el paquete que instala pacman. Mismo
# nombre que en los demás sistemas, y el programa tal cual (sin quitarle los
# símbolos), como en los otros paquetes.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# una carpeta nueva cada vez (makepkg deja la suya de trabajo), que se borra al terminar
arch=$(mktemp -d "$PWD/target/arch.XXXXXX")
trap 'rm -rf "$arch"' EXIT
cp dist/PKGBUILD "dist/veodardental_${version}_x86_64.tar.gz" "$arch/"
sed -i -e 's/^pkgname=.*/pkgname=veodardental/' -e 's/^options=(!lto)$/options=(!lto !strip !debug)/' "$arch/PKGBUILD"
grep -q '^pkgname=veodardental$' "$arch/PKGBUILD"
grep -q '^options=(!lto !strip !debug)$' "$arch/PKGBUILD"
# Fijada por su huella, como la de tools/compilar_linux.sh: el paquete se publica.
arch_image=archlinux:latest@sha256:d4594bb2300cba11f41c2f27923eb56f63d9988909c402f9c95517d1e362c899
docker run --rm --security-opt label=disable -v "$arch:/pkg" -e DUENO="$(id -u):$(id -g)" "$arch_image" bash -euc '
  trap "chown -R \"\$DUENO\" /pkg" EXIT
  pacman -Sy --noconfirm --needed base-devel >/dev/null
  # makepkg no se ejecuta como administrador
  useradd -m constructor
  chown -R constructor /pkg
  su constructor -c "cd /pkg && makepkg --nodeps --noconfirm"
'
cp "$arch/veodardental-$version-1-x86_64.pkg.tar.zst" dist/
# lo intermedio de Arch no se reparte
rm -f dist/PKGBUILD "dist/veodardental_${version}_x86_64.tar.gz"
ls -l dist/

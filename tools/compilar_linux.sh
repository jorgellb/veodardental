#!/bin/sh
# Compila el programa de Linux con una glibc antigua (la 2.28, de AlmaLinux 8)
# para que el mismo programa funcione en las distribuciones de hace años y en
# las de ahora: Red Hat, Rocky y Alma 8 o posteriores, Debian 10+, Ubuntu
# 20.04+, Fedora, openSUSE Leap 15.4+, Arch... Compilado en el propio
# ordenador, en cambio, solo funciona desde su versión de glibc en adelante.
#   tools/compilar_linux.sh      → target/release/veodardental
# Necesita Docker. Rust y lo descargado se quedan en target/glibc-2.28 para la
# vez siguiente. Termina con error si el programa pide una glibc más nueva.
set -eu
cd "$(dirname "$0")/.."
# Fijada por su huella (como las acciones del flujo, por su commit): lo que
# sale de aquí se publica. Para actualizarla: docker pull almalinux:8.10 y
# docker inspect --format '{{index .RepoDigests 0}}' almalinux:8.10
image=almalinux:8.10@sha256:2d0f2eb2a83cc09dbc2eb92fade4dbe8f709628149e81aa00aec70630406603b
max=2.28
dir=target/glibc-2.28
mkdir -p "$dir" target/release
# El proyecto, de solo lectura: el contenedor solo escribe en su carpeta.
# label=disable: en sistemas con SELinux, sin cambiar las etiquetas del proyecto.
docker run --rm --security-opt label=disable -v "$PWD:/src:ro" -v "$PWD/$dir:/src/$dir" -w /src \
  -e CARGO_HOME="/src/$dir/cargo" -e RUSTUP_HOME="/src/$dir/rustup" -e CARGO_TARGET_DIR="/src/$dir/target" \
  -e DUENO="$(id -u):$(id -g)" -e DIR="/src/$dir" "$image" bash -euc '
    # lo creado dentro del contenedor vuelve a ser de quien lo lanzó, también si falla
    trap "chown -R \"\$DUENO\" \"\$DIR\"" EXIT
    dnf install -y -q gcc gcc-c++ make cmake perl >/dev/null
    if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
      curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain stable --no-modify-path
    fi
    "$CARGO_HOME/bin/rustup" update stable --no-self-update >/dev/null
    "$CARGO_HOME/bin/cargo" build --release --locked
  '
cp "$dir/target/release/veodardental" target/release/veodardental
need=$(objdump -T target/release/veodardental | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
top=$(printf '%s\n%s\n' "$max" "$need" | sort -V | tail -1)
if [ "$top" != "$max" ]; then
  echo "El programa pide glibc $need: más nueva que la $max prevista" >&2
  exit 1
fi
echo "target/release/veodardental: pide glibc $need como mucho (prevista: $max)"

#!/bin/sh
# Instala de verdad los paquetes de dist/ en contenedores limpios de cada
# distribución, arranca lo instalado (--comprobar, sin ventana), comprueba que
# el gestor de paquetes trajo las bibliotecas que abre la ventana y lo
# desinstala. Nada se instala en este ordenador.
#   tools/probar_paquetes_linux.sh                 → todas
#   tools/probar_paquetes_linux.sh rockylinux:8    → solo esa
# Necesita Docker y los paquetes de tools/paquetes_linux.sh.
set -eu
cd "$(dirname "$0")/.."
# Rocky 8 es la más antigua (glibc 2.28, la del programa). Solo versiones con
# repositorios vivos: Debian 11 ya no tiene soporte y sus réplicas fallan.
todas="rockylinux:8 ubuntu:20.04 ubuntu:24.04 debian:12 debian:13 fedora:latest opensuse/leap:15.6 archlinux:latest portable"
[ $# -gt 0 ] || set -- $todas
fallos=""

probar() {
  # $1 imagen, $2 órdenes de instalación, $3 de desinstalación
  echo "── $1"
  docker run --rm --security-opt label=disable -v "$PWD/dist:/dist:ro" "$1" sh -euc "
    $2
    test -x /usr/bin/veodardental
    test -f /usr/share/applications/veodardental.desktop
    test -f /usr/share/icons/hicolor/256x256/apps/veodardental.png
    # las bibliotecas que abre la ventana (teclado, gráficos, tipografías) las trae el paquete
    for lib in libxkbcommon.so.0 libEGL.so.1 libfontconfig.so.1; do
      ldconfig -p | grep -q \"\$lib\" || { echo \"falta \$lib\"; exit 1; }
    done
    mkdir -p /tmp/raiz
    VEODARDENTAL_RAIZ=/tmp/raiz veodardental --comprobar
    $3
    test ! -e /usr/bin/veodardental
    echo \"OK: $1\"
  "
}

for d in "$@"; do
  # sigue con las demás aunque una falle, y al final dice cuáles
  case "$d" in
    debian:* | ubuntu:*)
      probar "$d" "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /dist/*.deb >/dev/null" \
        "apt-get remove -y -qq veodardental >/dev/null" || fallos="$fallos $d" ;;
    rockylinux:* | almalinux:* | fedora:*)
      probar "$d" "dnf install -y -q /dist/*.rpm" "dnf remove -y -q veodardental" || fallos="$fallos $d" ;;
    opensuse/*)
      probar "$d" "zypper --non-interactive -q install --allow-unsigned-rpm /dist/*.rpm" "zypper --non-interactive -q remove veodardental" || fallos="$fallos $d" ;;
    archlinux:*)
      probar "$d" "pacman -Sy --noconfirm -q >/dev/null && pacman -U --noconfirm /dist/*.pkg.tar.zst >/dev/null" "pacman -R --noconfirm veodardental >/dev/null" || fallos="$fallos $d" ;;
    portable)
      # el .tar.gz con instalar.sh, como un usuario sin privilegios
      echo "── portable (debian:12, sin administrador)"
      docker run --rm --security-opt label=disable -v "$PWD/dist:/dist:ro" debian:12 sh -euc '
        useradd -m clinica
        su clinica -c "
          set -eu
          cd ~ && tar -xzf /dist/VeodarDental-*-linux-*.tar.gz && cd VeodarDental-*/
          ./instalar.sh
          test -x ~/.local/opt/VeodarDental/veodardental
          grep -q \"^Exec=/home/clinica/.local/opt/VeodarDental/veodardental\$\" ~/.local/share/applications/veodardental.desktop
          # los datos de la clínica (su carpeta de verdad) no se tocan al desinstalar
          mkdir -p ~/.local/share/VeodarDental && echo {} > ~/.local/share/VeodarDental/clinica.json
          mkdir -p ~/raiz
          VEODARDENTAL_RAIZ=~/raiz ~/.local/bin/veodardental --comprobar
          ./desinstalar.sh
          test ! -e ~/.local/opt/VeodarDental && test ! -e ~/.local/bin/veodardental && test ! -e ~/.local/share/applications/veodardental.desktop
          test -f ~/.local/share/VeodarDental/clinica.json
        "
        echo "OK: portable"
      ' || fallos="$fallos $d" ;;
    *)
      echo "No sé probar «$d»" >&2
      fallos="$fallos $d" ;;
  esac
done
if [ -n "$fallos" ]; then
  echo "FALLAN:$fallos" >&2
  exit 1
fi
echo "Todas bien: $*"

#!/bin/sh
# Quita el programa instalado con instalar.sh (con --sistema si se instaló así).
# Los datos de la clínica y sus copias NO se tocan: están en otra carpeta.
set -eu
if [ "${1:-}" = "--sistema" ]; then
  dest=/opt/VeodarDental
  bin=/usr/local/bin
  apps=/usr/share/applications
  icons=/usr/share/icons/hicolor
else
  # Fuera de ~/.local/share/VeodarDental, que es la carpeta de los datos de la clínica.
  dest="$HOME/.local/opt/VeodarDental"
  bin="$HOME/.local/bin"
  apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
fi
# Solo la carpeta del programa (y su versión anterior, si la hay), nunca otra cosa.
for d in "$dest" "$dest.anterior"; do
  if [ -f "$d/veodardental" ]; then
    rm -f "$d/veodardental"
    rmdir "$d" 2>/dev/null || true
  fi
done
rm -f "$bin/veodardental" "$apps/veodardental.desktop"
for size in 32 128 256 512; do rm -f "$icons/${size}x${size}/apps/veodardental.png"; done
echo "VeodarDental desinstalado. Los datos de la clínica siguen en su carpeta."

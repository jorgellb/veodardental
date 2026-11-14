#!/bin/sh
# Instala VeodarDental desde este paquete portable, para cualquier distribución
# de Linux. Sin privilegios, solo para este usuario:
#   ./instalar.sh
# Para todos los usuarios del ordenador (en /opt), con permisos de administrador:
#   sudo ./instalar.sh --sistema
# Se desinstala con desinstalar.sh, con la misma opción.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
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
# La carpeta del programa se llama VeodarDental: así el propio programa puede
# instalar las actualizaciones oficiales firmadas en ella.
mkdir -p "$dest" "$bin" "$apps"
cp "$here/veodardental" "$dest/veodardental"
chmod 755 "$dest/veodardental"
ln -sf "$dest/veodardental" "$bin/veodardental"
for size in 32 128 256 512; do
  mkdir -p "$icons/${size}x${size}/apps"
  cp "$here/iconos/icono-$size.png" "$icons/${size}x${size}/apps/veodardental.png"
done
sed "s|^Exec=.*|Exec=$dest/veodardental|" "$here/veodardental.desktop" > "$apps/veodardental.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" >/dev/null 2>&1 || true
echo "VeodarDental instalado en $dest. Ábralo desde el menú de aplicaciones o con «veodardental»."

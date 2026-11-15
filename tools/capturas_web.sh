#!/bin/sh
# Capturas de VeodarDental para su ficha en veodar.com (src/assets/veodardental de la web).
# Cada pantalla sale a 1600×1000, como Dirección y con los datos de ejemplo. Lo que tiene
# forma de teléfono, DNI, NIE, CIF o IBAN se difumina: los datos son inventados, pero
# podrían coincidir con los de alguien. El programa marca dónde lo dibuja con
# --zonas-privadas (src/captura.rs).
#
#   cargo build --release --locked
#   tools/capturas_web.sh ~/Documentos/Tines/veodar/src/assets/veodardental
#
# Necesita Python 3 con Pillow y numpy (numpy, para el TAC de ejemplo). Los nombres de los
# ficheros son los de la web (src/components/veodardental/shots.ts).
set -eu
DEST=${1:?Indique la carpeta de las capturas de la web}
cd "$(dirname "$0")/.."
BIN=./target/release/veodardental
[ -x "$BIN" ] || { echo "Falta $BIN: cargo build --release --locked" >&2; exit 1; }
TMP=$(mktemp -d "${TMPDIR:-/tmp}/capturas-web.XXXXXX")
python3 -I tools/cbct_ejemplo.py "$TMP/cbct-ejemplo.dcm" >/dev/null

shot() {
  name=$1
  shift
  timeout 90 tools/aislado.sh "$BIN" --screenshot "$TMP/$name.png" --size 1600x1000 --zonas-privadas "$@" >/dev/null 2>&1 || true
  [ -s "$TMP/$name.png" ] || { echo "No ha salido la captura $name" >&2; exit 1; }
  echo "$name"
}
dir() { shot "$@" --user dir; }

dir inicio --screen inicio
dir agenda --screen agenda --fecha 2026-11-16
dir pizarra --screen pizarra
dir recalls --screen recalls
dir listin --screen listin
shot entrada --login
dir ficha-resumen --screen pacientes --patient 101 --tab resumen
dir ficha-filiacion --screen pacientes --patient 101 --tab filiacion
dir ficha-salud --screen pacientes --patient 101 --tab salud
dir ficha-historial --screen pacientes --patient 101 --tab historial
dir ficha-documentos --screen pacientes --patient 101 --tab documentos
dir ficha-presupuestos --screen pacientes --patient 101 --tab presupuestos
dir ficha-cuenta --screen pacientes --patient 101 --tab cuenta
dir odontograma --screen pacientes --patient 101 --tab odontograma
dir odontograma-radiografia --screen pacientes --patient 101 --tab odontograma --radiografia
dir periodontograma --screen pacientes --patient 108 --tab periodontograma
dir ortodoncia --screen ortodoncia
dir laboratorio --screen laboratorio
dir equipos --screen imagenes --patient 101
dir ficha-imagenes --screen pacientes --patient 101 --tab imagenes --imagen "radiografia pe"
for v in axial:0.61 coronal:0.506 sagital:0.234; do
  dir "tac_${v%%:*}" --screen pacientes --patient 101 --tab imagenes --estudio "$TMP/cbct-ejemplo.dcm" --pieza 36 --vista "$v" --bajar 430
done
dir comparar-radiografias --screen pacientes --patient 100 --tab imagenes --imagen "radiografia pe" --comparar --bajar 520
dir ia-radiografia --screen pacientes --patient 101 --tab imagenes --imagen orto --bajar 900
dir presupuestos --screen presupuestos
dir cobros --screen cobros
dir facturas --screen facturas --verifactu
dir verifactu --screen facturas --fact-tab 2 --verifactu
dir caja --screen caja
dir gastos --screen gastos
dir bancos --screen bancos
dir comunicaciones --screen comunicaciones
dir almacen --screen almacen
dir tarifas --screen tarifas
dir colaboradores --screen colaboradores --horario
dir informes --screen informes --report 7
dir configuracion --screen configuracion
dir modulos --screen "mas modulos"

# Se difumina cada zona (con su entorno, para que no quede un borde nítido) y se guarda
# con el nombre de la web; de los tres cortes del TAC, el visor recortado.
python3 -I - "$TMP" "$DEST" <<'EOF'
import json, os, sys
from PIL import Image, ImageFilter
src, dest = sys.argv[1], sys.argv[2]
names = sorted(f[:-4] for f in os.listdir(src) if f.endswith('.png'))
for n in names:
    im = Image.open(f'{src}/{n}.png').convert('RGB')
    assert im.size == (1600, 1000), (n, im.size)
    for z in json.load(open(f'{src}/{n}.png.zonas.json')):
        b = (max(int(z[0]) - 2, 0), max(int(z[1]) - 2, 0), min(int(z[2]) + 3, 1600), min(int(z[3]) + 3, 1000))
        big = (max(b[0] - 12, 0), max(b[1] - 12, 0), min(b[2] + 12, 1600), min(b[3] + 12, 1000))
        blurred = im.crop(big).filter(ImageFilter.GaussianBlur(6))
        im.paste(blurred.crop((b[0] - big[0], b[1] - big[1], b[2] - big[0], b[3] - big[1])), b[:2])
    if n.startswith('tac_'):
        im.crop((885, 389, 1270, 705)).save(f'{dest}/tac-{n[4:]}.png', optimize=True)
        if n == 'tac_axial':
            im.save(f'{dest}/tac.png', optimize=True)
    else:
        im.save(f'{dest}/{n}.png', optimize=True)
print(len(names), 'capturas en', dest)
EOF
rm -r "$TMP"

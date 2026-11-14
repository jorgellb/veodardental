#!/bin/sh
# Aprovisiona el servidor de datos de VeodarDental en un Linux con systemd
# (un servidor propio o uno alquilado en la nube). Ver docs/despliegue_nube.md.
#
#   sudo sh aprovisionar.sh --binario ./veodardental [--puerto 8899] [--permitir 203.0.113.10]
#   sh aprovisionar.sh --ensayo /tmp/ensayo --binario ./veodardental   (no toca el sistema)
#
# Hace: usuario de servicio sin entrada, carpetas solo para él, el programa en
# /opt/veodardental, la unidad de systemd y, si se dice, el cortafuegos solo
# para las direcciones de la clínica. No arranca el servicio: antes hay que
# llevar los datos de la clínica (la guía lo explica).
set -eu

ENSAYO=""
BINARIO=""
PUERTO=8899
PERMITIR=""
DIR="$(cd "$(dirname "$0")" && pwd)"

while [ $# -gt 0 ]; do
    case "$1" in
        --ensayo) ENSAYO="$2"; shift 2 ;;
        --binario) BINARIO="$2"; shift 2 ;;
        --puerto) PUERTO="$2"; shift 2 ;;
        --permitir) PERMITIR="$PERMITIR $2"; shift 2 ;;
        *) echo "Opción desconocida: $1" >&2; exit 2 ;;
    esac
done

[ -n "$BINARIO" ] && [ -f "$BINARIO" ] || { echo "Falta el programa (--binario RUTA)." >&2; exit 2; }
case "$PUERTO" in ''|*[!0-9]*) echo "El puerto tiene que ser un número." >&2; exit 2 ;; esac

# en ensayo, todo debajo de esa carpeta y las órdenes del sistema solo se escriben
R="${ENSAYO}"
run() {
    if [ -n "$ENSAYO" ]; then
        echo "[ensayo] $*" >> "$ENSAYO/ordenes.txt"
    else
        "$@"
    fi
}

if [ -z "$ENSAYO" ] && [ "$(id -u)" -ne 0 ]; then
    echo "Hace falta ejecutarlo como administrador (sudo)." >&2
    exit 1
fi
[ -n "$ENSAYO" ] && mkdir -p "$ENSAYO" && : > "$ENSAYO/ordenes.txt"

# 1. usuario del servicio, sin entrada ni carpeta personal propia
if [ -n "$ENSAYO" ] || ! id veodardental >/dev/null 2>&1; then
    run useradd --system --home-dir /var/lib/veodardental --shell /usr/sbin/nologin --user-group veodardental
fi

# 2. su carpeta, solo para él. Como administrador solo se toca la carpeta base
# (en /var/lib, que es del sistema), nunca lo de dentro: es del usuario del
# servicio y podría tener enlaces. Las subcarpetas las crea el programa, y
# systemd (StateDirectory) la deja igual al arrancar.
BASE="$R/var/lib/veodardental"
if [ -L "$BASE" ]; then
    echo "$BASE es un enlace: no se toca. Revíselo a mano." >&2
    exit 1
fi
if [ -n "$ENSAYO" ]; then
    mkdir -p "$BASE"
    chmod 700 "$BASE"
    echo "[ensayo] install -d -m 700 -o veodardental -g veodardental /var/lib/veodardental" >> "$ENSAYO/ordenes.txt"
else
    install -d -m 700 -o veodardental -g veodardental "$BASE"
fi

# 3. el programa
mkdir -p "$R/opt/veodardental"
cp "$BINARIO" "$R/opt/veodardental/veodardental"
chmod 755 "$R/opt/veodardental/veodardental"

# 4. la unidad de systemd (con el puerto elegido)
mkdir -p "$R/etc/systemd/system"
if [ "$PUERTO" = 8899 ]; then
    cp "$DIR/veodardental-servidor.service" "$R/etc/systemd/system/veodardental-servidor.service"
else
    sed "s|^ExecStart=/opt/veodardental/veodardental --servidor\$|ExecStart=/opt/veodardental/veodardental --servidor --puerto $PUERTO|" "$DIR/veodardental-servidor.service" > "$R/etc/systemd/system/veodardental-servidor.service"
fi
chmod 644 "$R/etc/systemd/system/veodardental-servidor.service"
run systemctl daemon-reload
run systemctl enable veodardental-servidor.service

# 5. cortafuegos: el puerto solo para las direcciones de la clínica (si se dicen)
if [ -n "$PERMITIR" ]; then
    for ip in $PERMITIR; do
        run ufw allow from "$ip" to any port "$PUERTO" proto tcp
    done
    run ufw deny "$PUERTO"/tcp
else
    echo "Aviso: sin --permitir, el puerto $PUERTO queda como esté en el cortafuegos. Lo recomendado: solo la VPN o las direcciones de la clínica."
fi

echo "Listo. Lleve los datos de la clínica a /var/lib/veodardental (docs/despliegue_nube.md) y arranque con: systemctl start veodardental-servidor"

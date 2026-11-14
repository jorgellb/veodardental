# Servidor en la nube (opcional)

El servidor de datos puede estar en un ordenador de la clínica (lo normal, ver
`instalacion.md`) o en un servidor Linux alquilado. Es el mismo programa con
el mismo protocolo:
- TLS 1.3 con la identidad propia del servidor, que los puestos fijan;
- la clave de la clínica;
- usuario y contraseña, y el código del móvil si se activa.

Esta guía está ensayada en local (`tests/ensayo_nube.rs`). **No se ha
contratado ningún alojamiento ni se han subido datos**: el alojamiento real
está pendiente (B-18, T-F6-02X).

## 1. Antes de contratar: condiciones del alojamiento
Los datos de los pacientes son datos de salud. Lo que sigue es el estudio
hecho para elegir el alojamiento, citado de memoria y sin consultar las
fuentes en este trabajo. Lo confirma el DPO de la clínica
(`revision_externa/dpo.md`).

- **Contrato de encargado del tratamiento** (RGPD, artículo 28). El proveedor
  trata datos por cuenta de la clínica. El contrato dice:
  - para qué y con qué medidas de seguridad;
  - qué subencargados tiene;
  - cómo avisa de una brecha;
  - qué pasa con los datos al terminar (devolución y borrado, también de sus copias).
- **Dónde están los datos** (RGPD, artículos 44 a 49). Se elige una región
  dentro del Espacio Económico Europeo. Si el proveedor o sus subencargados
  están sujetos a leyes de fuera del EEE que les puedan obligar a dar acceso,
  hay que valorarlo (transferencias y garantías). Lo más sencillo es un
  proveedor europeo con centros de datos en la UE.
- **Seguridad** (RGPD, artículo 32): certificaciones del proveedor (por
  ejemplo, ISO/IEC 27001 y 27018) y disco cifrado por el proveedor.
  - Los datos de VeodarDental ya van cifrados en el disco con AES-256-GCM.
  - Su clave está en el mismo servidor (`datos_claves.json`), así que el cifrado
    no protege frente a quien administra la máquina. La protege el contrato.
- **Brechas** (RGPD, artículo 33): el proveedor avisa a la clínica sin
  dilación. La clínica notifica a la autoridad de control en 72 horas cuando proceda.
- **Copias fuera del servidor:** en otro proveedor o en la clínica. Si no, un
  problema del proveedor se lleva los datos y sus copias.
- **Acceso al servidor:**
  - lo administra solo quien diga la clínica, con clave SSH y sin contraseña;
  - el puerto del programa queda abierto solo a la VPN de la clínica o a sus
    direcciones (ver el apartado 5).

## 2. Aprovisionar el servidor
En un Linux con systemd (Debian, Ubuntu…), como administrador:

```sh
sudo sh deploy/nube/aprovisionar.sh --binario ./veodardental --puerto 8899 --permitir IP-DE-LA-CLINICA
```

Hace:
- usuario de servicio `veodardental`, sin entrada;
- su carpeta, `/var/lib/veodardental`, solo para él. La crea systemd (`StateDirectory`), y el script solo toca la carpeta base: si es un enlace, se para. Las subcarpetas las crea el programa;
- el programa en `/opt/veodardental`;
- la unidad de systemd (`deploy/nube/veodardental-servidor.service`), endurecida:
  - sin privilegios;
  - el sistema de solo lectura salvo sus datos;
  - `UMask=0077`;
  - la zona horaria de la clínica: `TZ=Europe/Madrid`, o `Atlantic/Canary` en Canarias. Las fechas de cobros, facturas e informes son las de la hora del servidor.
- el cortafuegos (`ufw`), solo con las direcciones dadas.

**No arranca el servicio:** antes hay que llevar los datos.

Para ver lo que haría sin tocar nada, se usa la carpeta de ensayo:
`sh deploy/nube/aprovisionar.sh --ensayo /tmp/ensayo --binario ./veodardental`
(las órdenes del sistema quedan en `ordenes.txt`).

## 3. Llevar los datos de la clínica
Sin datos de la clínica el servidor no arranca: así nunca sirve datos de
ejemplo por error.
1. En el servidor de la clínica: `veodardental --parar-servidor` (o cierre el programa).
2. Copie por SSH (`scp`), al usuario del servicio:
   - `…/datos/VeodarDental/clinica.json` (y el diario `clinica.cambios`, si lo hay) a `/var/lib/veodardental/datos/VeodarDental/`;
   - `…/ajustes/VeodarDental/datos_claves.json` (la clave de los datos) a `/var/lib/veodardental/ajustes/VeodarDental/`;
   - `…/ajustes/VeodarDental/servidor.json` (clave de la clínica e identidad) a la misma carpeta. Así los puestos siguen reconociendo al servidor: solo cambia la dirección.
3. Póngalos en su sitio con su dueño y solo para él, fichero a fichero (nunca `chown -R` sobre la carpeta del servicio):
   - `sudo -u veodardental mkdir -p /var/lib/veodardental/datos/VeodarDental /var/lib/veodardental/ajustes/VeodarDental`;
   - por cada fichero: `sudo install -o veodardental -g veodardental -m 600 clinica.json /var/lib/veodardental/datos/VeodarDental/` (y lo mismo con los demás, en su carpeta);
   - después, `sudo systemctl start veodardental-servidor`.
4. `journalctl -u veodardental-servidor` dice la dirección, la huella y la hora del servidor. La clave de la clínica no se escribe en el registro; se ve en el servidor con `sudo -u veodardental HOME=/var/lib/veodardental XDG_CONFIG_HOME=/var/lib/veodardental/ajustes /opt/veodardental/veodardental --clave-servidor`.
5. En cada puesto, Configuración › Trabajo en red: la dirección nueva. La huella tiene que ser la misma de antes.

## 4. Copias y recuperación
- El servidor hace su copia diaria en su carpeta `copias`.
- La segunda copia (Configuración › Copias de seguridad) va **fuera de ese servidor**: un volumen de otro proveedor, o una carpeta de la clínica que recoge las copias cada día. Va cifrada con su contraseña.
- La clave de recuperación se guarda aparte, como en la clínica.
- Restaurar: `restauracion.md`. El ensayo de restauración se hace igual desde un puesto (Copias de seguridad › Ensayo de restauración).

## 5. Acceso de los puestos
- **Lo recomendado:** una VPN (por ejemplo, WireGuard) entre la clínica y el servidor, con el puerto del programa abierto solo en la VPN.
- **Sin VPN:** el cortafuegos solo con las direcciones fijas de la clínica (`--permitir`).
- En los dos casos el protocolo protege igual: los puestos fijan la huella del servidor y se piden la clave de la clínica y el usuario. Los intentos con claves o contraseñas incorrectas se frenan.

## 6. Vigilancia y actualizaciones
- `VEODARDENTAL_CLAVE=… veodardental --estado-servidor DIRECCIÓN [HUELLA]` comprueba desde cualquier ordenador que el servidor responde, con su versión, la de los datos, su hora y su huella. Termina con código 0 o 1, para una alarma.
  - La clave de la clínica va en esa variable o por la entrada, nunca en la orden: se vería en la lista de procesos y en el historial.
- **Actualizar:** primero el servidor y después los puestos (D-22): `veodardental --actualizar paquete`, y luego `systemctl restart veodardental-servidor`.

## 7. Ensayado
`tests/ensayo_nube.rs`, con el programa de verdad como proceso aparte y sus carpetas aisladas:
- **`the_cloud_server_rehearsal_runs_locally`:**
  - sin datos no arranca;
  - como servicio no escribe la clave;
  - dice su zona horaria;
  - se comprueba desde fuera con la clave buena, y no con otra clave ni con otra huella;
  - se para y vuelve a arrancar con la misma identidad;
  - sus ficheros quedan solo para su usuario.
- **`the_provisioning_script_rehearses_without_touching_the_system`:**
  - el aprovisionamiento deja el programa, la unidad y las carpetas;
  - no arranca el servicio;
  - el programa arranca con el entorno de esa unidad.

La unidad pasa `systemd-analyze verify` (solo avisa de que el programa no está instalado en el ordenador del ensayo).

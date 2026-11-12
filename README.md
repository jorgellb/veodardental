# VeodarDental

Programa de gestión de clínicas dentales escrito en Rust (interfaz con egui/eframe).
Pacientes, agenda, odontograma y periodontograma, presupuestos, cobros y caja,
facturas (VeriFactu), mutuas y tarifas, recalls, almacén, laboratorio,
comunicaciones (WhatsApp, SMS, email), telefonía, trabajo en red entre puestos,
receta electrónica y más.

No incluye datos de ninguna clínica: al arrancar sin datos ofrece datos de
ejemplo ficticios.

## Nombre anterior

El programa se llamaba **DentaSuite**. La primera vez que se abre con el nombre
nuevo pasa sus carpetas (datos de la clínica y sus copias, imágenes, ajustes del
puesto, certificados, contraseña de las copias, WhatsApp y documentos) de
`DentaSuite` a `VeodarDental`, sin mezclar ni pisar nada si la nueva ya existe.
El instalador es de un programa nuevo: desinstale antes el de DentaSuite.

## Código fuente

Este directorio es todo el código del programa y no depende de ningún fichero de
fuera de él. No lleva código ni datos de GESDEN: con GESDEN solo se conecta, en
solo lectura, al importar desde su base de datos de SQL Server. Lo que se genera
al compilar (`target/`, `dist/`) y las capturas (`shots/`) no son código.

## Compilar

Necesita Rust estable (1.89 o posterior).

```sh
cargo build --release      # programa en target/release/veodardental
cargo test                 # pruebas automáticas
```

En Linux hacen falta las bibliotecas de desarrollo habituales de una
aplicación gráfica (X11/Wayland y OpenGL).

## Modos del programa

| Orden | Para qué |
|---|---|
| `veodardental` | El programa normal |
| `veodardental --servidor` | Servidor de datos de la clínica (el resto de puestos se conectan a él) |
| `veodardental --repositorio-recetas` | Repositorio de recetas electrónicas para las farmacias (SREP) |
| `veodardental --screenshot captura.png` | Arranca, dibuja y guarda una captura (pruebas) |

## Trabajo en red y seguridad

Un ordenador hace de servidor (`--servidor` o Configuración › Puestos) y los
demás se conectan con su dirección y la clave de la clínica.

- **Todo va cifrado (TLS 1.3).** El servidor tiene su propia identidad, creada
  al azar, que no sale de él. La primera vez, antes de mandarle nada, el
  puesto le pide que demuestre que sabe la clave de la clínica (firmando un
  reto, sin que la clave viaje); entonces apunta su huella y después no acepta
  otro servidor. Así nadie en la red puede leer los datos ni hacerse pasar por
  el servidor (tampoco, con el puesto ya dado de alta, quien sepa la clave). La huella se ve en el servidor (Configuración › Puestos y al
  arrancar `--servidor`); si cambia, el puesto la enseña para compararla.
- **Sin entrar no hay datos.** Antes de entrar, el puesto solo sabe el nombre
  de la clínica y sus usuarios. Al entrar (contraseña y, si lo tiene, código
  del móvil) el servidor comprueba el acceso y da al puesto una sesión.
- **Las contraseñas no salen del servidor.** Los puestos no reciben las
  huellas de las contraseñas ni las claves del código del móvil; los fallos de
  acceso y los bloqueos los lleva el servidor. Solo Dirección puede cambiar
  los usuarios y activar o quitar el código del móvil.
- **Cada perfil recibe lo que necesita.** Recepción no recibe los datos
  clínicos que no usa (periodontogramas, imágenes y análisis de IA, recetas,
  ortodoncia, las respuestas de los cuestionarios de salud, antecedentes y
  notas clínicas) y no ve sus pantallas; de los cuestionarios solo sabe si los
  hay, de cuándo son y si están firmados (para pedirlos). Sí recibe los
  tratamientos con precio, las alertas y alergias y los documentos. Lo que no
  ve no lo puede borrar ni cambiar. Doctores, higienistas y Dirección lo
  reciben todo.
- **Los registros solo crecen**: ningún puesto puede borrar entradas del
  registro de accesos ni de la auditoría.
- Contraseñas con PBKDF2-HMAC-SHA256 (600 000 vueltas) y sal aleatoria; las
  nuevas, de 6 caracteres o más. Las de versiones anteriores siguen valiendo y
  se pasan al formato nuevo al entrar.

- **Solo viaja lo que cambia.** Cada puesto envía solo los registros que ha
  cambiado (si no ha cambiado nada, no envía nada) y recoge solo lo que han
  cambiado los demás; el servidor combina los cambios registro a registro sin
  guardar copias antiguas de los datos. Con una clínica grande (unos 220 MB de
  datos), un guardado normal envía alrededor de 1 KB.
- **Lo que no se puede hacer dos veces lo hace el servidor**, una operación
  tras otra: emitir, rectificar y anular facturas (números correlativos y
  cadena VeriFactu), generar remesas SEPA y registrar su cobro o sus recibos
  devueltos, liquidar a las mutuas, conciliar el banco y confirmar o rechazar
  las citas pedidas por internet. Si dos puestos lo hacen a la vez, el segundo
  recibe un aviso y ve lo que ha hecho el primero. Los cobros que llegan a una
  caja que otro puesto acaba de cerrar pasan a la caja abierta, para que el
  arqueo hecho siga cuadrando.

**Al actualizar, actualice el servidor y todos los puestos a la vez**: un
puesto con una versión anterior no puede conectar con el servidor nuevo (ni al
revés), y el programa lo dice con un aviso.

En un solo ordenador, al guardar solo se apunta lo que ha cambiado (en el
diario `clinica.cambios`, junto a los datos) y el fichero entero se reescribe
de vez en cuando y al cerrar; si se va la luz, al abrir se recupera lo
apuntado. Solo una ventana de VeodarDental puede guardar en los datos a la
vez: si se abre otra, avisa y no guarda.

Con carpeta compartida (sin servidor), cada puesto lee el fichero entero,
contraseñas incluidas: para varios puestos se recomienda el servidor.

La puesta en marcha comprueba si el disco está cifrado (BitLocker, FileVault o
LUKS): los datos de salud, las copias y las claves de los servicios (correo,
WhatsApp, IA) están en él, y es lo único que los protege si roban el ordenador.

## Importar desde otros programas

Configuración › Importar datos:

- **Microsoft SQL Server**: conexión de solo lectura (TDS 7.3, SQL Server 2008 o
  posterior; instancias con nombre `SERVIDOR\INSTANCIA`; usuario de SQL Server, o
  de Windows en el programa para Windows). GESDEN y Novaclinic (Infomed) se
  reconocen por sus tablas y se importan catálogo, tarifas y pacientes; de
  cualquier otro programa se elige una tabla o una consulta SELECT y se asignan
  las columnas.
- **Excel / CSV** (xlsx, xls, ods, csv) con el mismo asistente de columnas.

## Receta electrónica privada (conexión directa)

`src/srep.rs` implementa los servicios del repositorio de recetas del «Diseño
técnico e-receta privada – Servicios Repositorios v2.04.1» de los Consejos
Generales; `src/firma.rs` la firma CAdES con el certificado .p12/.pfx del
prescriptor; `src/colegiado.rs` la comprobación del colegiado; `src/srep_doc.rs`
la hoja de información al paciente con DataMatrix. Para usarlo con farmacias
reales hace falta la certificación del sistema y del repositorio por el CGCOM y
el Consejo de Dentistas, y publicar el repositorio tras un proxy TLS con
verificación del certificado de Nodofarma (Configuración genera la
configuración de nginx).

## Instaladores

Se hacen con [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)
(configuración en `Cargo.toml`, `[package.metadata.packager]`):

```sh
cargo install cargo-packager --locked
cargo packager --release                 # los formatos del sistema actual, en dist/
cargo packager --release --formats deb   # solo uno
```

| Sistema | Instaladores |
|---|---|
| Windows | `.exe` (NSIS, sin permisos de administrador) y `.msi` (WiX) |
| macOS | `.app` y `.dmg` (Apple Silicon e Intel) |
| Linux | `.deb` y AppImage |

Cada instalador se crea en su propio sistema. El flujo
`.github/workflows/instaladores.yml` lo hace solo en GitHub: pasa las pruebas y
crea los de Windows, macOS (los dos procesadores) y Linux; al subir una
etiqueta `v…` los publica en la versión. Sin firma digital, Windows
(SmartScreen) y macOS (Gatekeeper) avisarán al abrirlos la primera vez.

El icono se genera con `veodardental --icono assets/iconos`.

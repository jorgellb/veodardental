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

Necesita Rust estable (1.88 o posterior).

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

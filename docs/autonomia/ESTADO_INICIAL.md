# Estado inicial

Comprobado el 13 de noviembre de 2026 al empezar el encargo de
`VEODARDENTAL_GOAL.md` (SHA-256 `dceb8c7c55273f99eb7d7e17c629ff2867067891abe386a577c16157f937a38d`).

## Procedencia de las cifras

| Cifra | Fuente | Valor |
|---|---|---|
| Último commit | Informe del propietario (histórico) | `ade418b` |
| Último commit | `git rev-parse HEAD`, medido | `ade418bcd4cb576bdb7304eccfd8fe7e50d536ba`: coincide |
| Líneas de Rust en `src/` | Informe (histórico) | unas 54.600 |
| Líneas de Rust en `src/` | `find src -name '*.rs' \| xargs cat \| wc -l`, medido | 54.569 |
| Pruebas | Informe (histórico) | 302 y 10 manuales |
| Pruebas | `cargo test --workspace --all-targets --locked`, medido (E-001) | 302 pasan, 0 fallan, 10 ignoradas |
| Avisos de clippy | Informe (histórico) | 0 |
| Avisos de clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings`, medido (E-001) | 0, código de salida 0 |
| Mediciones de rendimiento (220 MB, 20.000 pacientes) | Informe (histórico) | guardado local de 0,02 a 0,2 s, cambio de unos 1 KB, entrada comprimida de unos 8 MB |
| Mediciones de rendimiento | Pendiente de reproducir con datos sintéticos (T-INI-08) | — |

Las cifras del informe son referencias históricas. Solo las marcadas como
«medido» son de esta ejecución.

## Git

- Rama inicial: `main`, HEAD `ade418b`. No hay más ramas ni `stash`.
- Ficheros sin seguimiento al empezar:
  - `VEODARDENTAL_GOAL.md`: el encargo, del propietario. Se conserva sin cambios (D-04).
  - `PLAN_DE_DESARROLLO.md`: documento del asistente de la sesión anterior, que lo escribió a petición del propietario.
- El trabajo de este encargo va en la rama local `autonomia/fases-0-6`. No se hace push ni fusión.

## Herramientas y controles

- Rust 1.99.1, cargo 1.99.1, clippy 0.1.99 y rustfmt 1.10.0.
- No hay `CLAUDE.md` en el repositorio. La instrucción global del usuario solo trata de la herramienta graphify y no afecta a este trabajo.
- CI (`.github/workflows/instaladores.yml`): se lanza a mano o con una etiqueta `v*`.
  - Ejecuta `cargo test` en Ubuntu 22.04.
  - Después, `cargo packager --release` en Windows, macOS (Apple Silicon e Intel) y Linux.
  - No comprueba formato ni clippy.
- Órdenes oficiales del repositorio:
  - `cargo test`, que es la del CI;
  - `cargo build --release`;
  - `cargo packager --release`, que hace los instaladores.
- `cargo fmt --check` fallaba con 397 bloques (3266 líneas de diferencia) porque no había `rustfmt.toml`. Ver D-03.

## Componentes

| Componente | Ficheros |
|---|---|
| Interfaz (egui/eframe) | `src/app.rs`, `src/screens/*.rs`, `src/theme.rs`, `src/icons.rs` |
| Modelo y reglas | `src/model.rs`, `src/finance.rs`, `src/extra.rs`, `src/agenda_extra.rs`, `src/stock.rs`, `src/horario.rs`, `src/insurer.rs`, `src/bank.rs` |
| Persistencia de un solo ordenador | `src/local.rs` (diario `clinica.cambios`), JSON en `Db::path()` |
| Carpeta compartida | `src/shared.rs` (combinación a tres bandas con el fichero bloqueado) |
| Servidor de datos y red | `src/server.rs` (TLS 1.3, sesiones, operaciones del servidor), `src/sync.rs` (cambios por registro), `src/vista.rs` (filtro por perfil), `src/users.rs`, `src/totp.rs` |
| Tareas automáticas | `src/comms.rs`, `src/mail.rs`, `src/whatsapp.rs`, `src/tasks.rs`, copias en `src/tools.rs`, VERI*FACTU en `src/verifactu.rs` |
| Importadores | `src/mssql.rs` (SQL Server, GESDEN, Novaclinic), `src/gesden.rs` (catálogo), `src/extra.rs` (Excel/CSV) |
| Portales web | `src/booking.rs` y `booking_page.html` (cita online), `src/budget_link.rs`, `src/doc_link.rs`, `src/rx_link.rs`, `src/portal.rs`, `src/slot_offer.rs`, `src/tunnel.rs` (Cloudflare) |
| Receta electrónica | `src/srep.rs`, `src/srep_doc.rs`, `src/srep_cert.rs`, `src/firma.rs`, `src/colegiado.rs`, `src/erx.rs`, `src/nomenclator.rs` |
| Cifrado y disco | `src/cifrado.rs` (copias cifradas), `src/disco.rs` (BitLocker, FileVault, LUKS) |
| Empaquetado | `Cargo.toml` `[package.metadata.packager]`, `assets/iconos` |

## Rutas y servicios

- Las rutas de datos, ajustes y documentos salían de `dirs::` en unos 25 sitios.
- En las pruebas solo se redirigía el fichero de datos, así que las pruebas leían el `puesto.json` real y las capturas listaban las copias reales.
- Corregido antes de seguir (T-INI-02, E-002).
- Los servicios externos (AEAT, Meta, SMTP, proveedores de IA, Cloudflare, Nodofarma y SQL Server) solo se llaman con una acción expresa y su configuración. Ninguna prueba no ignorada los contacta.

## Pruebas ignoradas

| Prueba | Por qué no se lanza | Requiere |
|---|---|---|
| `nomenclator::tests::real_official_list` | Lista oficial de la AEMPS | Fichero descargado (`DS_NOMENCLATOR`) |
| `ia::tests::openrouter_live_model_list` | Servicio en internet | Red hacia OpenRouter |
| `mssql::tests::real_server` | SQL Server de verdad | `DS_MSSQL` y un servidor |
| `verifactu::tests::dump_batch_for_schema_validation` | Volcado para xmllint | `VF_DUMP` y el XSD de la AEAT |
| `shared::tests::big_clinic_save_costs` | Medición larga (unos 220 MB) | Tiempo y memoria |
| `shared::tests::big_clinic_local_save` | Medición larga | Tiempo y memoria |
| `server::tests::big_clinic_login_costs` | Medición larga | Tiempo y memoria |
| `extra::tests::print_headers` | Lee un fichero real | `DS_PROBE` |
| `booking::tests::serve_demo_page` | Sirve una página durante minutos | Puerto local y tiempo |
| `ui_tests::login_with_saved_data` | Arranque con datos en disco | Carpeta de datos aislada |

## Funciones que el informe da por existentes

Comprobación pendiente de detallar en T-INI-06. Los módulos de la tabla de
componentes existen y tienen pruebas.

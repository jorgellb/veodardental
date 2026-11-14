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
| Mediciones de rendimiento | `tools/medir.sh`, clínica sintética de 8.000 pacientes y 226,5 MB, medido el 14/11/2026 (T-INI-08, `docs/autonomia/mediciones/v0.1.0.md`) | guardado local de un cambio de 0,021 a 0,147 s; cambio enviado de 4,5 KB (una cita y un cobro, contando cómo estaban y cómo quedan); entrada comprimida de recepción de 13,0 MB |

Las cifras del informe son referencias históricas. Solo las marcadas como
«medido» son de esta ejecución.

## Git

- Rama inicial: `main`, HEAD `ade418b`. No hay más ramas ni `stash`.
- Nota posterior (D-31): al quitar la línea de coautores de los mensajes, `ade418b` pasó a ser `69e33dc`. El contenido es el mismo.
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

Comprobado en el código (T-INI-06). Cada función que cita el informe, dónde
está y una prueba que la cubre. Las pruebas siguen en la batería.

| Función | Dónde | Prueba |
|---|---|---|
| Pacientes y responsables | `src/model.rs` (`Patient`, `Payer`), `src/screens/pacientes.rs`; fusión en `src/bank.rs` | `stock::tests::a_childs_treatment_is_invoiced_to_the_parent`, `bank::tests::merging_moves_everything_of_the_removed_patient` |
| Agenda | `src/model.rs` (`Appointment`), `src/agenda_extra.rs`, `src/horario.rs`, `src/screens/agenda.rs` | `agenda_extra::tests::overlapping_appointments_go_side_by_side` |
| Odontograma | `src/model.rs` (`MedEntry`, `OdgKind`), `src/screens/odontograma.rs` | `stock::tests::marking_a_planned_treatment_done_on_the_chart_completes_it`, `ui_tests::the_chart_completes_planned_treatments_and_can_undo_them` |
| Periodontograma | `src/screens/perio.rs`, dictado en `src/perio_voz.rs` | `perio_voz::tests::a_clean_dictation_is_understood` |
| Imágenes | `src/screens/imagenes.rs`, `src/dicom.rs`, `src/stl.rs`, `src/screens/visor3d.rs` | `dicom::tests::synthetic_images_are_read_with_scale_window_and_orientation`, `ui_tests::dicom_stl_and_comparison_are_shown_in_the_viewer` |
| Ortodoncia | `src/extra.rs`, `src/screens/ortodoncia.rs` | `extra::tests::ortho_phases_are_kept_and_a_finished_case_stops_its_controls` |
| Recetas | `src/erx.rs`, `src/srep.rs`, `src/srep_doc.rs`, `src/screens/recetas.rs` | `erx::tests::direct_prescription_signed_and_dispensed` |
| Presupuestos | `src/model.rs` (`Budget`), `src/plan.rs`, `src/screens/presupuestos.rs` | `extra::tests::a_plan_with_paid_or_remitted_installments_cannot_change` |
| Cobros | `src/model.rs` (`add_payment`), `src/finance.rs`, `src/screens/cobros.rs` | `finance::tests::a_wrong_payment_is_cancelled_and_leaves_the_cash_balanced` |
| Caja | `src/model.rs` (`CashSession`), `src/screens/caja.rs` | `server::tests::a_payment_made_while_another_station_closes_the_cash_leaves_it_balanced` |
| Facturas y VERI*FACTU | `src/model.rs` (`issue_invoice`), `src/verifactu.rs`, `src/screens/facturas.rs` | `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain`, `verifactu::tests::the_exemption_article_stays_with_each_invoice` |
| SEPA | `src/extra.rs` (remesas), `src/screens/remesas.rs` | `extra::tests::sepa_remittance_flow` |
| Mutuas | `src/insurer.rs`, `src/screens/mutuas.rs` | `insurer::tests::claim_collects_insured_acts_once` |
| Bancos | `src/bank.rs`, `src/screens/bancos.rs` | `bank::tests::norma43_with_several_accounts_and_control_totals` |
| Almacén | `src/stock.rs`, `src/screens/almacen.rs` | `stock::tests::realizing_treatment_consumes_stock`, `server::tests::material_spent_by_two_stations_at_once_is_all_taken_from_stock` |
| Laboratorio | `src/stock.rs` (`LabWork`), `src/lab_order.rs`, `src/screens/laboratorio.rs` | `stock::tests::lab_cost_written_after_receiving_goes_to_expenses` |
| Informes | `src/informes.rs`, `src/screens/informes.rs` | `informes::tests::collected_matches_the_operations`, `informes::tests::production_matches_the_operations` |
| Comunicaciones | `src/comms.rs`, `src/mail.rs`, `src/whatsapp.rs`, `src/sms.rs`, `src/screens/comunicaciones.rs` | `comms::tests::reminders_generated_twice_are_left_once` |
| Portales web | `src/booking.rs`, `src/portal.rs`, `src/budget_link.rs`, `src/doc_link.rs`, `src/rx_link.rs`, `src/tunnel.rs` | `portal::tests::tokens_are_random_and_checked` |
| Usuarios y doble factor | `src/users.rs`, `src/totp.rs`, `src/screens/login.rs` | `users::tests::lockout_after_failures_and_pin_change`, `users::tests::second_factor_with_the_phone_code` |
| Servidor TLS | `src/server.rs` (TLS 1.3 con identidad propia, prueba de la clave y huella fijada) | `server::tests::stations_only_accept_the_server_they_know`, `server::tests::nothing_secret_goes_to_an_unproven_server` |
| Copias | `src/tools.rs`, `src/cifrado.rs` | `tools::tests::backup_roundtrip_and_prune` |
| Importadores | `src/mssql.rs`, `src/migracion.rs`, `src/gesden.rs`, `src/extra.rs` | `migracion::tests::a_full_migration_with_its_reconciliation`, `mssql::tests::catalog_from_gesden_rows` |

No falta ninguna: todas estaban antes del encargo y no se han construido otra
vez. Las ampliaciones de este encargo están en `PROGRESO.md`.

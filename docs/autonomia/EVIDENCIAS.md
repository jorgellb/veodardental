# Evidencias

Órdenes ejecutadas con `tools/aislado.sh` (HOME, XDG y `VEODARDENTAL_RAIZ` en un
directorio temporal), salvo indicación.

## E-001 Batería inicial (HEAD `ade418b`, sin cambios)
| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 1 | 397 bloques con diferencias: el código no seguía rustfmt y no había configuración |
| `cargo test --workspace --all-targets --locked` | 0 | 302 pasan, 0 fallan, 10 ignoradas, en 3,06 s |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | sin avisos |
| `cargo build --workspace --release --locked` | 0 | 1 min 04 s; `target/release/veodardental` de 59,3 MB |

Las 10 pruebas ignoradas están clasificadas en ESTADO_INICIAL.md. Necesitan
servicios, ficheros reales o mediciones largas.

## E-002 Aislamiento de rutas (commit `3e48461`)
- **Prueba nueva:** `paths::tests::tests_never_use_the_real_folders`. Ninguna ruta de las pruebas (datos, ajustes, documentos y `Db::path()`) cae fuera de la carpeta temporal ni dentro de las carpetas del programa de este ordenador.
- **Prueba nueva:** `paths::tests::an_isolated_root_holds_every_folder`.
- **Cambio:** 24 usos de `dirs::` pasan por `crate::paths`, con 0 usos restantes fuera de `src/paths.rs` según `grep -rn "dirs::" src`.
- **Resultado:** `cargo test --locked` pasa con 304 pruebas, 0 fallos y 10 ignoradas; `cargo clippy --all-targets --locked -- -D warnings` da código 0.

## E-003 Formato (commit `aca725a`)
- `rustfmt.toml` con `max_width = 250` y `use_small_heuristics = "Max"`.
- `cargo fmt --all -- --check` da código 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` da código 0.
- `cargo test --workspace --all-targets --locked`: 304 pasan, 0 fallan, 10 ignoradas.

## E-004 Seguimiento, verificador y encargo (commits `50431a3` y `6f8d19c`)
- `VEODARDENTAL_GOAL.md` guardado sin cambios (SHA-256 `dceb8c7c…a38d`).
- `cargo run --example verificar_objetivo -- --progreso`: 134 líneas del encargo y 134 requisitos, todos con tareas. «RESULTADO: inventario íntegro (modo progreso)».
- `BLOQUEOS.md` registra los requisitos externos de la fase 0: GESDEN (B-03), receta certificada (B-09), AEAT (B-06), firma de instaladores (B-25) y WhatsApp (B-07), además de los demás.

## E-005 Exención de IVA configurable (T-F0-02 a T-F0-04)
- **Usos localizados:**
  - leyenda de la factura imprimible (`src/screens/centros.rs`), pantalla de la factura y exportación en texto (`src/screens/facturas.rs`), comentario de VERI*FACTU;
  - textos genéricos sin artículo en el presupuesto web.
- **Fuente oficial:** BOE consolidado, consultado el 13/11/2026 (docs/normativa.md).
- **Prueba nueva:** `verifactu::tests::the_exemption_article_stays_with_each_invoice`.
  - Antes de implementarse no compilaba: la función no existía.
  - Comprueba que la factura antigua imprime exactamente la leyenda de antes y que el valor por defecto no cambia nada.
  - Con el 20.Uno.5º, solo las nuevas cambian; la rectificativa cita la de su original; la cadena sigue íntegra; un fichero de configuración antiguo carga el 20.Uno.3º.
- **Prueba de pantalla nueva:** `ui_tests::the_vat_exemption_for_new_invoices_is_chosen_in_the_settings`, con una ventana de 1280×800. Elegir el 5.º cambia la configuración, deja registro y no toca las facturas emitidas.
- **Captura sintética:** `docs/autonomia/capturas/f0_exencion_iva_1280x800.png`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 306 pasan, 0 fallan, 10 ignoradas |

## E-006 Área 1: documentos, consentimientos y cuestionarios
- Revisión en `docs/autonomia/revision/area1.md`: 5 defectos demostrados (D1 a D5).
- **Pruebas nuevas**, que antes no compilaban porque no existía la función:
  - `doc_link::tests::a_signed_document_detects_later_changes`
  - `doc_link::tests::a_signed_consent_can_be_revoked_without_losing_it`
  - `doc_link::tests::a_minor_does_not_sign_their_own_document_online`
  - `health::tests::a_signed_health_form_is_sealed`
  - `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station` (en red, con TLS y sesiones de verdad)
  - `shared::tests::a_station_cannot_change_signed_records_in_the_shared_folder` (con y sin cambios de otros puestos a la vez)
  - `ui_tests::a_signed_consent_is_revoked_from_the_patient_file`
  - `ui_tests::a_signed_health_form_cannot_be_edited_in_the_file`
- **Invariante:** la fusión de pacientes sigue pasando lo firmado a la ficha que se conserva. Lo comprueba `bank::tests::merging_moves_everything_of_the_removed_patient`, que pasa. El servidor y la carpeta compartida permiten el cambio de paciente solo cuando ese paciente se elimina en la misma tanda.
- **Capturas sintéticas a 1280×800:**
  - `docs/autonomia/capturas/f1_area1_salud_1280x800.png`
  - `docs/autonomia/capturas/f1_area1_salud_respuestas_1280x800.png`
  - `docs/autonomia/capturas/f1_area1_documentos_1280x800.png`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 314 pasan, 0 fallan, 10 ignoradas |

## E-007 Área 2: recetas y vademécum
- Revisión en `docs/autonomia/revision/area2.md`: 5 defectos demostrados (P1 a P5). Fuentes oficiales en docs/normativa.md.
- **Pruebas nuevas**, que antes no compilaban porque no existían las funciones:
  - `finance::tests::only_a_dentist_or_doctor_with_licence_prescribes`
  - `finance::tests::official_list_drugs_get_the_same_warnings`
  - `finance::tests::warnings_are_reviewed_before_issuing`
  - `finance::tests::an_issued_prescription_is_kept_as_issued` (incluye la protección del servidor)
  - `nomenclator::tests::an_old_official_list_is_flagged`
  - `ui_tests::a_prescription_needs_a_prescriber_and_reviewed_warnings` (ventana de 1280×1600). Comprueba:
    - la higienista no queda como prescriptora;
    - no se imprime sin revisar los avisos;
    - la higienista no puede darlos por revisados;
    - la doctora sí, y queda anotado quién.
- **Captura sintética:** `docs/autonomia/capturas/f1_area2_receta_1280x800.png`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 320 pasan, 0 fallan, 10 ignoradas |

## E-008 Correcciones de la revisión automática de seguridad de los commits `b2cf77b` y `00838cc`
| Hallazgo | Corrección | Prueba de regresión, en rojo antes del arreglo |
|---|---|---|
| Un puesto podía «des-emitir» una receta borrando su código y cambiarla después | `firmado::keep_issued_rx` no acepta un código vacío, solo otro válido. En pantalla, «Quitar el código» pasa a ser «Corregir el código» | `finance::tests::an_issued_prescription_cannot_be_un_issued` |
| La revisión de los avisos de receta solo se comprobaba en pantalla | El servidor solo la acepta de un usuario cuyo colaborador puede recetar (`collaborator_problem`) y pone su nombre. Tampoco acepta cambiar el prescriptor a quien no puede recetar | `server::tests::only_a_prescriber_marks_prescription_warnings_reviewed` |
| El servidor deducía una «fusión» de un paciente borrado en la misma tanda, y así un puesto podía pasar lo firmado a otro paciente | Fusionar pacientes es la operación del servidor `ChainOp::MergePatients`, solo para Dirección; en carpeta compartida se hace con el fichero bloqueado. El servidor y `Db::keep_signed` ya no aceptan cambios de paciente en lo firmado o emitido | `server::tests::signed_records_only_change_patient_through_a_merge_on_the_server` y `shared::tests::signed_records_change_patient_only_through_a_merge_in_the_shared_folder` |

**Ventaja añadida:** en red, la fusión la hace el servidor con todos los datos, incluidos los que no recibe el puesto (recetas, imágenes), así que no queda nada sin pasar.

**Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 324 pasan, 0 fallan, 10 ignoradas |

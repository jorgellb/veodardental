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

## E-009 Área 3: odontograma, periodontograma e historial
- Revisión en `docs/autonomia/revision/area3.md`: 4 defectos demostrados (O3 a O6).
- **Pruebas nuevas**, todas en rojo antes del arreglo:
  - `stock::tests::marking_a_planned_treatment_done_on_the_chart_completes_it`
  - `stock::tests::the_filling_code_follows_the_faces_only_within_fillings`
  - `stock::tests::a_treatment_marked_done_by_mistake_goes_back_to_planned`
  - `server::tests::the_same_treatment_done_on_two_stations_spends_its_material_once`
    - Antes: el composite pasaba de 5 a 4,6 (doble consumo).
    - Ahora: pasa a 4,8. Deshacer y volver a hacer gasta otra vez.
  - `shared::tests::the_same_treatment_done_on_two_stations_spends_once_in_the_shared_folder`
  - `ui_tests::the_chart_completes_planned_treatments_and_can_undo_them`
- **Invariante:** `stock::tests::deleting_a_done_treatment_returns_its_material` sigue pasando con la devolución de material compartida entre borrar y deshacer.
- **Captura sintética:** `docs/autonomia/capturas/f1_area3_odontograma_1280x800.png`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 330 pasan, 0 fallan, 10 ignoradas |

## E-010 Área 4: bancos y conciliación
- Revisión en `docs/autonomia/revision/area4.md`: 11 defectos demostrados (B1 a B11).
- **Demostraciones del comportamiento anterior**, ejecutadas:
  - `parse_money` anterior, copiado en un programa aparte:

    | Entrada | Resultado anterior |
    |---|---|
    | «1,234.56» | `Some(1.23456)` |
    | «(12,50)» | `Some(12.5)` |
    | «−12,50» | `Some(12.5)` |
    | «12,50-» | `None` (fila perdida) |
    | «1.234.567» | `None` (fila perdida) |

  - `chrono::NaiveDate::parse_from_str("05/11/26", "%d/%m/%Y")` → `Ok(0026-11-05)`: es el primer formato que probaba `parse_any_date`.
  - Flujo anterior en red, con una prueba temporal que no queda en el repositorio: dos puestos que importan los mismos 2 movimientos en el puesto y los envían dejan el servidor de 3 a 7 movimientos (duplicados).
- **Por lectura del código anterior (`dee610b`):**
  - B1: solo se leían los registros 22 y 23.
  - B3: la comprobación de repetidos incluía los movimientos recién añadidos del mismo extracto.
  - B5: la primera columna que contenía «importe» era la de cargo.
  - B7 y B8: `find` tomaba el primer cobro del importe, anulado o no, y no había regla para devoluciones a pacientes.
  - B9: el panel marcaba el gasto como pagado y deshacer no lo devolvía.

  Las pruebas nuevas describen el comportamiento correcto que el código anterior no tenía.
- **Pruebas nuevas:**
  - `bank::tests::norma43_with_several_accounts_and_control_totals`
  - `bank::tests::a_cut_norma43_file_is_not_imported`
  - `bank::tests::identical_moves_in_a_statement_are_all_kept_and_not_reimported`
  - `bank::tests::statement_amounts_keep_their_sign_and_cents`
  - `bank::tests::statement_tables_are_read_whole_or_not_at_all`
  - `bank::tests::ambiguous_matches_are_left_for_the_user`
  - `bank::tests::refunds_are_reconciled_and_cancelled_payments_are_not`
  - `bank::tests::undoing_a_reconciliation_restores_what_it_changed`
  - `bank::tests::undoing_a_remittance_match_uncollects_it`
  - `bank::big_bank_tests::finding_doubtful_moves_in_a_big_clinic_is_quick`
  - `server::tests::a_statement_imported_and_reconciled_by_two_stations_at_once_counts_once`
  - `shared::tests::a_statement_imported_by_two_stations_counts_once_in_the_shared_folder`
  - `ui_tests::a_doubtful_bank_move_is_chosen_by_hand_and_can_be_undone`
- **Invariantes que siguen pasando:**
  - `bank::tests::reconcile_transfer_cards_and_expense`
  - `bank::tests::reconcile_a_remittance_with_a_returned_debit`
  - `bank::tests::a_remittance_credited_without_the_returned_debits_is_reconciled`
  - la prueba de remesas y devoluciones en red de `server::tests`
- **Medición** del cálculo de movimientos dudosos (`bank::big_bank_tests`):
  - Datos: 100.000 cobros sintéticos en 3 años, semilla 42, y 300 movimientos pendientes; perfil `dev` (optimizado con depuración), este equipo.
  - Primera versión, recorriendo todos los cobros por movimiento: 682, 715, 681, 684 y 687 ms (5 ejecuciones).
  - Con índices por importe y por día: 18, 14, 12, 14 y 12 ms.
  - `match_candidates` de un movimiento: 12 ms.
- **Capturas sintéticas:** `docs/autonomia/capturas/f1_area4_bancos_1280x800.png` y `docs/autonomia/capturas/f1_area4_bancos_1280x720.png`. La fecha se lee entera. A 720 px la página se desplaza en vertical.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 343 pasan, 0 fallan, 10 ignoradas |

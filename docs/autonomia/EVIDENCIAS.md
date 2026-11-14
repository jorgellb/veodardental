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

## E-011 Área 5: informes
- Revisión en `docs/autonomia/revision/area5.md`: 7 defectos demostrados (I1 a I7). Las definiciones de cada total están en `src/informes.rs`.
- **Cuadre independiente:** `informes::tests::november()` crea operaciones sintéticas.
  - Contenido:
    - dos centros, dos profesionales y una mutua (copago y baremo);
    - una anulación, una devolución y un recibo devuelto;
    - dos facturas y una rectificativa;
    - cobros en el último segundo de octubre y el primero de diciembre.
  - Los totales esperados están calculados a mano a partir de esa lista (no con el código probado), y todos cuadran al céntimo:
    - cobrado: 509 − 72 − 60 = 377 €; por centro, 177 + 200 €;
    - producción: 437,50 + 50 €, por profesional y por centro;
    - facturado: 172 − 100 = 72 €;
    - deuda: 100,50 € y 30 € a favor a 30/11;
    - mutuas: 50 € liquidados en noviembre y cobrados en diciembre.
- **Pruebas nuevas:**
  - `cents::tests::sums_are_exact_to_the_cent` (tipo `Cents` de T-R7-1, D-06)
  - `informes::tests::collected_matches_the_operations`
  - `informes::tests::production_matches_the_operations`
  - `informes::tests::invoiced_matches_the_operations`
  - `informes::tests::debt_at_a_cut_off_date`
  - `informes::tests::insurers_match_the_operations`
  - `informes::commission_tests::the_commission_counts_the_insurer_part_only_if_marked`
  - `screens::cobros::tests::the_payment_methods_add_up_to_the_total`
  - `ui_tests::the_economic_summary_shows_its_totals_and_filters`
  - `server::tests::a_station_is_warned_when_the_server_has_another_time_zone`
- **Por lectura del código anterior (`f7ea39c`):**
  - I1: las casillas recorrían `PAY_METHODS`, que no tiene «Domiciliación».
  - I2: `amount()` es solo la parte del paciente, y nadie sumaba `insurer_fee`.
  - I4 e I5: no había ni centro en los tratamientos ni fecha en la deuda.
  - I7: `access()` no decía la zona horaria.
- **Medición de «Pendientes de cobro»** (I6):
  - Datos: 5.018 pacientes y 100.013 cobros sintéticos (semilla 7); perfil `dev`, este equipo; prueba temporal que no queda en el repositorio.
  - Antes (`patient_balance` por paciente): 368 ms por fotograma.
  - Ahora (`balances_at`): 2,6 ms.
  - Mismo resultado: 3 deudores.
- **Capturas sintéticas:** `f1_area5_informes_t{7,2,0}_1280x{800,720}.png` y `f1_area5_cobros_1280x{800,720}.png`:
  - resumen económico, producción con las columnas paciente, mutua y total, y deuda a una fecha;
  - a 1280 px se leen sin recortes, y a 720 px de alto la página se desplaza.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 353 pasan, 0 fallan, 10 ignoradas |

## E-012 Área 6: comunicaciones
- Revisión en `docs/autonomia/revision/area6.md`: 7 defectos demostrados (C1 a C7).
- **Demostraciones ejecutadas:**
  - Servidor SMTP de prueba que recibe el correo entero y se corta sin «250»: `send_tracked` da `Uncertain`. El `send` anterior lo devolvía como un error más y `email_failed` lo dejaba para reintentar: hasta 3 envíos del mismo correo.
  - Con un 550 tras los datos, o con un puerto cerrado: `NotSent`, y se reintenta.
  - Servidor HTTP de prueba que recibe la petición de WhatsApp y se corta: el error empieza por «No se sabe si llegó». Antes era «Sin conexión con WhatsApp» y el mensaje quedaba «No se pudo enviar».
- **Por lectura del código anterior (`87aa461`):**
  - C1: `start_outbox` tomaba los pendientes del puesto sin reservarlos.
  - C3: `target = day + days` (día natural).
  - C4 a C6: no había ni baja por finalidad, ni lectura de «BAJA», ni comprobación de comodines.
- **Pruebas nuevas:**
  - `mail::tests::a_cut_after_the_data_is_uncertain_and_a_rejection_is_not`
  - `comms::tests::reminders_skip_weekends_and_holidays`
  - `comms::tests::a_patient_can_opt_out_of_one_kind_of_message`
  - `comms::tests::emails_are_claimed_once_and_uncertain_ones_wait_for_a_decision`
  - `comms::tests::templates_only_use_placeholders_that_are_filled_and_no_health_data`
  - `whatsapp::tests::an_uncertain_send_is_not_retried_and_replies_still_count`
  - `whatsapp::tests::replying_baja_stops_whatsapp`
  - `whatsapp::tests::network_errors_tell_not_sent_from_uncertain`
  - `server::tests::two_stations_sending_emails_at_once_send_each_one_once`
- **Pruebas de pantalla ajustadas** porque dependían del día y la hora de ejecución (sin cambiar ninguna comprobación; detalle en area6.md):
  - `ui_tests::daily_emails_are_sent_automatically`
  - `ui_tests::patient_link_actions_reach_the_agenda`

  Las dos fallaron en la batería de las 19:00 de un viernes y pasan tras el ajuste.
- **Capturas sintéticas:** `f1_area6_comunicaciones_1280x{800,720}.png` y `f1_area6_filiacion_1280x{800,720}.png` (avisos que quiere recibir).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 362 pasan, 0 fallan, 10 ignoradas |

## E-013 Área 7: ortodoncia
- Revisión en `docs/autonomia/revision/area7.md`: 6 defectos demostrados (T1 a T6).
- **T1, demostrado en rojo antes del arreglo:** `server::tests::two_ortho_controls_recorded_at_once_are_both_kept` daba `left: 5, right: 6`. Se perdía el control del primer gabinete.
- **Por lectura del código anterior (`586a838`):**
  - T2: `c.state = s` sin registro.
  - T3: los recalls no se tocaban al finalizar.
  - T4: `doctor_id: 3` fijo.
  - T5: `fee` sin enlace.
- **Pruebas nuevas:**
  - `server::tests::two_ortho_controls_recorded_at_once_are_both_kept`
  - `extra::tests::ortho_phases_are_kept_and_a_finished_case_stops_its_controls`
  - `extra::tests::an_ortho_case_is_charged_through_its_budget_installments`
- **Prueba ajustada** por depender de la hora: `booking::tests::web_page_books_a_free_slot`. Falló a las 19:40 y pasa tras el ajuste, sin cambiar sus comprobaciones.
- **Capturas sintéticas:** `f1_area7_ortodoncia_1280x{800,720}.png` y `f1_area7_ortodoncia_cobro_1280x800.png`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 365 pasan, 0 fallan, 10 ignoradas |

## E-014 Área 8: IA de radiografías
- Revisión en `docs/autonomia/revision/area8.md`: 5 defectos demostrados (A1 a A5).
- **Por lectura del código anterior (`f080edd`):**
  - A1 y A2: `unchecked` (marcadas por defecto) y sin revisión.
  - A3: sin límite de pago.
  - A4: el texto del aviso.
  - A5: sin versión de las instrucciones.
- **Pruebas nuevas:**
  - `ia::tests::paid_agents_are_not_called_past_the_monthly_limit`
  - `ia::tests::an_analysis_is_a_draft_until_a_professional_reviews_it`
  - `server::tests::only_a_licensed_professional_reviews_an_ai_analysis`
- **Prueba ajustada al flujo nuevo:** `ui_tests::ai_analysis_falls_back_and_adds_to_plan`. Comprueba además que sin revisar no se añade nada.
- **Capturas sintéticas:**
  - `f1_area8_ia_1280x{800,720}.png`: análisis guardado;
  - `f1_area8_ia_revision_1280x800.png`: propuestas sin marcar y «Añadir al plan» desactivado en el borrador.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 368 pasan, 0 fallan, 10 ignoradas |

## E-015 Corrección de la revisión automática de seguridad del área 8 (`6e2a560`)
| Hallazgo | Corrección | Prueba |
|---|---|---|
| El servidor solo protegía la revisión. Un puesto podía cambiar lo que contestó la IA tras revisarlo, pasar un análisis de pago a gratuito (y saltarse el límite del mes) o borrar lo revisado, lo usado en el plan o lo de pago | `ia::protect_report`: paciente, imagen, fecha, agente, modelo, coste, intentos, instrucciones y resultado no cambian. La revisión la pone solo un profesional y después no cambia. Lo pasado al plan solo crece tras darlo por bueno. Solo se borra un borrador gratuito sin usar. `protect_new_report`: un análisis nuevo no llega revisado ni con propuestas en el plan si no lo revisa un profesional. La pantalla solo ofrece borrar lo que se puede borrar | `server::tests::only_a_licensed_professional_reviews_an_ai_analysis`, ampliada: un cambio del resultado, del coste y del modelo tras la revisión no queda; lo revisado y el borrador de pago no se borran; el borrador gratuito sí |

**Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 368 pasan, 0 fallan, 10 ignoradas |

## E-016 Área 9: cita online y portales
- Revisión en `docs/autonomia/revision/area9.md`: 1 defecto demostrado (P1). El resto de comprobaciones, revisadas sin defecto, con las pruebas existentes citadas.
- **P1, demostrado en rojo antes del arreglo:** las dos pruebas fallaban con «caducado». A los 31 días, el enlace seguía publicado.
- **Pruebas nuevas:**
  - `doc_link::tests::document_links_expire_and_resending_renews_them`
  - `budget_link::tests::budget_links_expire`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 370 pasan, 0 fallan, 10 ignoradas |

## E-017 Área 10: telefonía y laboratorio digital
- Revisión en `docs/autonomia/revision/area10.md`: 6 defectos demostrados (F1 a F4, L1 y L2). Todas las pruebas nuevas estaban en rojo antes del arreglo:
  - `phone::tests::only_real_numbers_and_no_repeated_calls`: «<script>» se aceptaba como número.
  - `phone::tests::web_pages_cannot_fake_calls_and_floods_are_cut`: con `Sec-Fetch-Site: cross-site` devolvía 200.
  - `lab_order::tests::attachments_too_big_for_an_email_are_left_out`: sin aviso, y el adjunto entraba.
  - `server::tests::a_lab_work_received_on_two_stations_is_one_expense`: `left: 2, right: 1`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 374 pasan, 0 fallan, 10 ignoradas |

## E-018 T-F1-11: todas las pantallas a 1280×720 y 1280×800
- **Prueba nueva:** `ui_tests::every_screen_fits_1280_wide`.
  - **Qué recorre**, a 1280×720 y a 1280×800:
    - las 21 pantallas del menú;
    - las 11 pestañas de la ficha del paciente;
    - las 8 pestañas de Informes;
    - los 18 apartados de Configuración.
  - **Cuándo falla:** si algún elemento del árbol de accesibilidad (botones, etiquetas, casillas, tablas) se sale por la derecha o la izquierda de la ventana. El texto de una casilla de escribir más largo que ella no cuenta: la casilla lo recorta. Lo que no cabe de alto se desplaza.
  - **Primera ejecución (antes de los arreglos):** 34 elementos fuera en 7 pantallas; con las pestañas y apartados, 5 apartados de Configuración más.
- **Causas y arreglos:**
  - `Table::scale` no estrechaba las tablas por debajo del 70 %, y la fila ocupaba todo lo «disponible» aunque pasara del borde visible. Ahora se estrecha hasta el 50 % (las celdas ya recortan su texto) y no pasa del borde visible (`visible_width`). Arreglados Listín, Almacén y Caja.
  - Las páginas con desplazamiento y los apartados de Configuración median lo disponible incluyendo la barra de desplazamiento: ahora usan `theme::fit_width` (el borde visible y el de la ventana).
  - `field` en una fila que se parte: egui no parte un bloque vertical. Ahora el campo pasa solo a la línea siguiente si no cabe, y si no, se estrecha a lo que queda.
  - Filas concretas que pasan a partirse:
    - Colaboradores: comisión y campos;
    - Almacén: ficha del producto;
    - Tarifas: cabecera del catálogo;
    - Correo: horario del envío;
    - Cita online: huecos;
    - Exportar fichas: carpeta;
    - Copias: carpeta.

    En Receta electrónica, la tabla de profesionales va con desplazamiento lateral, porque ensanchaba todo lo de debajo.
- **Capturas sintéticas tras los arreglos:**
  - `f1_11_{colaboradores,almacen,tarifas,listin,caja}_1280x800.png`
  - `f1_11_filiacion_1280x720.png`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 375 pasan, 0 fallan, 10 ignoradas |

## E-019 T-F1-12a a T-F1-12d: concurrencia, fallos, protocolo y entradas al azar
- **Inventario y resultados:** `docs/autonomia/concurrencia.md`.
- **Defecto demostrado y corregido (D1):** en la carpeta compartida, dos ordenadores sin número de puesto (0) o con el mismo sacaban los mismos identificadores y recibos. La prueba `shared::tests::each_computer_gets_its_own_station_number_in_the_shared_folder` fallaba antes del arreglo con «0 y 0». Ahora `shared::pick_station` da un número libre al guardar y rechaza uno en uso.
- **Pruebas nuevas:**
  - **Comprueban lo que ya funcionaba**, y quedan como regresión:
    - `server::tests::a_push_resent_after_a_lost_answer_changes_nothing`
    - `server::tests::the_server_restarts_with_everything_and_stations_carry_on`
    - `server::tests::a_failed_write_leaves_nothing_half_saved`
  - **Entradas al azar con semilla fija** (`test_rng::Rng`):
    - `server::tests::the_request_reader_survives_random_bytes`
    - `server::tests::the_server_survives_random_requests_and_its_data_stays_readable`
    - `bank::fuzz::norma43_survives_random_files`
    - `bank::fuzz::statement_tables_and_amounts_survive_random_input`
    - `shared::tests::merging_random_edits_keeps_what_each_station_did`
- **Notas de las pruebas al azar:**
  - Un fallo de la prueba de combinación (un mismo paciente cambiado por los dos puestos) era de la propia prueba: es un conflicto, no un cambio de uno solo. Se corrigió la prueba, no el código.
  - El servidor con peticiones al azar llegó a aceptar cambios en `/cambios`, leer `/datos` y rechazar con 400 (histograma comprobado durante la prueba).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 384 pasan, 0 fallan, 10 ignoradas |

## E-020 Reglas transversales T-R7-2, T-R7-3 y T-R7-4
- **T-R7-2. Autoridad del servidor: numeración, cadena, permisos y operaciones irrepetibles con reintento.**
  - Numeración y cadena:
    - `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain`
    - `two_new_patients_never_share_a_history_number`
    - `sample::tests::receipts_are_unique_per_station`
    - `shared::tests::each_computer_gets_its_own_station_number_in_the_shared_folder`
  - Permisos de cada operación: el final de `server::tests::remittances_and_claims_made_by_two_stations_at_once_charge_once`, más los de banco, IA y receta.
  - Operaciones irrepetibles: las de `docs/autonomia/concurrencia.md`. La segunda vez se rechaza y el puesto ve el estado real.
  - Reintento tras respuesta perdida: `server::tests::a_push_resent_after_a_lost_answer_changes_nothing`.
- **T-R7-3. Cada anulación conserva el original y su referencia.**
  - **Defecto corregido:** anular una liquidación a mutua la borraba. Si era la última, su número se volvía a usar.
    - **Ahora:** queda «Anulada» con fecha y motivo, y sus actos vuelven a pendientes (`cancel_claim`).
    - **Prueba:** `insurer::tests::a_cancelled_claim_stays_on_record_and_its_number_is_not_reused`.
  - **El resto, con sus pruebas:**
    - cobros: anulación con `cancels` (`finance::tests::a_wrong_payment_is_cancelled_and_leaves_the_cash_balanced`, `ui_tests::installment_paid_in_cash_and_cancelled_from_the_dialog`);
    - facturas: rectificativa y anulación VERI*FACTU encadenadas (`verifactu::tests::corrections_and_cancellations_join_the_chain`);
    - remesa cobrada por error: cobros anulados, no borrados (`bank::tests::undoing_a_remittance_match_uncollects_it`);
    - recibos devueltos: `bank::tests::reconcile_a_remittance_with_a_returned_debit`;
    - consentimientos revocados sin perderlos: `doc_link::tests::a_signed_consent_can_be_revoked_without_losing_it`;
    - tratamiento vuelto a previsto con su material: `stock::tests::a_treatment_marked_done_by_mistake_goes_back_to_planned`;
    - análisis con IA revisados que no se borran: E-015.
- **T-R7-4. Escritura interrumpida, sin espacio y diario truncado.**
  - **Defecto corregido:** con una línea del diario dañada en medio, al abrir se aplicaban las de después y quedaba un hueco sin avisar.
    - **Ahora:** se recupera hasta el último cambio entero (`sync::replay_journal`).
    - **Prueba:** `server::tests::a_cut_or_damaged_journal_never_applies_changes_out_of_order`, en rojo antes del arreglo (`left: 2, right: 0`). Cubre también la última línea cortada por un apagón.
  - **Ya cubierto:**
    - `local::tests::saves_only_what_changed_and_comes_back_after_a_crash`
    - `server::tests::a_failed_write_leaves_nothing_half_saved` (disco lleno o sin permiso)
    - `server::tests::the_server_restarts_with_everything_and_stations_carry_on`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 386 pasan, 0 fallan, 10 ignoradas |

## E-021 Fase 2: motor de migración, asistente e informe de conciliación (T-F2-01 a T-F2-09)
Todo con **datos sintéticos** (inventados y señalados así en cada prueba), sin datos reales. La documentación de usuario está en `docs/migracion.md`, y las decisiones en D-18, D-19 y D-20.

- **T-F2-01. Motor e identificadores de origen.**
  - **Qué hay:** `src/migracion.rs`. Cada registro importado guarda programa, tabla o fichero, clase de dato e identificador (`Db::imports`, sincronizado por registro como los demás).
  - **Repetir no duplica, y una importación cortada se termina repitiéndola,** aunque las filas vengan en otro orden: `repeating_or_finishing_a_migration_never_duplicates`.
  - **La fusión posterior de dos pacientes conserva el origen:** `merging_patients_keeps_their_origin`.
- **T-F2-02. Entidades,** probadas en `a_full_migration_with_its_reconciliation` y `an_end_to_end_synthetic_migration_squares`:
  - pacientes con su responsable, alertas y alergias;
  - tratamientos hechos y pendientes;
  - presupuestos de varias líneas;
  - citas, con años de dos cifras, anuladas y no acude;
  - cobros, con el recibo del origen y sin caja;
  - saldos;
  - documentos e imágenes, con su ruta.
- **Lo de antes, como historia:** los tratamientos hechos no gastan material, no crean recalls y no salen para facturar ni para liquidar a la mutua (D-18). Para eso hay un campo nuevo, `MedEntry::settled_elsewhere`, que respetan `uninvoiced`, la emisión de facturas y `claimable`.
- **Ajuste de saldo (D-20):**
  - **Qué es:** el apunte `MIGSALDO`, que no es producción ni comisión (`MedEntry::is_act` en informes, inicio, liquidación de colaboradores, procedencia y comodines).
  - **Prueba:** `the_balance_adjustment_matches_the_old_debt_once`.
  - **Pacientes unidos:** no se les ajusta (`a_linked_patient_is_never_adjusted`).
- **T-F2-03. Asistente.**
  - **Qué hay:** `src/screens/migrar.rs`, en Configuración › Importar datos:
    - hojas desde fichero, carpeta (`a_folder_loads_each_sheet_by_its_name`) o SQL Server;
    - columnas con un ejemplo por campo, vista previa y perfiles;
    - simulacro aislado (`dry_run`, sobre una copia) con errores por fila;
    - duplicados decididos uno a uno;
    - importación solo tras un simulacro de lo de ahora, con copia de seguridad previa y registro de actividad.
  - **Prueba en pantalla:** `ui_tests::migration_wizard_simulates_decides_and_imports`. Comprueba:
    - el simulacro no cambia nada y nada se sale a 1280 de ancho;
    - un duplicado que solo comparte el NIF no ofrece «unirlo»;
    - al decidirlo hay que repetir el simulacro;
    - la importación hace lo decidido.
  - **Origen en solo lectura:** la prueba de extremo a extremo comprueba que los ficheros CSV conservan bytes y fecha.
  - **Capturas:** `capturas/f2_migracion_hojas_1280x800.png` y `capturas/f2_migracion_informe_1280x800.png`.
- **Duplicados (D-19).**
  - **Dos personas nunca se unen por el NIF ni por el número de historia:** `duplicates_are_decided_and_a_shared_nif_never_joins_two_people` y la fila de P2 en `a_full_migration_with_its_reconciliation`.
  - **Ninguna fila se pierde:** filas = nuevos + ya estaban + sin importar (con su porqué), para cada clase de dato, en `a_full_migration_with_its_reconciliation`.
- **T-F2-04. Informe de conciliación.**
  - **Qué compara:** por paciente y en total, en céntimos exactos:
    - pacientes;
    - deuda (y la de los pacientes sin importar, aparte);
    - citas futuras;
    - presupuestos abiertos;
    - tratamientos pendientes.
  - **Sin doble cómputo:** la deuda de aquí sale de tratamientos menos cobros importados, y el ajuste es solo la diferencia.
  - **En texto:** `the_report_reads_as_text`.
- **T-F2-05 y T-F2-06. GESDEN y Novaclinic.**
  - **Lo que se conserva:** el catálogo y las tarifas se importan como antes.
  - **Pacientes:** entran en el motor con `mssql::gesden_patients_for_migration`, que es la consulta conocida más `IdPac`, el identificador que no se repite. Sus columnas se asignan solas (`gesden_patient_columns_are_assigned_by_themselves`).
  - **Lo demás:** se lee con «Desde SQL» y se asigna y valida en el asistente.
  - **Sin comprobar con datos reales:** el mapeo real del resto. Sigue en B-03 y B-04 (T-F2-10 y T-F2-11); no se ha inventado ninguna tabla.
- **T-F2-07. Perfiles de columnas** que se guardan y se eligen (`ClinicConfig::mig_profiles`, `a_saved_profile_assigns_the_columns_again`).
  - **Pendiente:** los perfiles de Clinic Cloud y Dentalink con sus columnas reales esperan a sus exportaciones (B-05, T-F2-12).
- **T-F2-08. Formatos, versiones y campos soportados:** `docs/migracion.md`.
- **T-F2-09. Simulacro reproducible de extremo a extremo:** `an_end_to_end_synthetic_migration_squares`. Recorre todo el camino:
  1. Seis CSV sintéticos en una carpeta temporal, leídos con el lector de siempre.
  2. El simulacro, que no cambia los datos ni los ficheros.
  3. La importación, igual al simulacro, con el informe cuadrado en todo.
  4. Una repetición que no añade nada.
- **Seguridad:** la ruta de red de un documento (`\\servidor\recurso`) no se comprueba al importar, porque en Windows mirarla ya conecta con ese servidor y le da las credenciales del usuario. La prueba es la fila D3 de `a_full_migration_with_its_reconciliation`.
- **Prueba existente ajustada:** `import_patients_from_a_csv_in_the_settings_screen` va en una ventana más alta, porque encima está ahora la tarjeta de la migración. No cambia nada de lo que comprueba.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 401 pasan, 0 fallan, 10 ignoradas |

## E-022 Revisión de seguridad de la fase 2, ficheros privados y ensayo de instalación (T-F3-01)
- **Revisión de seguridad automática del commit `e4e3298`: dos hallazgos, corregidos.**
  - **Hallazgo:** la guarda de rutas de red de los documentos importados no reconocía las barras mezcladas (`\/servidor`) ni los prefijos de dispositivo (`\\?\UNC\`, `\\.\`, `\??\`). Además, aunque no se mirara al importar, la galería abriría después esa ruta. En los dos casos, Windows daría las credenciales del usuario a ese servidor.
  - **Ahora:**
    - una ruta de red solo se importa dentro de la carpeta de documentos que la clínica indica, sin `..`;
    - las demás quedan fuera con su explicación (D-21, `docs/migracion.md`).
  - **Pruebas:**
    - `migracion::tests::network_paths_are_recognised_and_only_trusted_inside_the_documents_folder`;
    - la fila D3 de `a_full_migration_with_its_reconciliation`.
- **Defecto de permisos demostrado y corregido.**
  - **Qué pasaba:** los ficheros siguientes se creaban con los permisos por defecto del sistema (0644 en Linux): cualquier otra cuenta del mismo ordenador podía leer los datos de salud. Solo `servidor.json` se protegía.
    - el fichero de datos de la clínica (`clinica.json`) y su diario, tanto en un ordenador como en el servidor;
    - las sesiones del servidor;
    - las copias de seguridad sin cifrar.
  - **Ahora:**
    - todos se crean solo para el usuario (0600), con `paths::create_private`, `append_private` y `write_private`;
    - los de versiones anteriores se corrigen al volver a escribirlos.
  - **Fuera, a propósito:** la carpeta compartida, porque la comparten varios puestos; su retirada es de la fase 4.
  - **Prueba:** forma parte del ensayo. Sin el arreglo, falla con `0o644`.
- **T-F3-01. Ensayo de instalación en Linux con raíces aisladas:** `server::tests::installation_rehearsal_on_linux`.
  1. El servidor crea su clave y su identidad una vez (0600). Repetirlo no las cambia.
  2. Arranca con ellas.
  3. Un puesto se da de alta y apunta la huella que enseña el servidor; con una clave mala no entra.
  4. Recepción no puede conciliar el banco y Dirección sí.
  5. Datos, diario, sesiones y copias quedan en 0600.
  6. El cifrado del disco se comprueba de verdad. En este ordenador sale `Unknown` (las pruebas van en tmpfs); el análisis de BitLocker, FileVault y LUKS tiene sus propias pruebas en `disco::tests`.
  - **Pendiente fuera de aquí:** Windows y macOS reales siguen en B-24 (T-F3-01X).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 403 pasan, 0 fallan, 10 ignoradas |

## E-023 T-F3-02: actualizaciones coordinadas
- **Defectos de pérdida de datos demostrados y corregidos.**
  - **1. Programa antiguo con datos más nuevos.**
    - **Qué pasaba:** los cargaba y, al guardar o al reescribir el diario, borraba los campos que no conocía.
    - **Ahora:** los abre sin guardar, con el mensaje de actualizar; no reescribe el fichero ni quita el diario. El servidor no arranca con ellos.
  - **2. Puesto antiguo con servidor nuevo (o al revés).**
    - **Qué pasaba:** el puesto escribía registros sin los campos nuevos.
    - **Ahora:** cada puesto manda su versión de los datos (`X-Datos`) y el servidor rechaza sus escrituras (426) si no es la misma, con un mensaje que dice qué actualizar. Leer sí puede.
    - **Al conectar:** `/acceso` dice la versión del servidor, y el puesto avisa con las versiones de los dos.
  - **3. Lo no enviado se perdía al cerrar el puesto.**
    - **Qué pasaba:** lo que un puesto no podía enviar (sin conexión, o porque hay que actualizar) solo estaba en memoria.
    - **Ahora:** va a una bandeja en su ordenador (`pendientes.json`, 0600), cuando falla un envío y al cerrar. Se envía en la sesión siguiente y se borra al llegar.
- **Copia previa:**
  - antes de completar datos de una versión anterior (`Db::load_from`);
  - antes de cambiar el programa: si no se puede hacer, no se actualiza.
- **Paquetes de versión (`src/actualizar.rs`):**
  - manifiesto firmado con Ed25519 (`ring`), con versión, versión de datos y tamaño y SHA-256 de cada fichero;
  - se rechazan: sin firma válida, con un fichero cambiado o de más, con nombres con ruta o si la versión no es más nueva;
  - sin clave de firma configurada (`RELEASE_KEYS` vacía) no se acepta ninguna actualización. Es B-27: la clave la genera el responsable del producto.
- **Aplicación por pasos con diario:**
  1. Preparado.
  2. Copia.
  3. Cambio, con dos renombrados.
  4. Comprobación de la versión nueva con `--comprobar`, que abre los datos sin escribir.
  - Si la comprobación falla, vuelve sola a la anterior.
  - `--volver-atras` vuelve a mano y dice si hay que restaurar la copia: solo si la versión nueva cambió la de los datos.
  - Solo en una carpeta propia del programa; nunca en una instalación del sistema.
- **Pruebas:**
  - `local::tests::newer_data_is_never_rewritten_and_older_data_is_backed_up_first`
  - `server::tests::stations_and_server_with_other_data_versions_do_not_write_to_each_other`
  - `server::tests::unsent_changes_survive_in_the_outbox_and_reach_the_server_later` (reenviarla no cambia nada)
  - `actualizar::tests::only_a_signed_complete_and_newer_package_is_accepted`
  - `actualizar::tests::an_update_installs_checks_and_rolls_back_by_itself_if_the_new_version_fails`
  - `actualizar::tests::a_cut_between_any_two_steps_leaves_the_previous_version_whole`: corte tras cada paso y entre los dos renombrados. Al arrancar queda la versión anterior entera, con los datos intactos, sin diario, y se puede volver a actualizar.
  - `actualizar::tests::undoing_after_the_data_changed_says_what_to_restore`
- **A mano:**
  - `--comprobar`: «VeodarDental 0.1.0 · datos 1 · 18 pacientes», código 0.
  - `--actualizar` en una instalación del sistema: lo rechaza, código 1.
- **Prueba ajustada:** `local::tests::only_one_window_saves` falló una vez en la batería completa. Mientras otra prueba del mismo proceso lanza un programa (el ensayo de instalación lanza `lsblk`), el hijo tiene una copia del fichero de bloqueo hasta que arranca. Ahora espera hasta un segundo a que se libere; comprueba lo mismo. Después, dos baterías seguidas en verde.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` (dos veces) | 0 | 411 pasan, 0 fallan, 10 ignoradas |

## E-024 T-F3-03: firma y empaquetado, y revisión de seguridad de la bandeja
- **Revisión de seguridad automática del commit `7704bd1`: dos hallazgos, corregidos.**
  - **Hallazgos:**
    - **Suplantación:** lo pendiente de un usuario se enviaba con la sesión del siguiente que entrara, con sus permisos.
    - **Pérdida de datos:** un envío correcto de otro usuario borraba esa bandeja ajena.
  - **Ahora:**
    - una bandeja por usuario y servidor (`server::outbox_path(usuario, huella)`), que solo se envía cuando entra ese mismo usuario en ese mismo servidor;
    - al reenviar se omiten, con aviso, los cambios de usuarios, puestos, conexiones y configuración (`server::replayable`);
    - cada envío correcto borra solo la bandeja propia.
  - **Prueba:** la de la bandeja comprueba las rutas por usuario y servidor y lo que no se reenvía.
  - **Límite aceptado:** quien puede escribir en la carpeta del usuario del sistema podría editar su bandeja. También podría cambiar el programa o los datos de ese ordenador, así que queda fuera de lo que esto protege. Los cambios de administración no se reenvían nunca desde ella.
- **T-F3-03.**
  - **Flujo de instaladores:** `.github/workflows/instaladores.yml`.
    - Firma de Windows (signtool, con sello de tiempo y verificación) y de macOS (codesign con *hardened runtime*, notarización y grapado). Cada una solo se hace si están sus secretos.
    - Firma del paquete de actualización (B-27), con el ejemplo nuevo `examples/firmar_version.rs`.
    - `FIRMA.txt` en cada artefacto dice qué se ha firmado: nada se declara firmado sin firma.
    - Comprobación del `.deb` por dentro en Linux.
    - Las acciones van fijadas a su commit, porque esos pasos manejan secretos (aviso de semgrep). Los commits salen de `git ls-remote` de cada repositorio.
  - **Empaquetado comprobado en Linux, en este ordenador:**
    - `cargo packager --release --formats deb`: código 0, `dist/veodardental_0.1.0_amd64.deb` (20 MB; `dist/` no va al repositorio).
    - Por dentro: `usr/bin/veodardental` ejecutable, `veodardental.desktop` e iconos de 32, 128, 256 y 512.
    - El binario empaquetado, con raíz aislada: `--comprobar` → «VeodarDental 0.1.0 · datos 1 · 18 pacientes», código 0.
  - **Firma de versiones comprobada:**
    1. Clave Ed25519 desechable de OpenSSL, en la carpeta temporal y fuera del repositorio.
    2. `firmar_version` escribe el manifiesto.
    3. `openssl pkeyutl -verify` sobre él: «Signature Verified Successfully», código 0.
  - **Sin firmar todavía:** los binarios no están firmados ni notarizados. Faltan los certificados (B-25) y la clave de versiones (B-27).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 411 pasan, 0 fallan, 10 ignoradas |

## E-025 T-F3-04: registro de errores e informe para soporte, y revisión de seguridad de la CI y de la sesión caducada
- **Revisión de seguridad automática del commit `c08cd25`: dos hallazgos, corregidos.**
  - **CI.**
    - **Hallazgo:** una ejecución a mano desde cualquier rama recibía las claves de firma.
    - **Ahora:**
      - todas las firmas exigen una etiqueta `v*`;
      - el trabajo de instaladores solo lee (`contents: read`), y publicar es un trabajo aparte, el único con permiso de escritura y solo con etiqueta;
      - el flujo recomienda guardar los secretos en un entorno de GitHub con revisores obligatorios, que configura el responsable.
  - **Sesión caducada.**
    - **Hallazgo:** era un comportamiento anterior a este trabajo. Los cambios en memoria de un usuario se enviaban con la sesión del siguiente que entrara, y al reconectar se perdían.
    - **Ahora:** al caducar la sesión o al reconectar, van a la bandeja de su dueño. Si entra otro usuario, no se envían con su sesión y los datos se recogen de nuevo (`App::unsent_owner`).
- **T-F3-04 (`src/registro.rs`).**
  - **Registro local:** `paths::data()/registro`, solo para el usuario (0600), con rotación (actual y dos anteriores, 1 MB cada uno). Apunta los avisos, los errores del servidor y los fallos del programa (pánico, con el sitio del código).
  - **Minimización** antes de escribir:
    - correos, teléfonos y números largos, NIF/NIE, IBAN, rutas, claves y lo que va entre comillas o «»;
    - una línea por apunte, de 500 caracteres como mucho.
  - **Informe para soporte** (Configuración › Registro de cambios):
    - lleva versión, sistema y cuántos datos hay (no cuáles), y las últimas líneas ya minimizadas;
    - se prepara a mano y se ve entero; solo sale si se copia, se guarda o se envía;
    - enviarlo pide la casilla de consentimiento y una dirección de soporte, que por defecto está vacía. No hay telemetría.
  - **Pruebas:**
    - `registro::tests::personal_data_never_reaches_the_log`
    - `registro::tests::the_log_rotates_and_keeps_three_files_at_most`
    - `registro::tests::the_support_report_is_previewed_and_only_sent_with_consent` (ningún nombre ni NIF de los pacientes de ejemplo en el informe)
    - `ui_tests::support_report_is_previewed_and_needs_consent`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 415 pasan, 0 fallan, 10 ignoradas |

## E-026 T-F3-05: ensayo de restauración y clave de recuperación, y revisión de seguridad del registro
- **Revisión de seguridad automática del commit `2945231`: un hallazgo, corregido.**
  - **Hallazgo:** el texto de los avisos es libre. Un nombre de paciente sin comillas («No se puede borrar a Juan Pérez…») pasaba la minimización.
  - **Ahora:** los avisos y los errores del servidor se apuntan quitando además las palabras de los nombres que hay en los datos: pacientes y responsables, usuarios, colaboradores y contactos (`registro::log_free`, `scrub_with`).
  - **Prueba:** ampliada `personal_data_never_reaches_the_log`.
  - **Límite aceptado:** los fallos del programa (pánicos) se apuntan sin acceso a los datos: solo con la minimización por patrones. Salen de invariantes del código y no suelen llevar nombres.
- **T-F3-05.**
  - **Ya estaba, con sus pruebas:**
    - copias escalonadas: `automatic_copies_are_kept_staggered_for_a_year`;
    - avisos de fallo o de copia antigua: `a_failed_or_old_backup_is_warned`;
    - segunda copia cifrada: `second_copy_is_encrypted_with_the_backup_password`;
    - cambio de contraseña: `old_copies_are_encrypted_and_moved_to_a_new_password`.
  - **Nuevo — ensayo de restauración real aislada (`tools::restore_drill`, botón en Copias de seguridad):**
    - abre la copia en una carpeta aparte, como la abriría el programa;
    - compara registro a registro pacientes, citas, tratamientos, cobros, facturas, documentos e imágenes, además del total cobrado y la cadena de facturas;
    - no toca los datos en uso, borra la carpeta del ensayo (los datos descifrados no quedan) y deja el resultado en el registro de cambios.
  - **Nuevo — clave de recuperación:**
    - se guarda solo fuera de las carpetas de las copias y de la segunda copia (`cifrado::export_key`);
    - abre las copias en otro ordenador sin la contraseña, desde el diálogo de restaurar (`read_key_file`).
  - **Prueba:** `tools::tests::a_restore_drill_checks_the_contents_and_the_key_lives_apart`.
    - en las carpetas de copias solo hay copias;
    - guardar la clave allí se rechaza;
    - en otro ordenador, sin clave, se pide la contraseña; con la contraseña o con la clave de recuperación, el ensayo sale bien en todo;
    - los datos en uso no cambian;
    - una copia dañada no pasa.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 416 pasan, 0 fallan, 10 ignoradas |

## E-027 T-F3-06: VERI*FACTU contra una AEAT simulada
- **Defecto demostrado y corregido.**
  - **Qué pasaba:** un error SOAP del cliente sobre el envío entero (esquema no válido, certificado) se trataba como «sin conexión». El envío automático lo repetía cada dos minutos para siempre, con el mismo error.
  - **Ahora:**
    - las facturas quedan «Envío rechazado: revisar»;
    - el envío automático se para y el panel lo dice;
    - se reenvían a mano cuando esté corregido.
  - **Errores del servidor de la AEAT:** se reintentan como una caída (`client_fault`, `mark_rejected`, `needs_review`).
- **Doble de prueba explícito de la AEAT** (`fake_aeat`, en 127.0.0.1): contesta a cada envío lo que diga la prueba, o corta la conexión sin responder tras recibirlo.
  - `verifactu::tests::a_lost_answer_is_resent_identically_and_reconciled_as_registered`:
    1. Caída tras registrar: queda «sin conexión» y se reintenta sola.
    2. El reintento manda exactamente el mismo sobre (mismas huellas y fechas).
    3. La AEAT contesta «duplicado» (3000) y se concilia como registrada. No queda nada pendiente, y se guarda la espera pedida (180 s).
  - `verifactu::tests::a_rejected_submission_stops_the_automatic_retries_but_a_server_fault_does_not`: rechazo del cliente → parado y reenviable a mano; fallo del servidor → reintento; y después, correcto.
- **Ya estaba, con su prueba:**
  - aceptación parcial con un rechazado por línea, que no se reenvía solo: `sends_pending_and_records_results`;
  - lectura de respuestas y errores: `reads_aeat_reply_and_faults`;
  - envío desde un solo ordenador: `only_one_computer_sends_to_the_aeat`;
  - la espera entre envíos que pide la AEAT, en el envío automático (`screens::verifactu::tick`).
- **Sin probar con la AEAT real:** el entorno de pruebas de la AEAT con el certificado de la clínica sigue en B-06 (T-F3-06X).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 418 pasan, 0 fallan, 10 ignoradas |

## E-028 T-F3-07: WhatsApp, correo y SMS contra dobles de prueba
- **WhatsApp y correo:** ya estaban, con sus dobles de prueba (área 6, E-012).
  - **Correo:**
    - `mail::tests::a_cut_after_the_data_is_uncertain_and_a_rejection_is_not`
    - `comms::tests::emails_are_claimed_once_and_uncertain_ones_wait_for_a_decision`, que también comprueba que dos puestos no envían el mismo
    - `comms::tests::automatic_emails_are_not_sent_at_night`
  - **WhatsApp:**
    - `whatsapp::tests::full_round_trip_with_fake_meta` (Meta simulada)
    - `gateway_round_trip_with_fake_waha_and_evolution`
    - `network_errors_tell_not_sent_from_uncertain`
    - `an_uncertain_send_is_not_retried_and_replies_still_count`
    - `replying_baja_stops_whatsapp`
- **SMS: no había pasarela** (se copiaba el texto y se marcaba como enviado). Ahora hay una (`src/sms.rs`).
  - **Formato:** genérico por HTTP: JSON por POST con `Authorization: Bearer` e `Idempotency-Key`, sin suponer el formato de ningún proveedor.
  - **Número:** en formato internacional.
  - **Solo cifrada:** https, salvo en este mismo ordenador.
  - **Resultado,** clasificado como el de un correo:
    - no enviado (se puede reintentar): sin conexión, 4xx o 503;
    - incierto: corte tras enviarlo, 500, 502 o 504. Espera al «¿Llegó?» antes de repetirse (`Db::sms_outcome`).
  - **Dónde se guarda:** la dirección y la clave en este ordenador (`sms.json`, 0600), no en los datos de la clínica.
  - **En la pantalla:** se envía en segundo plano desde Comunicaciones y se configura en Configuración › WhatsApp. Sin pasarela, sigue la copia manual.
  - **Pruebas con una pasarela simulada explícita:**
    - `sms::tests::each_outcome_is_classified_and_a_retry_never_sends_twice`: envío, rechazo 400, corte tras recibir (incierto), reintento con la misma clave sin segundo SMS, 502 incierto, 503 no enviado, sin conexión, sin https y número fijo;
    - `sms::tests::the_outcome_is_recorded_on_the_message`;
    - `sms::tests::numbers_go_out_in_international_format`.
- **Sin probar con servicios reales:** la cuenta de WhatsApp Business y sus plantillas aprobadas (B-07, T-F3-07X) y el contrato con un proveedor de SMS. No se ha contratado nada.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 421 pasan, 0 fallan, 10 ignoradas |

## E-029 T-F3-09: instrumentos del piloto
- **Qué hay:** `src/medidas.rs`, y en los datos la lista `measures`, que se sincroniza entre puestos y se vacía al empezar con datos propios.
  - **Tareas:** dar cita, cobrar y cerrar la caja, con tiempo y pasos (clics y teclas).
    - Empiezan al abrir el diálogo de cita nueva, el de cobro o la pantalla de caja, y terminan al guardar.
    - Si se cierra sin guardar, no cuentan.
  - **Incidencias:** los errores del servidor y las que se apuntan a mano. Las notas se minimizan como el registro de errores, nombres de los datos incluidos.
  - **Disponibilidad:** caídas de la conexión con el servidor y su duración. Se apuntan en cuanto vuelven a estar los datos.
  - **Restauraciones y ensayos de restauración:** con su resultado.
  - **Resumen de 30 días** (Configuración › Registro de cambios › Medidas del piloto): medianas de tiempo y pasos por tarea, incidencias, minutos sin conexión y porcentaje de disponibilidad, y restauraciones bien y con fallos.
  - **Exportación** en CSV con `;`.
- **Sin contenido clínico:** no se apunta el paciente, el tratamiento ni el importe.
- **Pruebas:**
  - `medidas::tests::the_summary_gives_medians_availability_and_restorations`, con valores calculados a mano (99,17 % con 6 minutos de caída en 12 horas);
  - `medidas::tests::measures_carry_no_patient_data`: ni el nombre ni el NIF del paciente en el CSV, y ningún campo de paciente o importe;
  - `ui_tests::giving_an_appointment_is_measured_for_the_pilot`: una cita dada desde la agenda queda medida, y abrir y cancelar no cuenta.
- **Pendiente fuera de aquí:** el mes de piloto real sigue en B-08 (T-F3-10). Estos son sus instrumentos, no su resultado.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 424 pasan, 0 fallan, 10 ignoradas |

## E-030 T-F3-08: guías comprobadas contra el programa, y fórmulas en los CSV
- **T-F3-08.** Tres guías, comprobadas por una prueba que falla si un botón cambia de nombre:
  - `docs/instalacion.md`: lista de instalación del servidor y los puestos, con disco cifrado, huella, permisos, copias con clave aparte y ensayo, puesta en marcha, migración, VERI*FACTU y comprobación final. Avisa de que los instaladores aún no van firmados, sin desactivar protecciones.
  - `docs/restauracion.md`: restaurar una copia (con contraseña o clave de recuperación), ensayo mensual, pérdida de la contraseña, y actualizar y volver atrás (orden, paquete firmado, apagón a mitad, `--volver-atras` y cuándo hay que restaurar).
  - `docs/guia_breve.md`: recepción (cita, llegadas, cobro, caja, mensajes, sin conexión), profesionales y qué hacer si algo va mal.
  - **Prueba:** `docs_tests::tests::the_guides_only_name_what_the_program_has`. Cada texto entre «» de las guías (más de 40 citas) está en el código, y cada opción `--…` en `main.rs`.
- **Revisión de seguridad automática del commit `445396c`: un hallazgo, corregido.**
  - **Hallazgo:** inyección de fórmulas en el CSV de medidas. El mismo problema tenían dos exportaciones anteriores a este trabajo (la liquidación a mutuas y los gastos) y el informe de migración que se pega en una hoja de cálculo.
  - **Ahora:** todas pasan sus textos por `model::sheet_cell`: sin separadores ni saltos de línea, y con un apóstrofo delante de lo que empieza por `=`, `+`, `-`, `@` o un tabulador.
  - **Pruebas:** `medidas::tests::measures_carry_no_patient_data` y `migracion::tests::the_report_reads_as_text`, con un nombre de paciente «=HIPERVINCULO(…)» en el origen.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 425 pasan, 0 fallan, 10 ignoradas |

## E-031 T-F4-00: registro de normativa consultada
- **Dónde:** `docs/normativa.md`, consultado el 13/11/2026 en fuentes oficiales, con fuente, fecha, versión, contenido, efecto en el código y lo pendiente.
  - **VERI*FACTU (calendario):** RDL 15/2025, BOE-A-2025-24446; 1/1/2027 para el IS y 1/7/2027 para los demás.
  - **Historia clínica:** Ley 41/2002, artículo 16 (acceso) y 17 (cinco años como mínimo, más según la comunidad).
  - **Registro de jornada:** ET artículo 34.9 y guía del Ministerio. El desarrollo digital no consta aprobado. La conservación de cuatro años está marcada para confirmar con la lectura literal.
  - **Libros registro de IVA e IRPF:** formato común de la AEAT, versión del 18/02/2026. Para A3, ContaSol y Sage no hay especificación oficial pública: B-11.
  - **TicketBAI:** NF 3/2020, DF 32/2020 y DF 48/2021. Si alcanza a los servicios sanitarios exentos no está confirmado, y Navarra está pendiente.
  - **Receta privada:** SREP, CGCOE, CGCOM y Nodofarma.
- **Lo que no se afirma:** ninguna certificación ni ningún cumplimiento. Lo confirmado y lo pendiente van separados.

## E-032 T-F4-02: registro de accesos a cada historia
- **Qué hay:** `src/accesos.rs`. Apunta quién consulta y quién modifica la historia de cada paciente: cuándo, desde qué puesto, con qué usuario y en qué parte (pestaña de la ficha, o tratamientos, ficha, recetas…).
- **Nunca copia la historia:** solo el nombre de la parte. Lo que cuenta como historia está en `HISTORY_LISTS`; citas y gastos no, por ejemplo.
- **Protegido y limitado:**
  - va en un fichero aparte (`accesos_historias.jsonl`, 0600), fuera de los datos de la clínica, así que los puestos no lo reciben;
  - cada línea va encadenada con la huella de la anterior: borrar o cambiar una línea se detecta (`verify`), y Dirección ve si está íntegro;
  - solo Dirección lo lee: en la ficha, «Accesos a esta historia»; en red, `GET /historia`, que rechaza a los demás.
  - Las consultas se apuntan una vez cada cinco minutos por usuario, paciente y parte, para no llenarlo.
- **En red lo registra el servidor:**
  - la consulta llega por la sesión del puesto (`POST /historia`) y el servidor la apunta con el usuario de la sesión, aunque el puesto diga otro;
  - las modificaciones las apunta el servidor al recibir los cambios (`State::save`).
- **En un solo ordenador:** lo apunta el programa, con las historias tocadas en cada guardado (`LocalStore::touched`).
- **Pruebas:**
  - `accesos::tests::the_log_is_chained_and_tampering_shows`: cadena, 0600, y borrado o cambio detectados;
  - `accesos::tests::changes_tell_which_patient_and_part_without_content`;
  - `server::tests::the_server_records_who_reads_and_changes_each_history`: usuario de la sesión, consulta sin duplicar, modificación con su parte, Recepción sin acceso al registro, la nota clínica fuera del registro y el registro fuera de `/datos`;
  - `local::tests::each_save_tells_which_histories_it_touched`.
- **Límites:**
  - El modo de carpeta compartida no apunta accesos: se retira en T-F4-05.
  - El paso de la interfaz en un solo ordenador (al abrir la ficha) no tiene prueba de interfaz propia: las pruebas de interfaz comparten el bloqueo de los datos. Sí la tienen sus piezas (`LocalStore::touched`, `accesos::append`) y la vía en red.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 429 pasan, 0 fallan, 10 ignoradas |

## E-033 T-F4-03: derechos de protección de datos, y revisión de seguridad del registro de accesos
- **Revisión de seguridad automática del commit `e1d4ce0`: cuatro hallazgos.**
  - **Corregidos:**
    1. **Paciente de una modificación:** el servidor lo saca de sus propios datos (lo que había y lo que queda), no de lo que diga el puesto. Un «antes» falso que nombra a otro paciente no se le atribuye.
    2. **Puesto:** el de una consulta sale de la sesión, no de la cabecera `X-Puesto`.
    3. **Última línea del registro:** se lee como bytes. Si no se puede leer, no se apunta nada, en lugar de empezar otra cadena en silencio. `verify` dice todas las líneas rotas.
  - **Corregido en parte:**
    4. **Carpeta compartida:** en ese modo se apuntan las consultas.
       - **Límite declarado:** las modificaciones no, porque ese modo se retira en T-F4-05.
  - **Pruebas:** ampliadas `server::tests::the_server_records_who_reads_and_changes_each_history` (puesto que miente y «antes» falso) y `accesos::tests::the_log_is_chained_and_tampering_shows` (todas las líneas rotas, y nada apuntado tras una línea dañada).
- **T-F4-03 (`src/rgpd.rs`, ficha › Filiación › Protección de datos):**
  - **Solicitudes de derechos:** acceso, rectificación, supresión, limitación, oposición y portabilidad.
    - Plazo de un mes (RGPD art. 12.3) y aviso de las que están fuera de plazo.
    - Quién lo pide y en calidad de qué. Un menor de 14 años, por su representante (LOPDGDD art. 7). Representante o autorizado, con el documento que lo acredita.
    - Queda en el registro de cambios.
  - **Acceso y portabilidad:** ZIP (0600) con los datos (también ortodoncia, análisis con IA y solicitudes), los ficheros que están en el ordenador y un manifiesto con el SHA-256 de cada uno, quién y cuándo.
  - **Limitación:** no se le envían mensajes.
  - **Bloqueo:** sale de la lista de pacientes (Dirección lo sigue viendo) y solo Dirección lo quita.
  - **Supresión: desactivada** (`SUPPRESSION_ENABLED = false`) hasta la revisión externa (B-10, T-F4-03X). El botón explica por qué:
    - falta la solicitud;
    - la conservación mínima de la historia, cinco años desde el último proceso (Ley 41/2002) o más según la comunidad;
    - las facturas, que se conservan por obligación legal.
  - **Rectificación:** en la ficha, con la solicitud registrada.
- **Pruebas con datos ficticios:**
  - `rgpd::tests::requests_have_a_deadline_and_check_who_asks`
  - `rgpd::tests::restriction_stops_messages_and_only_management_unblocks`
  - `rgpd::tests::suppression_is_disabled_and_explains_what_must_be_kept`
  - `rgpd::tests::the_export_is_complete_traceable_and_private`
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 433 pasan, 0 fallan, 10 ignoradas |

## E-034 T-F4-05: retirada de la carpeta compartida, y revisión de seguridad de los derechos RGPD
- **Revisión de seguridad automática del commit `1141e02`: dos hallazgos, corregidos.**
  1. **Desbloqueo solo en el puesto.**
     - **Hallazgo:** que solo Dirección desbloquee se comprobaba únicamente en el puesto.
     - **Ahora:** el servidor conserva el bloqueo si lo pone o lo quita otro perfil, sin impedir el resto del cambio.
     - **Prueba:** `server::tests::only_management_blocks_or_unblocks_a_patient_on_the_server`.
  2. **Exportación de cualquier fichero.**
     - **Hallazgo:** la exportación podía meter en el ZIP cualquier fichero del ordenador mediante la ruta de una «imagen», como una clave privada.
     - **Ahora:** solo entran ficheros normales (no enlaces) de imagen o documento, de 200 MB como mucho, fuera de la configuración y de las copias del programa. Los demás se listan como no disponibles.
     - **Prueba:** ampliada `rgpd::tests::the_export_is_complete_traceable_and_private`, con una clave y un enlace simbólico que no salen.
- **T-F4-05: carpeta compartida.**
  - **Puestos nuevos:** ya no se puede configurar en uno nuevo. Trabajo en red explica por qué (reparte el fichero entero, con contraseñas y claves) y propone el servidor (`shared::NO_NEW_SHARED`).
  - **Puestos que ya la usan:** siguen funcionando, con aviso, y «Pasar los datos al servidor» (`shared::move_to_server`). Este paso, con la carpeta bloqueada:
    1. copia los datos a este ordenador; lo que hubiera aquí se aparta con otro nombre;
    2. comprueba que son iguales;
    3. deja la carpeta intacta: se conserva hasta que se autorice retirarla;
    4. deja la marca `MIGRADO_AL_SERVIDOR.txt`, y desde entonces ningún puesto guarda en la carpeta: se le pide que se conecte al servidor.
  - **Puesto nuevo en red:** se conecta al servidor y no recibe contraseñas ni códigos del móvil. Lo comprueban pruebas ya existentes: `server::tests::only_direction_changes_users_and_secrets_stay_on_the_server` y las del acceso.
  - **Pruebas:**
    - `shared::tests::a_shared_folder_moves_to_the_server_and_stays_untouched`: datos iguales, lo de aquí apartado, carpeta intacta, puesto rechazado tras el paso y no se repite;
    - `ui_tests::a_new_station_cannot_set_up_a_shared_folder`.
  - **Pendiente:** las claves de los servicios (correo, IA) siguen llegando a los puestos en red, porque los puestos envían. Moverlas al servidor es otro cambio, que queda anotado aquí.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 436 pasan, 0 fallan, 10 ignoradas |

## E-035 T-F4-06: registro de jornada, y revisión de seguridad del bloqueo
- **Revisión de seguridad automática del commit `952fa58`: un hallazgo, corregido.**
  - **Hallazgo:** otro perfil podía borrar en red un paciente con los datos bloqueados.
  - **Ahora:** el servidor descarta ese borrado.
  - **Prueba:** final de `server::tests::the_time_record_only_grows_on_the_server`.
- **T-F4-06 (`src/jornada.rs`):**
  - **Fichar:** en Inicio cada persona ficha lo suyo (entrada, inicio y fin de pausa, salida), en orden y a la hora del momento.
  - **Resumen por día:** primera entrada, última salida, pausas y minutos trabajados, con aviso si el día está incompleto.
  - **Correcciones (Configuración › Usuarios y permisos › Registro de jornada):**
    - solo Dirección y siempre con motivo;
    - sustituyen, anulan o añaden una marca que faltaba;
    - la original se conserva y queda en el registro de cambios.
  - **Acceso por rol:** cada persona ve lo suyo y Dirección todo. En red lo aplica el servidor: a quien no es Dirección solo le envía sus fichajes (`View::only_own`).
  - **Exportación** en CSV por periodo (una fila por persona y día, y las correcciones con su motivo), con las celdas protegidas contra fórmulas.
  - **En red, el servidor hace cumplir:**
    - solo se añade (no se cambia ni se borra una marca);
    - cada uno ficha lo suyo, y la hora la pone el servidor, no el ordenador del puesto;
    - las correcciones son solo de Dirección.
- **Normativa:** ET art. 34.9 y guía del Ministerio, en `docs/normativa.md`.
  - El desarrollo reglamentario digital no consta aprobado a esta fecha.
  - La conservación de cuatro años se ha marcado para confirmarla con la lectura literal.
  - El registro no tiene ninguna función de borrado.
- **Pruebas:**
  - `jornada::tests::a_day_with_a_pause_adds_up_and_the_order_is_kept`
  - `jornada::tests::corrections_keep_the_original_and_say_why`
  - `server::tests::the_time_record_only_grows_on_the_server`: hora del servidor y no la del puesto, nada por otra persona, ni correcciones sin ser Dirección, ni cambios ni borrados, la corrección de Dirección, y cada uno recibe solo lo suyo.
- **Pendiente:** la revisión de la asesoría laboral (B-26, T-F4-06X). Esta pantalla no demuestra el cumplimiento.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 439 pasan, 0 fallan, 10 ignoradas |

## E-036 Revisión de seguridad del registro de jornada en red
- **Revisión de seguridad automática del commit `e7d3b06`: dos hallazgos, corregidos.**
  1. **Orden de los fichajes.**
     - **Hallazgo:** el servidor no comprobaba el orden: una segunda entrada seguida, o una salida sin entrada, enviadas desde el puesto.
     - **Ahora:** la comprueba con lo que ya tiene de esa persona (`jornada::next_allowed`), y pone el puesto de la sesión.
  2. **Autor de la corrección.**
     - **Hallazgo:** quién corrige lo ponía el puesto.
     - **Ahora:** lo pone el servidor con el usuario de la sesión. Una corrección sin motivo se rechaza.
  - **Prueba:** ampliada `server::tests::the_time_record_only_grows_on_the_server` (puesto de la sesión, segunda entrada rechazada y autor de la corrección).
- **Batería:** fmt 0, clippy 0; `cargo test --workspace --all-targets --locked` con código 0, 439 pasan, 0 fallan y 10 ignoradas.

## E-037 T-F4-07: libros registro en el formato común de la AEAT
- **Fuentes oficiales** (en `docs/normativa.md`):
  - el PDF «Formatos de los Libros Registro de IVA e IRPF» (V.18.02.2026): nombre del fichero, hojas, XLSX de 4 MB como mucho y un libro por año;
  - los diseños `LSI.xlsx` (01-01-2026), con columnas, tipos, longitudes y validaciones numeradas;
  - la plantilla `PLANTILLA_LIBROS_UNIFICADOS.xlsx` (09-09-2026), con la cabecera exacta en las filas 1 a 3, los datos desde la 4 y las listas de claves.
  - El PDF se convirtió con `pdftotext` y los XLSX se leyeron como datos.
- **Qué hay:** `src/libros.rs`.
  - **Nombre:** ejercicio + NIF + «T» (persona física, libros unificados de IVA e IRPF) o «C» (sociedad, libros del IVA) + nombre.
  - **Hojas:** EXPEDIDAS_INGRESOS y RECIBIDAS_GASTOS (o EXPEDIDAS y RECIBIDAS), con la cabecera copiada tal cual de la plantilla, incluidas sus erratas («Refrencia Externa»).
  - **Facturas emitidas** (sin las anuladas):
    - actividad A y tipo 05 (o 03 en una sociedad), con el epígrafe del IAE configurado;
    - el tipo de factura de VERI*FACTU (F1, F2, R1, R5);
    - I01, clave 01, exenta E1, total = base;
    - el NIF y el nombre solo si no es simplificada.
  - **Gastos:**
    - concepto G por categoría (propuesta: Material G01, Laboratorio G03, Alquiler G12, Nóminas G04, Suministros GY4, Seguros G20, Marketing G22, Mantenimiento G13, Impuestos G26, Otros G37);
    - número de recepción correlativo y NIF del proveedor de Almacén;
    - total = base + cuota, y cuota deducible 0 (actividad exenta).
  - **Antes de entregarlo** se comprueban las validaciones 2, 4, 5, 11, 32, 33 y 37 del diseño y el límite de 4 MB, y se dice lo que falta: NIF de proveedores, número de factura y epígrafe.
  - **Dónde:** Configuración › Facturación y cobros › Libros registro.
- **Pruebas:**
  - `libros::tests::columns_and_codes`
  - `libros::tests::the_books_follow_the_official_layout_and_validations`: se relee con calamine; nombres de hoja, las tres filas de cabecera idénticas a la plantilla, datos desde la fila 4, claves y totales, y fallos de las validaciones 4, 11, 32 y 37 detectados.
  - `libros::tests::a_company_gets_the_vat_books_without_income_tax_columns`
- **Pendiente (B-11):**
  - que la gestoría lo acepte y revise los conceptos de gasto y el IVA no deducible en el IRPF;
  - los formatos propios de A3, ContaSol y Sage, sin especificación oficial pública.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 442 pasan, 0 fallan, 10 ignoradas |

## E-038 T-F4-01: receta electrónica privada, organismos confirmados y expediente
- **Ya estaba (área 2, E-007), con sus pruebas:**
  - firma y repositorio;
  - hoja de información al paciente con DataMatrix y código de barras;
  - trazabilidad;
  - circuito completo con una farmacia simulada: `srep_cert::tests::the_whole_circuit_passes_with_a_simulated_pharmacy` y `srep::tests::pharmacy_lifecycle_follows_the_standard`;
  - servicio por HTTP con comprobación del proxy;
  - expediente de certificación con los 30 controles de la auditoría.
- **Nuevo:** los organismos y el procedimiento, confirmados en las fuentes (`docs/normativa.md`, 13/11/2026), van ahora en el expediente:
  - para un odontólogo certifica el CGCOE, con recertificación por auditoría completa cada dos años;
  - las farmacias validan las recetas a través de Nodofarma contra el repositorio del Consejo.
  - **Prueba:** `srep_cert::tests::the_dossier_has_the_system_the_controls_and_the_tests` (ampliada).
- **Pendiente fuera de aquí:** la certificación y la conexión real con las farmacias (B-09, T-F4-01X).

## E-039 T-F4-04: cifrado de los datos con claves gestionadas
- **Qué hay:** `src/datos_cifrados.rs` cifra con AES-256-GCM (`ring`, sin criptografía propia):
  - el fichero de datos de la clínica (un ordenador y servidor);
  - cada línea del diario de cambios;
  - las copias locales sin contraseña de copias.
- **Claves:**
  - aleatorias de 256 bits, con identificador;
  - guardadas en la configuración (`datos_claves.json`, 0600), no en la carpeta de los datos;
  - se conserva la anterior, así que lo cifrado antes se sigue abriendo.
- **Rotación:** clave nueva y todo reescrito con ella.
- **Recuperación:** la clave se guarda aparte, nunca en la carpeta de los datos (se rechaza), y se recupera en otro ordenador.
- **Integridad:** un byte cambiado y no se abre. Entonces la carga da error, nunca «no hay datos», así que no se guarda encima.
- **Migración recuperable en los dos sentidos** (cifrar y dejar de cifrar):
  1. todo guardado y copia de seguridad previa;
  2. fichero nuevo al lado, comprobado;
  3. claves guardadas;
  4. cambio atómico y diario vacío.
  - Un corte en cualquier punto deja un fichero que se abre: lo cifrado y lo sin cifrar se leen igual.
- **Dónde:** Configuración › Copias de seguridad › Cifrado de los datos de este ordenador.
- **Pruebas:**
  - `datos_cifrados::tests::data_is_encrypted_rotated_and_decrypted_without_loss`: el disco sin nombres de pacientes, el diario cifrado, la rotación leyendo lo de la clave anterior y el descifrado;
  - `datos_cifrados::tests::tampering_is_detected_and_the_recovery_key_opens_the_data`: byte cambiado, otro ordenador sin claves, clave de recuperación y corte a mitad;
  - `datos_cifrados::tests::journal_lines_round_trip_and_mix`.
- **Límite:** las imágenes están en carpetas que comparten los programas de radiología (puentes); cifrarlas allí rompería esos programas. Las protege el cifrado del disco, que se comprueba en la puesta en marcha (`disco`). Queda anotado aquí y en el módulo.
- **No se ha tocado:** ningún dato real; la migración solo se hace a mano desde la pantalla.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 445 pasan, 0 fallan, 10 ignoradas |
- **Prueba existente:** `ui_tests::backup_password_encrypts_the_second_copy` buscaba «sin cifrar», que ahora salía dos veces. Se cambió el texto nuevo, no la prueba.

## E-040 Revisión de seguridad del cifrado de los datos (vuelta atrás a sin cifrar)
- **Hallazgo (revisión automática del commit de T-F4-04):** con el cifrado puesto, se aceptaban un fichero de datos o líneas del diario sin cifrar. Quien pudiera escribir en la carpeta de los datos podía poner otros sin cifrar en su lugar y saltarse la prueba de integridad.
- **Corrección (`src/datos_cifrados.rs`, `src/sync.rs`, `src/model.rs`, `src/tools.rs`):**
  - con el cifrado puesto, `from_disk` rechaza lo que no está cifrado; la carga da error y no se guarda encima;
  - `line_from_disk` distingue tres casos: línea ilegible (`Ok(None)`), línea sin cifrar con el cifrado puesto (`Err`) y línea buena;
  - las copias de seguridad de antes de cifrar se siguen restaurando a mano (`from_disk_backup`), porque las elige y confirma quien restaura.
- **Diario (`replay_journal`):**
  - una línea ilegible solo se acepta como corte si es la última; antes del final, se avisa en vez de cortar en silencio, así que no se pierde lo de después al reescribir;
  - una revisión menor que la anterior se rechaza («desordenado»): las líneas se escriben siempre en orden;
  - una línea sin cifrar con una revisión nueva se rechaza; con revisión ya incluida en el fichero (de antes de cifrar), se ignora.
- **Cambio de cifrado cortado:** con la nueva regla, un corte entre «guardar las claves» y «poner el fichero» dejaría un fichero sin cifrar con las claves puestas. `finish_conversion`, al abrir y solo quien bloquea los datos, termina el cambio o lo deshace:
  - termina si el fichero nuevo corresponde a las claves guardadas y se abre entero;
  - si no, lo quita y sigue el de antes.
- **Sobre lo de atar la posición de la línea al cifrado:** se ha preferido la comprobación del orden por revisión, que va dentro de lo cifrado y autenticado. Ninguna de las dos impide que alguien con acceso de escritura a la carpeta quite las últimas líneas o el diario entero (volver a un estado anterior). Eso necesitaría un contador guardado fuera de la carpeta y queda anotado como límite. Contra ello están los permisos (0600) y el cifrado del disco.
- **Pruebas:**
  - nueva: `datos_cifrados::tests::plain_data_is_refused_when_encrypted_and_cut_conversions_recover`. Cubre el fichero sin cifrar puesto en su lugar, líneas cambiadas de sitio, una dañada antes del final, la última cortada, una sin cifrar nueva, las de antes de cifrar ya incluidas, y los cortes antes y después de guardar las claves al rotar y al descifrar;
  - cambiadas: `datos_cifrados::tests::journal_lines_round_trip_and_mix` comprueba el rechazo con cifrado y la lectura sin él. `server::tests::a_cut_or_damaged_journal_never_applies_changes_out_of_order` sigue comprobando que no se aplica nada después de la línea dañada y ahora exige además el aviso.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 446 pasan, 0 fallan, 10 ignoradas |

## E-041 T-F4-08: facturación foral (arquitectura y TicketBAI), sin activar
- **Fuente:** «Especificaciones funcionales y técnicas del sistema TicketBAI 1.0» (19/12/2019), apartados 4.3.2 y 4.3.3 y anexo 3. Se consultó el 14/11/2026.
- **Hecho (`src/foral.rs`):**
  - se elige el territorio: común, Araba, Bizkaia, Gipuzkoa o Navarra (Navarra, pendiente: su sistema no se ha consultado);
  - sin confirmación de la gestoría (fecha y quién) no cambia nada;
  - con un territorio foral confirmado, VERI*FACTU no aplica: `VfConfig::active`, que comprueba también `send_pending`, y el QR VERI*FACTU deja de imprimirse;
  - `tbai`: identificativo TBAI con CRC-8 y URL del QR de cada hacienda, comprobados con los ejemplos de la especificación (CRC «237», QR `cr=007`).
- **Por separado, y pendiente (B-12):**
  - el fichero TBAI (XML firmado con XAdES) y su envío;
  - la homologación en el registro de software de cada hacienda (`HOMOLOGATED = false`);
  - la activación real. La puesta en marcha lo muestra como pendiente.
- **Interfaz:** Configuración › Facturación › Territorio fiscal.
- **Pruebas:**
  - `foral::tbai::tests::the_examples_of_the_specification_come_out_exactly`;
  - `foral::tests::nothing_is_activated_without_confirmation_and_homologation`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 448 pasan, 0 fallan, 10 ignoradas |

## E-042 T-F4-09: pruebas preparadas para la gestoría, el DPO y la auditoría de seguridad
- **Hecho (`docs/revision_externa`):**
  - `README.md`: qué es y qué no es (ni aprobación ni certificación), cómo comprobar lo citado y cómo devolver la revisión;
  - `gestoria.md`: exención de IVA (B-01), VERI*FACTU (B-06), libros registro y exportación contable (B-11) y territorio foral (B-12). En cada punto, lo hecho, dónde está, sus pruebas y lo que decide la gestoría;
  - `dpo.md`:
    - qué datos se guardan y dónde, cifrado y copias;
    - acceso a la historia y registro de accesos;
    - derechos del paciente (supresión desactivada, B-10);
    - lo que sale de la clínica a cada proveedor;
    - minimización en los registros y registro de jornada;
  - `auditoria_seguridad.md`: arquitectura, autenticación y permisos, inventario criptográfico, robustez, hallazgos propios ya corregidos (con su evidencia), límites conocidos y cómo reproducir.
- **Comprobado:** `docs_tests::tests::the_review_package_only_cites_what_exists` exige que todo lo citado exista: cada fichero `src/…`, cada prueba (por su nombre), cada evidencia y cada bloqueo. En su primera ejecución encontró dos citas que no existían, y se corrigieron.
- **`docs/normativa.md`:** la entrada de TicketBAI recoge la especificación técnica consultada y su efecto en el código.
- **Pendiente:** las revisiones mismas (T-F4-09X, B-13). Ninguna se presenta como hecha.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 449 pasan, 0 fallan, 10 ignoradas |

## E-043 T-F5-01: cuadro de mando con fórmulas definidas y verificación independiente
- **Hecho (`src/cuadro.rs`, Informes › Cuadro de mando):** cada indicador con su fórmula, su denominador y lo que no entra (detallado en el módulo). Todo se cuenta en enteros (minutos, citas, céntimos); sin denominador se muestra «—», no un 0 %.
  - **Ocupación de cada profesional:** minutos citados ÷ minutos de su horario. Del horario se quitan festivos y bloqueos, y los bloqueos solapados cuentan una vez. Las citas anuladas y reprogramadas no cuentan; las de «no acude» sí, porque el hueco estaba reservado.
  - **Ocupación de cada gabinete:** minutos citados ÷ minutos que abre la clínica.
  - **Ausencias:** «no acude» ÷ (finalizadas + «no acude»). Las citas sin cerrar se cuentan aparte.
  - **Aceptación de presupuestos:** en número y en importe, aceptados o cerrados ÷ presentados.
  - **Producción por hora de sillón:** producción del informe ÷ horas de citas finalizadas, en total y por profesional.
  - **Recalls citados:** citados ÷ recalls con fecha prevista en el periodo.
  - **Filtros:** los de los informes. Se dice qué indicador no es de un centro (los profesionales, los presupuestos y los recalls).
  - **Comparación** con el periodo anterior de la misma duración, y **objetivos** de la clínica, cada uno opcional (`ClinicConfig::kpi_goals`).
- **Verificación independiente:** una semana sintética (16 a 22/11/2026) con un festivo, dos bloqueos solapados, dos profesionales con horarios distintos, citas en todos los estados, actos dentro y fuera del periodo, presupuestos y recalls. Los valores esperados están calculados a mano en el comentario de la prueba, no con el código:
  - `cuadro::tests::each_indicator_matches_the_operations` (por ejemplo, 225 ÷ 1110 minutos, 1 ÷ 5 ausencias, 230,50 € en 210 minutos);
  - `cuadro::tests::filters_comparison_and_goals`;
  - `cuadro::tests::overlapping_blocks_are_not_counted_twice`.
- **Pantalla:**
  - los indicadores van en una rejilla de etiquetas, no pintados, para que se lean con un lector de pantalla;
  - `ui_tests::the_dashboard_shows_each_indicator_with_its_formula` comprueba la fórmula que se ve con los valores de `cuadro` y un objetivo marcado;
  - `ui_tests::every_screen_fits_1280_wide` incluye la pestaña nueva;
  - captura: `docs/autonomia/capturas/informes_cuadro_de_mando.png`, con los datos de ejemplo.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 453 pasan, 0 fallan, 10 ignoradas |

## E-044 T-F5-02: pago por internet (TPV virtual de Redsys y Bizum), sin cargos
- **Fuente:** documentación de desarrolladores de Redsys, consultada el 14/11/2026. Recogida en `docs/normativa.md`.
- **Firma `HMAC_SHA512_V2` (`src/pago.rs`):** da exactamente la de los dos ejemplos oficiales, comprobada antes con openssl y Python y después en Rust. El AES-CBC es de los crates `aes` y `cbc` (RustCrypto), que ya estaban en `Cargo.lock` por `p12-keystore`; el HMAC, de `ring`. No hay criptografía propia.
- **Flujo:**
  1. La clínica pide el pago desde el presupuesto, la próxima cuota o la cita (señal), y el enlace queda copiado.
  2. El paciente abre `/pagar/{código}`, elige tarjeta o Bizum y va a la página del banco con un formulario firmado. El importe es el de la petición, no el del navegador.
  3. Redsys avisa en `/api/tpv/notificacion`. Solo cuenta con la firma bien, del comercio y terminal configurados, en euros y por el importe pedido.
- **Una sola vez:** una notificación repetida no cobra dos veces. Un segundo pago real (otro pedido) se apunta y queda para devolver. Un importe distinto no se apunta solo.
- **Incierto:** un intento sin notificación pasados 30 minutos se resuelve a mano («Se cobró» / «No se cobró») tras comprobarlo en el portal del TPV.
- **Devoluciones:** petición REST firmada (tipo 3). Si Redsys corta tras enviarla, queda incierta y no se repite sin comprobarla.
- **Señal de reserva:** política configurable que ve el paciente antes de pagar.
  - Si el paciente anula con la antelación fijada, se le devuelve; con menos, o si no acude, se conserva.
  - Si la anula la clínica, siempre se devuelve.
  - Se aplica al anular desde «Mi cita» y desde la agenda.
- **Clave del terminal:** solo en el ordenador que publica la página (`tpv.json`, 0600), nunca en los datos de la clínica.
- **Pruebas** (sin cargos y sin datos de tarjeta, con la clave pública de pruebas de Redsys y un Redsys simulado):
  - `pago::tests::the_signature_matches_the_redsys_examples`;
  - `pago::tests::the_pay_form_and_notices_are_signed_and_checked`: otra clave, otro comercio, otra moneda y la versión antigua se rechazan;
  - `pago::tests::a_notice_is_charged_once_and_a_second_payment_is_flagged`;
  - `pago::tests::an_attempt_without_notice_becomes_uncertain_and_is_settled_by_hand`;
  - `pago::tests::installments_and_deposits_follow_their_rules`;
  - `pago::tests::refunds_are_signed_recorded_once_and_uncertain_ones_are_not_repeated`: hecha, rechazada, cortada y sin conexión;
  - `booking::tests::online_payment_from_the_page_to_the_signed_notice`: de la página a la notificación por HTTP, con una notificación falsificada;
  - `ui_tests::an_online_payment_is_requested_from_the_budget_and_settled_in_payments`.
- **Pendiente (B-14, T-F5-02X):** el contrato del TPV y la prueba en el entorno de pruebas del banco.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 461 pasan, 0 fallan, 10 ignoradas |

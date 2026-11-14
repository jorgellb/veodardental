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

## E-045 T-F5-03: plan de tratamiento por fases y alternativas, del odontograma al presupuesto
- **Flujo (`src/plan.rs`, pantalla del presupuesto):**
  1. Los previstos del odontograma pasan a un presupuesto, que es la alternativa A del plan.
  2. **Fases:** cada línea va en una fase (columna «Fase»), con subtotal y nombre por fase.
  3. **Alternativas:** copias en el mismo plan para cambiarlas («Crear alternativa»).
  4. **Versiones:** «Versión nueva». La anterior se conserva tal cual, también si estaba firmada, y ya no se puede aceptar. Sus previstos pasan a la nueva.
  5. **Aceptación expresa** de una alternativa («Aceptar la alternativa B»), también desde la firma por internet:
     - las demás alternativas del plan quedan descartadas;
     - los previstos comunes se reutilizan y los que solo eran de las descartadas se quitan;
     - no se acepta dos veces, ni una versión sustituida, ni otra alternativa de un plan ya aceptado.
  6. **Consentimientos:** al aceptar se crean, para firmar, los que piden sus tratamientos (plantilla con clase de tratamiento). Los firmados no se tocan y no se duplican.
- **Sumas exactas:** en céntimos. Las fases suman exactamente el total, comprobado también con 200 presupuestos al azar, y el total exacto coincide con el que se imprime.
- **Cuadro de mando:** las versiones sustituidas y las alternativas descartadas no cuentan como presupuestos presentados.
- **Versión de los datos 2** (D-23), con su paso de actualización probado.
- **Pruebas:**
  - `plan::tests::phases_add_up_exactly_to_the_total`;
  - `plan::tests::choosing_an_alternative_discards_the_others_without_duplicates`;
  - `plan::tests::a_new_version_keeps_the_old_one_and_only_the_new_one_is_accepted`;
  - `plan::tests::accepting_prepares_the_right_consents_and_leaves_signed_ones_alone`;
  - `plan::tests::old_data_gets_the_consent_kinds_on_upgrade`;
  - `ui_tests::a_treatment_plan_alternative_is_created_and_chosen`;
  - captura: `docs/autonomia/capturas/presupuesto_plan_de_tratamiento.png`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 467 pasan, 0 fallan, 10 ignoradas |

## E-046 T-F5-04: notas de evolución con plantillas, dictado, transcripción y resumen revisado
- **Hecho (`src/evolucion.rs`, `src/screens/evolucion.rs`):** notas en la ficha › Historial.
  - **Plantillas por tratamiento:** se proponen primero las de los tratamientos de hoy, con las piezas puestas y huecos «___» que hay que completar.
  - **Dictado desde una grabación,** transcrito por un servicio compatible con la API de OpenAI (Configuración › IA, fichero privado `stt.json`).
  - **Resumen opcional con la IA,** desactivado de serie. Se envía sin el nombre del paciente; lo que sale es una propuesta y se señala lo que no está en el dictado (piezas, cifras, dosis y medicamentos del vademécum).
- **Nada entra en la historia sin confirmar:**
  - un borrador con huecos no se confirma;
  - con dictado o IA hay que marcar que se ha revisado;
  - con datos de la IA que no están en el dictado, hay que revisarlos expresamente.
- **Procedencia** en cada nota: escrito, plantilla, dictado (servicio y huella del texto), resumen con IA (modelo), y quién confirmó y cuándo.
- **Mínimo de datos:** el audio no se guarda y la grabación puede borrarse al terminar; del dictado solo queda la huella.
- **Confirmada, no cambia:** se corrige con otra nota que la cita.
  - Protegida en el servidor y en la carpeta compartida.
  - La lista de lo protegido está ahora en una sola constante (`firmado::PROTECTED_LISTS`).
- **Fallo encontrado por la prueba nueva y corregido:** el servidor dejaba borrar una nota confirmada porque su filtro tenía la lista escrita a mano.
- **Otras listas:** las notas son clínicas (Recepción no las recibe) y entran en el registro de accesos. Salen en la exportación del paciente, junto con los pagos por internet. Al fusionar pacientes pasan a la ficha que se conserva, también las peticiones de pago.
- **Pruebas:**
  - `evolucion::tests::nothing_enters_the_history_without_review_and_confirmation`;
  - `evolucion::tests::an_ai_summary_with_facts_not_in_the_dictation_is_flagged`;
  - `evolucion::tests::dictation_is_transcribed_by_the_configured_service`: doble de prueba que comprueba el multipart, el modelo y la clave;
  - `server::tests::a_confirmed_evolution_note_cannot_be_changed_by_a_station`;
  - `ui_tests::an_evolution_note_is_written_from_a_template_and_confirmed`.
- **Pendiente (B-15, T-F5-04X):** el proveedor o modelo real y la grabación desde el micrófono dentro del programa.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 472 pasan, 0 fallan, 10 ignoradas |

## E-047 Revisión de seguridad de las notas de evolución y de la fusión de pacientes
- **Hallazgo 1 (suplantación):** un puesto podía mandar una nota ya «confirmada» con cualquier nombre y hora de confirmación.
  - **Corrección (`src/server.rs`, `evolucion::EvoNote::stamp`):** al recibir una nota que pasa a confirmada, el servidor la sella él: el usuario de la sesión (activo y con permiso clínico), su hora y la huella del texto.
  - Sin ese permiso, la nota se queda en borrador.
- **Hallazgo 2 (pérdida de datos al fusionar):** la fusión de pacientes en el servidor pasaba las notas de evolución y las peticiones de pago a la ficha que se conserva, pero esas listas no estaban entre las que la fusión guarda (`ChainOp::needs`), así que el cambio no llegaba al disco. Tampoco estaban las solicitudes de derechos, que faltaban desde T-F4-03.
  - **Corrección:** `rights_requests`, `pay_requests` y `evo_notes` se añaden a la lista de la fusión.
- **Pruebas:** `server::tests::a_confirmed_evolution_note_cannot_be_changed_by_a_station` comprueba ahora dos cosas más:
  - el nombre que manda el puesto se sustituye por el de la sesión;
  - tras la fusión hecha por Dirección, la nota y la petición de pago están en la ficha que se conserva. Se comprobó que esta parte falla sin la corrección.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 475 pasan, 0 fallan, 10 ignoradas |

## E-048 T-F5-05: periodontograma por voz con corrección y confirmación visibles
- **Hecho:** `src/perio_voz.rs` y el recuadro «Sondaje por voz» del periodontograma.
- **El dictado:** escrito, con el dictado del teclado del móvil o transcrito de una grabación con el servicio de transcripción de T-F5-04.
- **Lo que se entiende:** pieza (FDI, en palabras o cifras), lado, sondaje de tres puntos, recesión, sangrado y placa por puntos, movilidad, furca y ausente. Las muletillas se ignoran.
- **Lo dudoso se señala y no se aplica:**
  - un sondaje de menos o de más de tres valores;
  - una profundidad de más de 12 mm;
  - una palabra que no se entiende en medio de las cifras;
  - una pieza que no existe;
  - datos sin pieza o sin lado;
  - el mismo dato dictado dos veces con valores distintos.
- **Cifras sueltas:** tras una pieza que no existe, nada se atribuye a la anterior. Unas cifras sueltas no pasan por una pieza: cifra a cifra solo vale tras «pieza».
- **Revisión:** los cambios se ven uno a uno («Pieza 16 · Sondaje V mesial: 3 → 4»), sobre una copia. La ficha solo cambia al confirmar, y entonces se aplica solo lo seguro, con apunte en la auditoría.
- **Pruebas:**
  - `perio_voz::tests::a_clean_dictation_is_understood`;
  - `perio_voz::tests::noise_and_transcription_errors_never_slip_in`: muletillas, «tres» mal transcrito como «tren», cifras de más, profundidad imposible, pieza inexistente, datos sin pieza o sin lado, contradicciones, y 500 dictados al azar en los que nunca entra un valor fuera de rango;
  - `perio_voz::tests::changes_are_shown_before_and_only_the_copy_changes`;
  - `ui_tests::a_dictated_probing_is_reviewed_and_only_the_safe_part_applied`.
- **Pendiente (B-15):** la grabación desde el micrófono dentro del programa y el servicio de transcripción real.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 476 pasan, 0 fallan, 10 ignoradas |

## E-049 T-F5-06: visor DICOM/CBCT y STL, y comparación de radiografías
- **DICOM (`src/dicom.rs`):** lector propio de DICOM Parte 10, sin dependencias nuevas.
  - Lee DICOM sin comprimir, en sintaxis implícita y explícita *little endian*, y salta las secuencias anidadas.
  - Admite 8 o 16 bits, con o sin signo, en escala de grises (MONOCHROME1 y 2) y RGB entrelazado o por planos, con el reescalado y la ventana del fichero.
- **CBCT:** una carpeta de cortes, ordenados por su posición (no por el nombre del fichero), o un fichero multicorte. Hay vistas axial, coronal y sagital con la proporción real (tamaño de píxel y distancia entre cortes), y se saltan los ficheros de la carpeta que no son DICOM.
- **Orientación y escala:**
  - letras de los bordes según `ImageOrientationPatient`;
  - barra de 10 mm;
  - el paciente y la fecha que dice el fichero, con un aviso si el nombre no es el de la ficha.
- **STL (`src/stl.rs`):** binario y ASCII, medidas en mm y vista sombreada girada con profundidad (z-buffer), calculada en segundo plano.
- **Límites:**
  - 512 MB de píxeles y 5 millones de triángulos como mucho;
  - las sintaxis comprimidas (JPEG, JPEG 2000, RLE) se rechazan con un aviso para exportar sin comprimir;
  - un fichero cortado o incoherente da un error, nunca un pánico. Se probó con 3000 DICOM y 2000 STL mutados o al azar.
- **Sin bloquear la pantalla:** la carga y la vista del STL van en un hilo aparte, y la pantalla muestra «Cargando…».
- **Comparación:** dos imágenes del mismo paciente (si no, se niega), la más antigua a la izquierda («Antes» / «Después»), con fecha, tipo, piezas, los días entre una y otra, y el mismo zoom y el mismo giro en las dos.
- **Sin diagnóstico automático:** solo se ve.
- **Muestras:** sintéticas, generadas en las pruebas. No se ha usado ningún estudio real.
- **Pruebas:**
  - `dicom::tests::synthetic_images_are_read_with_scale_window_and_orientation`;
  - `dicom::tests::a_cbct_series_is_ordered_and_resliced_with_its_proportions`;
  - `dicom::tests::invalid_compressed_huge_or_random_files_are_refused_without_panicking`;
  - `stl::tests::binary_and_ascii_models_are_read_and_measured`;
  - `stl::tests::invalid_cut_or_huge_models_are_refused_without_panicking`;
  - `visor3d::tests::the_name_in_the_file_is_checked_against_the_patient`;
  - `ui_tests::dicom_stl_and_comparison_are_shown_in_the_viewer`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 483 pasan, 0 fallan, 10 ignoradas |

## E-050 Revisión de seguridad del visor: memoria de los estudios DICOM y de los STL
- **Hallazgo:** el límite de los DICOM comparaba los bytes de los píxeles del fichero, pero la memoria que se reserva es mayor:
  - 4 bytes por vóxel (`i32`) más el color, así que un estudio de 8 bits dentro del límite podía ocupar cuatro veces más;
  - en una carpeta de cortes, el total se comprobaba después de leer cada corte.
- **Corrección (`src/dicom.rs`):**
  - el límite es de memoria y se comprueba antes de reservar nada (`parse_within`);
  - en una serie, cada corte se lee con lo que queda del presupuesto, y el fichero no se carga si ya lo pasa;
  - el volumen se reserva justo, sin copias de más.
- **STL (`src/stl.rs`):** el fichero tampoco pasa de lo que ocupan 5 millones de triángulos en binario (`MAX_FILE`). Antes se admitía tres veces más, por si era ASCII.
- **Prueba:** `dicom::tests::invalid_compressed_huge_or_random_files_are_refused_without_panicking` comprueba ahora dos casos:
  - un estudio de 400 MB en el fichero y 800 MB en memoria se rechaza por memoria;
  - con un presupuesto menor, se rechaza un corte que antes cabía.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 483 pasan, 0 fallan, 10 ignoradas |

## E-051 T-F5-07: alertas clínicas ampliadas con fuente citada y revisión del profesional
- **Hecho (`src/alertas.rs`, `src/screens/alertas.rs`):** cuatro alertas, cada una con lo que hay que comprobar y su fuente citada, con enlace:
  - profilaxis de endocarditis (AHA 2021);
  - osteonecrosis por antirresortivos (AAOMS 2022);
  - anticoagulantes y antiagregantes (SDCEP 2022);
  - embarazo (criterios de selección de radiografías de la ADA y la FDA, y ficha técnica en CIMA).
  - Ninguna recomienda tratamientos ni receta: la prueba comprueba que no hay dosis ni pautas.
- **Cuándo saltan:** por las alertas del paciente (las del cuestionario de salud) o sus antecedentes escritos, y solo para los tratamientos a los que afectan. Por ejemplo, la anticoagulación afecta a extracciones, implantes, apicectomías e higiene, pero no a una obturación.
- **Revisión del profesional:**
  - una línea por alerta en la ficha (todas las pestañas), con «La he revisado» (queda quién y cuándo, y vale ese día) y el detalle y la fuente al abrirla;
  - hasta revisarla, un tratamiento afectado no se deja como hecho por ninguno de los cuatro caminos: pintar en el odontograma como realizado, ✔ en el odontograma, ✔ en el historial y «Añadir» como realizado.
- **Las de antes siguen:** las alertas del cuestionario y los avisos de las recetas.
- **Pendiente:** los textos los revisa un odontólogo (`alertas::REVIEWED` sigue en falso, B-16, T-F5-07X).
- **Otras listas:** las revisiones son datos clínicos (Recepción no las recibe), están en el registro de accesos y pasan a la ficha que se conserva al fusionar pacientes.
- **Ajuste:** el recuadro se hizo compacto (una línea por alerta) porque, a toda altura, dejaba fuera de la ventana botones que usan otras dos pruebas de pantalla. No se cambiaron esas pruebas.
- **Pruebas:**
  - `alertas::tests::each_rule_cites_its_source_and_triggers_with_its_risk`;
  - `alertas::tests::the_professional_reviews_the_alert_and_it_counts_that_day`;
  - `ui_tests::a_clinical_alert_is_reviewed_before_an_extraction_is_done`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 486 pasan, 0 fallan, 10 ignoradas |

## E-052 Revisión de seguridad de las alertas revisadas (vistos sellados por el servidor)
- **Hallazgo:** el visto de una alerta clínica lo mandaba el puesto con su nombre y su hora, y el servidor no lo comprobaba. Se podía poner otro nombre o cambiar o borrar un visto.
- **Corrección (`src/server.rs`, `src/firmado.rs`):**
  - un visto nuevo solo se guarda si el usuario de la sesión está activo y tiene permiso clínico, y si la alerta y el paciente existen;
  - quién y cuándo los pone el servidor;
  - un visto ya guardado no cambia ni se borra: `alert_acks` está en `firmado::PROTECTED_LISTS`.
- **Prueba:** `server::tests::alert_reviews_and_sterilization_are_stamped_and_kept_by_the_server`:
  - nombre y hora falsos sustituidos;
  - una alerta inventada no se guarda;
  - un visto borrado desde el puesto sigue en el servidor.

## E-053 T-F5-08: esterilización (ciclos, lotes, instrumental, pacientes, incidencias y afectados)
- **Hecho (`src/esterilizacion.rs`, Almacén › Esterilización):**
  - **Ciclos:** por autoclave (numerados), con programa, hora y quién. Sus lotes llevan contenido, caducidad y una etiqueta única, también entre autoclaves de nombre parecido.
  - **Validación:** con los indicadores químico y Bowie-Dick al terminar, y biológico después.
  - **Uso:** solo un lote de un ciclo validado, sin caducar, sin retirar y sin usar; queda ligado al paciente (y a su tratamiento o cita). La ficha › Historial muestra los lotes usados y avisa si su ciclo es fallido.
  - **Incidencia** (indicador no conforme, biológico positivo o notificada a mano): el ciclo pasa a fallido, se retiran sus lotes sin usar y sale la lista de pacientes en que se usaron sus lotes, con fecha y tratamiento. Se puede exportar a un CSV privado, con fórmulas neutralizadas.
  - **Auditoría:** ciclo, validación, uso, incidencia (con lotes retirados y número de afectados) y resultado biológico.
- **En red:**
  - el uso lo sella el servidor y no cambia ni se borra una vez anotado;
  - un lote retirado no vuelve;
  - un ciclo fallido no vuelve atrás.
  - Al fusionar pacientes, el uso pasa a la ficha que se conserva.
- **Fallo encontrado por la prueba y corregido:** las etiquetas de «Autoclave 1» y «Autoclave 2» salían iguales («AUTO-0001-01»).
- **Pruebas:**
  - `esterilizacion::tests::packs_are_only_used_from_a_validated_cycle_and_traced_to_the_patient`;
  - `esterilizacion::tests::an_incident_lists_exactly_the_affected_patients_and_is_audited`: solo los pacientes del ciclo fallido, por fecha, y la auditoría;
  - `server::tests::alert_reviews_and_sterilization_are_stamped_and_kept_by_the_server`;
  - `ui_tests::a_sterilization_incident_shows_the_affected_patient`.
- **Batería** (de E-052 y E-053):

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 490 pasan, 0 fallan, 10 ignoradas |

## E-054 Revisión de seguridad de la esterilización en red
- **Hallazgos (revisión automática del commit de T-F5-08):**
  1. el servidor no comprobaba los cambios de estado de un ciclo (un puesto podía validarlo sin indicadores, o crearlo ya validado);
  2. tampoco comprobaba el uso de un lote (ciclo, caducidad, retirada, paciente) ni los lotes nuevos;
  3. los lotes, que llevan el paciente en su uso, llegaban a Recepción y no constaban en el registro de accesos.
- **Correcciones (`src/server.rs`, `src/vista.rs`, `src/accesos.rs`):**
  - **Ciclo nuevo:** en curso, sin indicadores y con quién lo creó, puesto por el servidor.
  - **Validar:** solo con permiso clínico, indicador químico correcto y Bowie-Dick no fallido.
  - **Fallido:** solo con motivo; es para siempre.
  - **Inmutable:** autoclave, número, programa y hora; los indicadores ya puestos no cambian; nada se borra.
  - **Uso de un lote:** se comprueba con lo que tiene el servidor (ciclo validado, sin caducar, sin retirar, paciente que existe y permiso clínico) y lo sella el servidor.
  - **Lote nuevo:** solo en un ciclo en curso (del servidor o creado en la misma tanda), con etiqueta única, sin usar ni retirar.
  - **Lote guardado:** no se borra; etiqueta, ciclo, contenido y caducidad no cambian.
  - **Visibilidad:** `sterile_packs` es una lista clínica (Recepción no la recibe) y su uso entra en el registro de accesos del paciente, que se saca del uso del lote.
- **Pruebas:**
  - `server::tests::alert_reviews_and_sterilization_are_stamped_and_kept_by_the_server` comprueba ahora además:
    - un ciclo que llega ya «validado» queda en curso;
    - no se usa un lote de un ciclo sin validar;
    - no se acepta un lote nuevo en un ciclo validado.
  - `vista::tests::reception_does_not_get_clinical_data` incluye las listas clínicas nuevas.

## E-055 Revisión de seguridad: etiquetas de lotes únicas con una sola regla
- **Hallazgo:** al comprobar que la etiqueta de un lote nuevo es única, los lotes de la misma tanda se comparaban distinguiendo mayúsculas y los del servidor sin distinguirlas. Dos lotes «dup-1» y «DUP-1» de la misma tanda pasaban los dos.
- **Corrección (`src/server.rs`):** las dos comprobaciones normalizan igual (sin espacios y en minúsculas), y una etiqueta vacía no vale.
- **Prueba:** `server::tests::alert_reviews_and_sterilization_are_stamped_and_kept_by_the_server` manda dos lotes nuevos con «dup-1» y « DUP-1 » en la misma tanda: entra uno.

## E-056 T-F5-09: encuestas de satisfacción y solicitud de reseñas
- **Hecho (`src/encuestas.rs`, la página `/encuesta/{código}`, Comunicaciones y Configuración › Cita online):**
  - **Envío:** la plantilla «Encuesta de satisfacción» (desactivada hasta que la clínica la active) se manda unos días después de una cita finalizada, con el enlace personal (128 bits).
  - **Consentimiento:** el de los demás avisos (protección de datos, canal aceptado, ni limitado ni bloqueado). Sin aviso enviado no queda encuesta.
  - **Bajas:** desde la propia encuesta («No quiero recibir más encuestas») o por finalidad en la ficha. En los correos va cómo darse de baja.
  - **Sin duplicados:** una por cita, ninguna si el paciente recibió otra en los últimos 90 días (configurable), y respuesta una sola vez.
  - **Resultados medibles** en el cuadro de mando: enviadas, respondidas, tasa de respuesta, NPS con su fórmula y media, en total y por profesional.
  - **Reseña sin condiciones:** el enlace a la reseña pública se ofrece a todos, con cualquier nota (política vigente de Google, en `docs/normativa.md`). Solo se admiten enlaces https.
  - **Integridad:** una encuesta respondida no se retoca ni se borra desde un puesto (`surveys` en `firmado::PROTECTED_LISTS`).
- **Pruebas:**
  - `encuestas::tests::surveys_are_not_duplicated_and_results_are_measured` (NPS calculado a mano: (2 − 1) ÷ 4 = 25);
  - `encuestas::tests::the_survey_message_respects_consent_unsubscribe_and_duplicates`;
  - `encuestas::tests::the_review_link_is_offered_to_everyone_whatever_the_score`;
  - `encuestas::tests::an_answered_survey_cannot_be_retouched_by_a_station`;
  - `booking::tests::a_survey_is_answered_once_from_its_page`: nota baja con reseña ofrecida, una sola respuesta, nota fuera de rango y baja.
- **Sin envíos reales.**
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 495 pasan, 0 fallan, 10 ignoradas |

## E-057 T-F5-10: «Mi espacio», portal del paciente con autorización por objeto y por responsable
- **Hecho (`src/portal_paciente.rs`, la página `/portal/{código}`, ficha › Filiación › «Mi espacio»):** citas próximas (con su «Mi cita»), presupuestos (con su enlace para firmar), facturas, documentos firmados y «Mis recetas», en páginas sin código para el móvil.
- **Por responsable:** el titular es el paciente (desde los 16 años) o su responsable, identificado por el NIF de «Factura a nombre de», que ve a todos sus pacientes. Lo que ve se calcula al publicar: si cambia el responsable en la ficha, cambia lo que ve. Los datos bloqueados no salen.
- **Entrada:**
  - con el DNI/NIE y la fecha de nacimiento (la huella va unida al enlace y solo se publica la huella);
  - 5 fallos bloquean el enlace media hora, y hay límite por dirección IP.
- **Sesión:** código aleatorio en una cookie `HttpOnly`, `SameSite=Strict` (y `Secure` con https), que caduca a los 20 minutos sin uso y a las 2 horas. Va unida a su enlace y a su acceso, y se cierra con «Salir».
- **Por objeto:** cada factura y cada documento se busca solo entre los pacientes del titular. Otro número da «no existe», sin decir si es de otro paciente.
- **Revocación:** un acceso nuevo revoca el anterior del mismo titular; revocado, el enlace y sus sesiones dejan de valer al publicar. Todo va a la auditoría y, al fusionar fichas, el acceso pasa a la que se conserva.
- **Pruebas:**
  - `portal_paciente::tests::a_holder_sees_only_their_patients_and_a_responsible_sees_their_dependents`;
  - `booking::tests::the_patient_portal_never_shows_another_patient_by_changing_an_id`: por HTTP, con la sesión del paciente 101 se piden todas las facturas y documentos de la base por su número, y solo responden los suyos. Se prueban también: sin sesión (401), la sesión en el enlace de otro paciente, salir, revocar y el bloqueo tras 5 fallos;
  - `ui_tests::the_patient_portal_access_is_granted_and_revoked_from_the_file`.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 498 pasan, 0 fallan, 10 ignoradas |

## E-058 Revisión de seguridad de «Mi espacio»: quién ve a quién y entrada sin DNI
- **Hallazgos:**
  1. un responsable veía a todos los pacientes que lo tenían en «Factura a nombre de», también adultos (un hijo mayor al que se le siguen pagando las facturas), y los que se añadían después;
  2. si a un paciente se le borraba el DNI de la ficha después de darle el acceso, se podía entrar con el DNI vacío.
- **Correcciones (`src/portal_paciente.rs`):**
  - un responsable solo ve a menores de 16 años (deja de verlos al cumplirlos) que además estaban autorizados al darle el acceso; la lista se guarda en el acceso;
  - no se publica un acceso sin DNI/NIE y fecha de nacimiento válidos;
  - la entrada rechaza un DNI/NIE de menos de 5 caracteres.
- **Prueba:** `portal_paciente::tests::a_holder_sees_only_their_patients_and_a_responsible_sees_their_dependents` comprueba ahora además:
  - un adulto con el mismo responsable no sale, ni un menor añadido después;
  - a los 16 años deja de salir;
  - sin DNI en la ficha el acceso no se publica, y un DNI vacío nunca vale.

## E-059 T-F5-11: idiomas (castellano, catalán, euskera, gallego e inglés)
- **Hecho (`src/i18n.rs`):** catálogo cuya clave es el texto en castellano; lo que falta en un idioma sale en castellano (alternativa predeterminada).
  - **Interfaz:** idioma elegido en Configuración › Datos de la clínica. El menú, los grupos y las pestañas de la ficha salen traducidos; el resto, en castellano.
  - **Paciente:** su idioma en la ficha › Filiación.
    - Sus avisos de las plantillas de siempre (recordatorio, revisión, cumpleaños, presupuesto, encuesta), con las etiquetas de los enlaces y el texto de la baja, salen en su idioma con la fecha en su idioma. Una plantilla cambiada por la clínica sale tal cual.
    - «Mi espacio» y la encuesta salen en su idioma o, si no lo tiene, en el del navegador (`Accept-Language`), con fechas e importes en su formato.
  - **Documentos:** un consentimiento traducido solo se usa si está validado, con fecha y quién; si no, el original en castellano. Lo ya firmado se conserva tal cual.
  - **Revisión pendiente:** las traducciones las revisa un traductor nativo (`i18n::REVIEWED` es falso, `docs/revision_externa/traducciones.md`, B-17, T-F5-11X). En euskera, las frases se han hecho sin sufijos sobre los comodines.
- **Pruebas de cobertura:**
  - `i18n::tests::the_catalog_is_complete_with_the_same_placeholders`: los cinco idiomas completos, sin claves repetidas y con los mismos comodines. Cubre la navegación y todas las plantillas de aviso;
  - `i18n::tests::every_text_used_with_t_is_in_the_catalog`;
  - `i18n::tests::missing_text_falls_back_to_spanish_and_formats_follow_the_language`: fechas largas en los cinco idiomas, incluidos «d'octubre» en catalán y el genitivo en euskera, importes e `Accept-Language`;
  - `i18n::tests::messages_and_documents_follow_the_patient_language`;
  - `ui_tests::the_interface_in_each_language_is_translated_and_fits`: menú y ficha en cada idioma a 1280 de ancho.
- **Fallo encontrado por las pruebas y corregido:** el genitivo de los meses en euskera («azaroaren»).
- **Ajuste:** la nota del idioma de la interfaz, en su propia línea; en la misma fila no se partía y desbordaba a 1280.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 503 pasan, 0 fallan, 10 ignoradas |

## E-060 Revisión de seguridad: la palabra de baja de cada idioma
- **Hallazgo:** el pie de baja en catalán y gallego pide contestar «BAIXA», pero la respuesta por WhatsApp solo se reconocía como baja si decía «BAJA» o «STOP» o una frase en castellano. Un paciente que contestaba lo que se le pedía seguía recibiendo mensajes.
- **Corrección (`src/whatsapp.rs`, `is_opt_out`):**
  - vale la palabra de cada pie («BAJA», «BAIXA») y «STOP» y «UNSUBSCRIBE»;
  - valen las formas habituales de pedirlo en catalán, gallego, euskera e inglés («no vull rebre», «non quero recibir», «ez dut jaso nahi», «don't want to receive»…), sin tener en cuenta los apóstrofos.
- **Pruebas:**
  - `whatsapp::tests::the_word_each_footer_asks_for_is_an_opt_out`: saca del pie de cada idioma la palabra en mayúsculas que se pide contestar y comprueba que esa palabra, en mayúsculas, minúsculas o con punto, da de baja. Si una traducción cambia la palabra sin que se reconozca, la prueba falla. Comprueba también frases habituales en cada idioma;
  - `whatsapp::tests::replying_baja_stops_whatsapp`: además, «No vull canviar la cita» y «Non podo ir» no son una baja.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 504 pasan, 0 fallan, 10 ignoradas |

## E-061 T-F5-12: campañas a grupos de pacientes
- **Hecho (`src/campanas.rs`, `src/screens/campanas.rs`, D-24):** pestaña «📣 Campañas» en Comunicaciones.
  - **Segmento:** edad, última visita, presupuesto pendiente y tratamiento hecho o no en el último año. El tratamiento solo lo ajusta un perfil clínico.
  - **Consentimiento por finalidad:** el de promociones, con fecha, en Filiación, además del de protección de datos y el del canal.
    - La baja de las campañas es una clase de aviso más.
    - Las dos cosas se vuelven a mirar al pasar cada mensaje a la cola.
    - Al fusionar fichas no se pierde una baja (antes se perdían las bajas de la ficha eliminada).
  - **Límites:** horario de lunes a viernes, máximo diario y días sin otra campaña al mismo paciente.
  - **Previsualización:** cuántos la recibirán, cuántos se quedan fuera y por qué, y los primeros mensajes tal como saldrán. Al programarla, la lista queda fija.
  - **Envío:** ninguno. Los mensajes solo pasan a la cola de «Mensajes», con la baja en el idioma del paciente, y desde allí se envían como los demás. Dos puestos no ponen dos mensajes al mismo paciente (también entran en la clave de duplicados).
  - **Quién:** la programa y la cancela Dirección; el servidor no acepta de otro perfil que la programe, la cambie o la borre. Al cancelarla se descarta lo que está en la cola sin enviar.
  - **Resultados:** pasados a la cola, por pasar, los que ya no podían recibirla, y las citas atribuidas y a cuántas acudió. El criterio está escrito en la pantalla (`campanas::CRITERION`).
- **Pruebas (ninguna envía nada):**
  - `campanas::tests::the_preview_shows_who_and_why_not_and_scheduling_freezes_the_list`: excluidos por motivo (sin consentimiento, de baja, sin contacto, campaña reciente), texto con los datos del paciente y la baja, comodines de salud rechazados y lista fija;
  - `campanas::tests::segments_by_age_last_visit_budget_and_treatment`;
  - `campanas::tests::queueing_respects_hours_weekdays_daily_limit_and_late_unsubscribes`: antes y después del horario, sábado, límite diario, una baja de última hora y la frecuencia entre campañas;
  - `campanas::tests::cancelling_stops_what_is_left_and_drops_what_is_unsent`;
  - `campanas::tests::two_stations_queue_one_message_per_patient`;
  - `campanas::tests::results_attribute_only_new_appointments_within_the_window`: ni la cita que ya tenía, ni la de fuera de plazo, ni la anulada;
  - `campanas::tests::only_direction_schedules_or_changes_a_campaign`;
  - `server::tests::only_direction_schedules_campaigns_and_any_station_queues_them`: con el servidor de verdad, recepción no programa, ni añade destinatarios, ni cambia el texto, ni borra. Comprobado que falla si se quita la regla del servidor;
  - `bank::tests::merging_moves_everything_of_the_removed_patient`: además, campañas y bajas;
  - `ui_tests::a_campaign_is_previewed_and_scheduled_by_direction_only`: Dirección crea, previsualiza y programa; recepción no ve «Programar campaña»; el consentimiento en la ficha queda con su fecha.
- **Pendiente externo (DPO):** el texto del consentimiento para promociones, si se puede elegir por tratamiento y cuánto se guardan las listas (`docs/revision_externa/dpo.md`, apartado 5 ter).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 513 pasan, 0 fallan, 10 ignoradas |

## E-062 Revisión de seguridad de las campañas: el servidor pasa los mensajes a la cola
- **Hallazgos:**
  1. Un puesto que no era de Dirección podía marcar destinatarios de una campaña programada: como «ya no podían recibirla» (para saltárselos) o con un mensaje inventado. Para el servidor era lo que hace el paso a la cola.
  2. Al fusionar dos fichas, el consentimiento para promociones de la que se eliminaba pasaba a la que se conserva aunque esta no lo tuviera.
- **Correcciones:**
  - **Servidor:** los mensajes de una campaña programada los pasa a la cola el servidor, con sus datos y su hora (`ChainOp::QueueCampaigns`, que cualquier puesto puede pedir; sin servidor, el propio programa).
    - De otro perfil que no sea Dirección solo acepta borradores (`campanas::draft_only`): ni marcar destinatarios, ni programar, cambiar, cancelar o borrar.
    - El puesto solo lo pide cuando hay algo que pasar (`Db::campaigns_due`).
  - **Fusión:** el consentimiento para promociones se conserva solo si lo tenían las dos fichas (con la fecha más reciente).
- **Pruebas:**
  - `server::tests::only_direction_schedules_campaigns_and_the_server_queues_them` (antes `…_and_any_station_queues_them`):
    - recepción no programa;
    - recepción no marca destinatarios (ni como pasados a la cola ni como saltados), no añade a nadie, no cambia el texto y no borra;
    - lo que marca el servidor apunta a un mensaje de verdad del paciente y de la campaña, y no hay más.
  - `campanas::tests::the_server_has_everything_it_needs_to_queue`: con solo las listas que prepara el servidor da lo mismo que con todo, y todo lo que cambia está entre lo que guarda. Comprobado que falla si se quita «messages» de esas listas.
  - `campanas::tests::only_direction_schedules_or_changes_a_campaign`: marcar o saltarse a alguien, o devolverla a borrador, ya no lo acepta `draft_only`; `campaigns_due` respeta el horario y el fin de semana.
  - `bank::tests::merging_moves_everything_of_the_removed_patient`: además, el consentimiento solo de una ficha no pasa a la que se conserva.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 514 pasan, 0 fallan, 10 ignoradas |

## E-063 T-F6-01: grupos de clínicas (sociedades, centros con permisos y vista del grupo)
- **Hecho (D-25):**
  - **Centros con permisos** (`User::centers`, `src/vista.rs`, `server::State::save` y `chain_op`):
    - el servidor no acepta de un usuario de unos centros citas, cajas, cobros, citas pedidas por internet ni huecos de otro centro (crear, cambiar, mover o borrar);
    - tampoco operaciones con el centro de otro, sin centro, o sobre un cobro o una cita pedida de otro centro;
    - no le manda las cajas de los demás centros;
    - al cambiarle los centros o el perfil, su puesto recoge los datos enteros.

    En la pantalla: los centros de cada usuario en Configuración › Usuarios, y el selector de centro y los informes con solo sus centros.
  - **Sociedades:** cada una, su instalación (su numeración, su cadena VERI*FACTU, sus cajas y sus pacientes).
  - **Vista del grupo** (`src/grupo.rs`, `src/screens/grupo.rs`):
    - el resumen de cada clínica, por centro: producción, cobrado, citas, a las que acudió y a las que no; además, lo facturado y las altas. Se calcula con los informes y no lleva datos de pacientes;
    - las claves de grupo se guardan como huella y las crea y retira Dirección (Configuración › Trabajo en red);
    - la ruta `GET /grupo/resumen` solo responde con una clave de grupo en uso, y los intentos fallidos se frenan;
    - Informes › Grupo de clínicas junta las clínicas, con los totales del grupo.
- **Pruebas:**
  - `server::tests::a_user_of_one_center_cannot_act_in_another`:
    - solo recibe su caja, y los cobros de todos (para el saldo);
    - no da, ni cambia, ni mueve citas de otro centro, ni borra un cobro de otro centro;
    - no hace operaciones en otro centro, sin centro o con un cobro de otro centro;
    - con los centros cambiados, recibe los datos enteros.

    Comprobado que falla si se quita cualquiera de las cuatro reglas: escritura, operaciones, datos enteros y cajas.
  - `server::tests::the_group_key_opens_only_the_summary`:
    - el resumen tal cual sale, sin nombres, documentos, teléfonos ni correos de pacientes, y solo con los campos de la lista cerrada;
    - con la clave de grupo no se entra en `/datos`, `/cambios`, `/acceso`, `/estado` ni `/historia`;
    - la clave de la clínica no abre el resumen;
    - fechas que faltan o un periodo de más de 400 días: no;
    - probar claves acaba en 429, también para la buena.
  - `server::tests::a_group_of_two_clinics_adds_up_their_summaries`: dos servidores con su identidad.
    - El resumen de cada uno coincide con sus datos y los totales del grupo suman los dos.
    - La clave de una no vale en la otra, ni con otra huella, ni sin huella, ni retirada.
    - Una clave creada desde recepción no llega al servidor.
  - `grupo::tests::the_summary_matches_the_reports_and_carries_no_patient_data`: con dos centros, cada centro coincide con los informes y los centros suman el total.
  - `grupo::tests::group_keys_are_stored_as_hashes_and_can_be_revoked`;
  - `grupo::tests::the_connection_text_round_trips_and_needs_the_server_fingerprint`;
  - `ui_tests::a_user_is_limited_to_a_center_and_a_clinic_joins_the_group_view`.
- **Pendiente externo:** un grupo real con datos conjuntos (T-F6-09X).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 521 pasan, 0 fallan, 10 ignoradas |

## E-064 T-F6-02: servidor en la nube opcional, desplegado y ensayado en local
- **Hecho:**
  - **Servidor sin ventana** (`server::run`, `RunOptions`):
    - sin datos de la clínica no arranca, salvo `--datos-de-ejemplo` para ensayos; así un servidor nuevo nunca sirve datos de ejemplo por error;
    - con la salida redirigida, como servicio, no escribe la clave de la clínica en el registro;
    - dice su hora y su zona;
    - admite `--puerto`;
    - si falla, termina con código 1.
  - **Mantenimiento sin ventana:**
    - `--estado-servidor DIRECCIÓN CLAVE [HUELLA]`: comprueba desde fuera, con la prueba de identidad o la huella, y termina con 0 o 1 para una alarma;
    - `--parar-servidor [DIRECCIÓN]`: solo desde el propio ordenador;
    - `--clave-servidor`: en el propio ordenador.
  - **Despliegue** (`deploy/nube/`):
    - unidad de systemd endurecida: usuario propio, sistema de solo lectura salvo sus datos, `UMask=0077`, sin privilegios y con la zona horaria de la clínica. Pasa `systemd-analyze verify`;
    - aprovisionamiento con modo de ensayo, que no toca el sistema: usuario, carpetas, programa, unidad y cortafuegos con las direcciones de la clínica. No arranca sin los datos.
  - **Guía** (`docs/despliegue_nube.md`):
    - condiciones del alojamiento (encargo, región del EEE, seguridad, brechas, copias fuera, acceso por VPN), pendientes de confirmar por el DPO;
    - cómo llevar los datos, copias y recuperación, vigilancia y actualizaciones.
- **Pruebas** (`tests/ensayo_nube.rs`, con el programa de verdad como proceso aparte y carpetas aisladas; todo por 127.0.0.1):
  - `the_cloud_server_rehearsal_runs_locally`:
    - sin datos no arranca;
    - la clave no va a la salida;
    - dice `TZ=Europe/Madrid`;
    - `--estado-servidor` funciona con la clave buena, y no con otra clave ni con otra huella;
    - se para y vuelve a arrancar con la misma identidad;
    - datos, configuración y claves quedan en 0600.
  - `the_provisioning_script_rehearses_without_touching_the_system`:
    - las órdenes del sistema solo se apuntan, sin `systemctl start`;
    - la unidad lleva el puerto, el usuario, la zona horaria y el endurecimiento, y nunca datos de ejemplo;
    - carpetas en 0700;
    - el programa instalado arranca con el entorno de la unidad;
    - opciones no válidas terminan con código 2.
- **Comprobado a mano:** el servidor aislado escribió su clave solo con `--clave-servidor`. `--parar-servidor` sin dirección usa el puerto de `servidor.json`: con `--puerto` hay que darle la dirección, y la guía y la prueba lo hacen.
- **Pendiente externo:** contratar el alojamiento y validar las transferencias (B-18, T-F6-02X).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 523 pasan (521 + 2 del ensayo en la nube), 0 fallan, 10 ignoradas |
| `systemd-analyze verify deploy/nube/veodardental-servidor.service` | 0 | solo avisa de que el programa no está instalado en este ordenador |

## E-065 Revisión de seguridad del despliegue: carpeta del servicio y clave fuera de la orden
- **Hallazgos:**
  1. El aprovisionamiento, como administrador, hacía `chmod` en las subcarpetas y `chown -R` en la carpeta del servicio. Esa carpeta es del usuario del servicio: un enlace puesto ahí habría desviado esos cambios a otro sitio del sistema. La guía también decía `chown -R` al llevar los datos.
  2. `--estado-servidor` recibía la clave de la clínica en la orden, que se ve en la lista de procesos y en el historial.
- **Correcciones:**
  - **Carpeta del servicio:**
    - la crea systemd (`StateDirectory=veodardental`, `StateDirectoryMode=0700`);
    - el script solo toca la carpeta base (`install -d -m 700 -o veodardental`), dentro de `/var/lib`, que es del sistema, y se para si es un enlace;
    - nada recursivo; las subcarpetas las crea el programa;
    - en la guía, los ficheros se ponen uno a uno con `install -o veodardental -m 600`.
  - **La clave** de `--estado-servidor` va en `VEODARDENTAL_CLAVE` o por la entrada. Sin clave, dice que falta.
- **Pruebas** (`tests/ensayo_nube.rs`):
  - `the_cloud_server_rehearsal_runs_locally`: con la clave en la variable y por la entrada, sí; sin clave, «Falta la clave».
  - `the_provisioning_script_rehearses_without_touching_the_system`:
    - las órdenes incluyen `install -d … /var/lib/veodardental`, y ningún `chown -R` ni `chmod -R`;
    - la unidad lleva `StateDirectory` y `StateDirectoryMode=0700`;
    - con la carpeta del servicio como enlace, el script se para sin tocar nada.
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 523 pasan, 0 fallan, 10 ignoradas |
| `systemd-analyze verify deploy/nube/veodardental-servidor.service` | 0 | solo avisa de que el programa no está instalado aquí |

## E-066 T-F6-03: datáfono y financiación (adaptadores, estados, conciliación, cancelación y recuperación)
- **Hecho (D-26, `docs/datafono.md`):**
  - **Datáfono** (`src/datafono.rs`):
    - operaciones con referencia única, estado e historia;
    - el cobro o la devolución, una sola vez, al aprobarse, en la caja del centro donde se hizo;
    - recuperación preguntando por la referencia;
    - cancelación, en la que manda el datáfono;
    - devolución por lo que quede;
    - solo los cuatro últimos dígitos de la tarjeta;
    - adaptadores manual y HTTP con contrato, y ajustes en un fichero privado del puesto.
  - **Financiación** (`src/financiacion.rs`):
    - solicitud, aprobación o denegación, firma, pago de la entidad y cancelación, con historia;
    - el paciente, pagado por el total, una sola vez;
    - la comisión, como gasto;
    - desistimiento en plazo con devolución a la entidad;
    - conciliación del ingreso del banco por lo recibido.
  - **Pantalla:**
    - Cuenta del paciente: datáfono, con aviso de las operaciones sin respuesta, y financiación;
    - Configuración › Facturación: el datáfono del puesto.

  Las listas nuevas pasan al fusionar fichas y quedan con los centros (`card_txs` es de un centro).
- **Arreglado de paso:** las clínicas del grupo (`grupo.json`, con las claves de grupo) se guardaban legibles para otras cuentas del ordenador. Ahora van solo para el usuario (`write_private`).
- **Pruebas** (todas con el datáfono simulado `datafono::fake` o sin red):
  - `datafono::tests::a_connected_terminal_charges_once`:
    - consultas hasta aprobarse;
    - un solo cobro aunque se pregunte y se reenvíe, y el datáfono cobró una vez;
    - denegada, sin cobro;
    - con otra clave, el datáfono no atiende;
    - la tarjeta queda como «···· 0004», sin el número completo.
  - `datafono::tests::a_lost_reply_is_recovered_by_asking_not_by_resending`:
    - respuesta perdida: «sin respuesta», se pregunta y se apunta el cobro que sí se hizo, una vez;
    - no llegó al datáfono: no hecha;
    - en el datáfono más de dos minutos: se comprueba.
  - `datafono::tests::cancelling_and_refunding_follow_the_terminal`:
    - cancelar mientras está en el datáfono;
    - una cancelada que sí se cobró pasa a aprobada, y una aprobada no vuelve atrás;
    - la devolución sin pasarse, y no de otro paciente.
  - `datafono::tests::a_standalone_terminal_is_recorded_by_hand`: con autorización, una vez; «no pasó»; ajustes sin https rechazados.
  - `financiacion::tests::a_financing_goes_from_request_to_payment_once`:
    - orden de los pasos;
    - el pago no supera lo financiado y se apunta una sola vez;
    - saldo, comisión e historia;
    - conciliación por lo recibido, una vez.
  - `financiacion::tests::withdrawing_follows_the_deadline_and_returns_the_money`:
    - cancelar antes del pago;
    - desistir pagada en plazo, con devolución, y el paciente vuelve a deber lo de antes;
    - fuera de plazo, no;
    - validaciones.
  - `ui_tests::the_card_terminal_and_a_financing_from_the_patient_account`: cobro con el datáfono sin conectar, con autorización, un solo cobro; solicitud de financiación y aprobación con referencia.
  - `bank::tests::merging_moves_everything_of_the_removed_patient` sigue en verde con las listas nuevas.
- **Ajuste por la prueba de pantalla:** los botones de los pasos de la financiación dicen «Marcar aprobada», «Marcar firmada»… Antes coincidían con el estado de los cobros y eran ambiguos.
- **Pendiente externo:** proveedor, especificación y contrato (B-19, T-F6-03X).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 530 pasan (528 + 2 del ensayo en la nube), 0 fallan, 10 ignoradas |

## E-067 Revisión de seguridad: sacar dinero y el banco, con su permiso también en el servidor
- **Hallazgos:**
  1. La devolución con datáfono y los pasos de la financiación que mueven dinero se hacían en el puesto sin los permisos de las operaciones parecidas: caja para devolver o anular, cobros para cobrar, banco para conciliar.
  2. **Fallo anterior, de más alcance:** el servidor no comprobaba los cobros que llegaban directamente de un puesto. Un usuario sin permiso de caja podía apuntar un cobro en negativo o una anulación, o cambiar el importe de un cobro o borrarlo, sin pasar por `ChainOp::Refund` o `ChainOp::CancelPayment`. También podía tocar los movimientos del banco.
- **Correcciones:**
  - **Servidor** (`State::save`, `payment_change_ok`), para quien no es Dirección:
    - un cobro en negativo o que anula otro, solo con permiso de caja;
    - un cobro no se borra, ni cambia de importe, paciente, forma de pago, fecha, lo que anula o la cuota que paga. Sí se corrigen otras cosas, como el concepto o la caja a la que va;
    - los movimientos y las cuentas del banco, solo con su permiso.
  - **Financiación:** el pago de la entidad, la cancelación y la conciliación son operaciones del servidor (`ChainOp::FinPaid` con permiso de cobros, `ChainOp::FinCancel` con el de caja y `ChainOp::FinMatchBank` con el del banco).
  - **Pantalla:** «Devolver» del datáfono, solo con caja. En la financiación, «Pagada por la entidad» con cobros, «Cancelar» con caja y «Conciliar» con el banco.
- **Pruebas:** `server::tests::money_out_and_the_bank_need_their_permission`, con el servidor de verdad:
  - la doctora apunta un cobro, pero no uno en negativo ni una anulación; no cambia el importe de otro ni lo borra, y sí corrige el concepto;
  - recepción apunta una devolución, pero no concilia a mano en el banco;
  - financiación: la doctora no cancela (sin caja); recepción la marca pagada una vez, con el cobro del total y la comisión; recepción no concilia, y Dirección sí.

  Comprobado que falla si se quita la regla de los cobros («sacar dinero o anular, no») o la del banco («el banco, con su permiso»).
- **Batería:**

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | — |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | — |
| `cargo test --workspace --all-targets --locked` | 0 | 531 pasan (529 + 2 del ensayo en la nube), 0 fallan, 10 ignoradas |

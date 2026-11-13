# Concurrencia, fallos y entradas al azar (T-F1-12)

Actualizado el 13/11/2026. Cada flujo, con la prueba que lo comprueba en red (servidor) y, donde aplica, en la carpeta compartida.

## T-F1-12a: dos puestos a la vez
| Flujo | Servidor | Carpeta compartida |
|---|---|---|
| Facturas (numeración y cadena) | `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain` | `shared::tests::two_stations_invoicing_in_a_shared_folder_keep_the_chain` |
| Recibos e identificadores (numeración única por puesto) | Cada puesto su serie (`sample::tests::receipts_are_unique_per_station`) | `shared::tests::each_computer_gets_its_own_station_number_in_the_shared_folder` (nuevo, defecto D1) |
| Números de historia | `server::tests::two_new_patients_never_share_a_history_number` | `shared::tests::two_stations_invoicing_in_a_shared_folder_keep_the_chain` |
| Cuotas | `server::tests::an_installment_paid_by_two_stations_at_once_is_charged_once` | Con el fichero bloqueado (`ChainOp`) |
| Anulaciones y devoluciones | La misma prueba (no se anula dos veces) | Con el fichero bloqueado |
| Remesas y recibos devueltos; liquidaciones a mutuas | `server::tests::remittances_and_claims_made_by_two_stations_at_once_charge_once` | Con el fichero bloqueado |
| Caja (cobro mientras otro cierra) | `server::tests::a_payment_made_while_another_station_closes_the_cash_leaves_it_balanced` | `shared::tests::a_payment_made_while_another_station_closes_the_cash_goes_to_the_open_one` |
| Stock (gasto a la vez y mismo tratamiento hecho dos veces) | `material_spent_by_two_stations_at_once_is_all_taken_from_stock` y `the_same_treatment_done_on_two_stations_spends_its_material_once` | `stock_spent_by_two_stations_adds_up`, `stock_moves_made_by_two_stations_at_once_are_all_counted` y `the_same_treatment_done_on_two_stations_spends_once_in_the_shared_folder` |
| Recordatorios | `server::tests::reminders_made_by_two_stations_at_once_are_sent_once` | `comms::tests::reminders_generated_twice_are_left_once` |
| Correos (envío) | `server::tests::two_stations_sending_emails_at_once_send_each_one_once` | Con el fichero bloqueado |
| Reservas por internet | `server::tests::an_online_request_confirmed_by_two_stations_at_once_makes_one_appointment`; en la página, `booking::tests::web_page_books_a_free_slot` (409) | Con el fichero bloqueado |
| Banco | `server::tests::a_statement_imported_and_reconciled_by_two_stations_at_once_counts_once` | `shared::tests::a_statement_imported_by_two_stations_counts_once_in_the_shared_folder` |
| Ortodoncia (controles) | `server::tests::two_ortho_controls_recorded_at_once_are_both_kept` | La misma combinación de listas |
| Laboratorio (gasto) | `server::tests::a_lab_work_received_on_two_stations_is_one_expense` | El mismo identificador del gasto |
| Lo firmado y lo emitido | `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station` y `signed_records_only_change_patient_through_a_merge_on_the_server` | `shared::tests::a_station_cannot_change_signed_records_in_the_shared_folder` |

**D1, defecto demostrado y corregido.** En la carpeta compartida, el número de puesto lo pone cada ordenador a mano; los de antes tenían 0. Dos ordenadores con 0 (o con el mismo número) sacaban los mismos identificadores y los mismos recibos, así que dos pacientes nuevos a la vez quedaban en uno.
- **Demostración:** la prueba fallaba con «0 y 0».
- **Ahora:**
  - al guardar, el ordenador recibe un número libre (o el que ya usó) y se le queda;
  - un número que está usando otro ordenador en los últimos 30 minutos se rechaza con su nombre (`shared::pick_station`).
- **Límite:** lo creado antes del primer guardado de un ordenador sin número usa identificadores sin su número. Ese primer guardado se hace al empezar a trabajar.

## T-F1-12b: desconexión, reenvío, reinicio y fallo de escritura
| Caso | Prueba | Resultado |
|---|---|---|
| Respuesta perdida y reenvío del mismo cambio | `server::tests::a_push_resent_after_a_lost_answer_changes_nothing` (nueva) | Sin duplicados. Ya funcionaba: los cambios llevan el «antes» y se combinan. |
| Operación del servidor repetida (cobrar, anular, remesa…) | Las pruebas de `ChainOp` de 12a | La segunda vez se rechaza («ya está pagada», «ya anulado»). |
| Puesto desconectado mucho tiempo | `server::tests::a_station_too_far_behind_gets_everything` | Recibe los datos enteros. |
| Reinicio del servidor o apagón | `server::tests::the_server_restarts_with_everything_and_stations_carry_on` (nueva) | Arranca con el fichero y el diario, la revisión no vuelve atrás y los puestos siguen. |
| Fallo de escritura (disco lleno o sin permiso) | `server::tests::a_failed_write_leaves_nothing_half_saved` (nueva) | No avanza la revisión, el servidor sigue con lo de antes, el disco queda sin nada a medias, y el mismo cambio se guarda después. |
| Diario y escritura atómica en un ordenador | `sync::tests::the_journal_brings_back_what_was_not_rewritten` y `local::tests::saves_only_what_changed_and_comes_back_after_a_crash` | Existentes. |

## T-F1-12c: protocolo del servidor
| Entrada | Prueba |
|---|---|
| Cuerpo mayor que el máximo (413), cabecera larga (431), por trozos (501), cuerpo corto (400) | `server::tests::requests_are_limited` |
| Clave equivocada repetida (429); sin clave no se lee el cuerpo | `requests_are_limited` |
| Respuestas grandes comprimidas y leídas con límite | `big_answers_travel_compressed` y `answers_are_read_with_a_limit` |
| El servidor no acepta cuerpos comprimidos | Revisado: solo comprime lo que envía. |
| Peticiones y cuerpos estropeados, con sesión y sin ella | `server::tests::the_server_survives_random_requests_and_its_data_stays_readable` (nueva) |

## T-F1-12d: entradas al azar con semilla fija (`test_rng::Rng`)
| Punto | Prueba | Iteraciones |
|---|---|---|
| Lector de peticiones | `server::tests::the_request_reader_survives_random_bytes` | 20.000 |
| Servidor entero (todas las rutas) | `server::tests::the_server_survives_random_requests_and_its_data_stays_readable` | 4.000. Llegan al fondo: hubo cambios aceptados en `/cambios`, lecturas en `/datos` y rechazos 400. |
| Norma 43 | `bank::fuzz::norma43_survives_random_files` | 3.000 |
| Excel/CSV de extractos e importes | `bank::fuzz::statement_tables_and_amounts_survive_random_input` | 2.000 |
| Combinación de cambios de dos puestos | `shared::tests::merging_random_edits_keeps_what_each_station_did` | 150. Se puede leer, no se pierde nada creado, lo cambiado por uno queda y el resultado es repetible. |

Todas en menos de un segundo. La semilla está en cada prueba: un fallo se reproduce igual.

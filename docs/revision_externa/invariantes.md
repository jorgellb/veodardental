# Invariantes protegidos y sus pruebas (T-INI-07)

Son las once mejoras que el encargo pide conservar (apartado 2 del objetivo).
Para cada una: qué se garantiza, dónde está y qué pruebas lo demuestran.
Todas usan datos inventados y se ejecutan en la batería
(`tools/aislado.sh cargo test --workspace --all-targets --locked`). Cuando un
cambio toca uno de estos flujos, sus pruebas tienen que seguir pasando; la
evidencia de cada cambio lo anota.

Las pruebas marcadas con «falla sin la regla» se comprobaron quitando la regla
y viendo que fallan. Está anotado en la evidencia citada.

## 1. Contraseñas y secretos solo en el servidor
Las contraseñas se guardan con sal y hash, en el servidor. A ningún puesto le
llegan contraseñas, sales, claves del móvil ni claves de grupo. Nada secreto
va a un servidor que no ha probado su identidad.
- `server::tests::only_direction_changes_users_and_secrets_stay_on_the_server`
- `server::tests::nothing_secret_goes_to_an_unproven_server`
- `server::tests::the_server_applies_each_operation_permission_to_each_profile` (E-079)
- `server::tests::phone_code_needs_the_password_first`
- `users::tests::salts_are_random`, `users::tests::old_hashes_still_work_and_are_upgraded`
- `grupo::tests::group_keys_are_stored_as_hashes_and_can_be_revoked`

## 2. Permisos aplicados en el servidor
Cada escritura y cada operación se comprueba en el servidor según el perfil,
los permisos y los centros del usuario, aunque el puesto se salte la pantalla
(`src/server.rs`, `State::save` y `ChainOp::needs`; rutas en
`auditoria_seguridad.md`, 2 bis).
- `server::tests::the_server_applies_each_write_rule_to_each_profile` (E-079)
- `server::tests::the_server_applies_each_operation_permission_to_each_profile` (E-079)
- `server::tests::reception_does_not_receive_clinical_data`
- `server::tests::money_out_and_the_bank_need_their_permission` (falla sin la regla, E-067)
- `server::tests::a_user_of_one_center_cannot_act_in_another` (falla sin la regla, E-063)
- `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station`

## 3. Límites de entrada y descompresión
Lo que pasa del límite se rechaza antes de leerlo entero. La tabla completa,
con una prueba por límite, está en `auditoria_seguridad.md`, 4 bis (E-080 y
E-081).
- `server::tests::requests_are_limited`
- `server::tests::answers_are_read_with_a_limit` (respuesta comprimida que se infla: bomba gzip)
- `server::tests::idle_connections_cannot_take_the_server` y `server::tests::a_slow_drip_head_is_cut_at_the_deadline` (fallan sin la regla)
- `dicom::tests::invalid_compressed_huge_or_random_files_are_refused_without_panicking`
- `stl::tests::a_file_over_the_limit_is_refused_before_reading_it`
- `server::tests::the_server_survives_random_requests_and_its_data_stays_readable`

## 4. Escrituras atómicas y recuperación del diario
Un guardado se escribe entero o no se escribe. El diario recupera lo que no se
llegó a reescribir. Una línea cortada al final se descarta; una dañada antes,
o líneas desordenadas, se avisan y no se aplican.
- `server::tests::a_failed_write_leaves_nothing_half_saved`
- `server::tests::a_cut_or_damaged_journal_never_applies_changes_out_of_order`
- `server::tests::the_server_restarts_with_everything_and_stations_carry_on`
- `sync::tests::the_journal_brings_back_what_was_not_rewritten`
- `local::tests::saves_only_what_changed_and_comes_back_after_a_crash`
- `datos_cifrados::tests::plain_data_is_refused_when_encrypted_and_cut_conversions_recover`
- `actualizar::tests::a_cut_between_any_two_steps_leaves_the_previous_version_whole`
- `shared::tests::two_stations_saving_to_same_file_lose_nothing`

## 5. Identificadores y numeración únicos entre puestos
Cada puesto (1 a 15) y el servidor (16) crean identificadores que acaban en
su número, módulo 16. Los recibos llevan la serie del puesto. Los números de
historia y de factura los da el servidor.
- `sample::tests::ids_made_on_different_stations_never_collide` (nueva)
- `sample::tests::receipts_are_unique_per_station`
- `server::tests::two_new_patients_never_share_a_history_number`
- `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain`
- `shared::tests::each_computer_gets_its_own_station_number_in_the_shared_folder`

## 6. Facturas encadenadas por el servidor
La huella de cada factura encadena con la anterior y la calcula el servidor
(o el ordenador que tiene la carpeta). Las rectificativas y las anulaciones
entran en la cadena. Las facturas de antes conservan su huella (D-29).
- `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain`
- `shared::tests::two_stations_invoicing_in_a_shared_folder_keep_the_chain`
- `verifactu::tests::official_hash_examples`
- `verifactu::tests::corrections_and_cancellations_join_the_chain`
- `verifactu::tests::a_lost_answer_is_resent_identically_and_reconciled_as_registered`
- `verifactu::tests::only_one_computer_sends_to_the_aeat`
- `propiedades::old_records_keep_their_amounts_and_the_invoice_chain_its_hashes` (E-078)

## 7. Operaciones de dinero y de stock idempotentes
Un envío repetido tras una respuesta perdida no cambia nada. Lo que hacen dos
puestos a la vez se cobra, se gasta o se apunta una sola vez, y el stock
cuenta todos los movimientos.
- `server::tests::a_push_resent_after_a_lost_answer_changes_nothing`
- `server::tests::an_installment_paid_by_two_stations_at_once_is_charged_once`
- `server::tests::remittances_and_claims_made_by_two_stations_at_once_charge_once`
- `server::tests::the_same_treatment_done_on_two_stations_spends_its_material_once`
- `server::tests::material_spent_by_two_stations_at_once_is_all_taken_from_stock`
- `server::tests::a_lab_work_received_on_two_stations_is_one_expense`
- `server::tests::a_statement_imported_and_reconciled_by_two_stations_at_once_counts_once`
- `server::tests::orders_are_proposed_and_received_once_and_approved_by_expenses`
- `pago::tests::a_notice_is_charged_once_and_a_second_payment_is_flagged`
- `datafono::tests::a_connected_terminal_charges_once`, `datafono::tests::a_lost_reply_is_recovered_by_asking_not_by_resending`

## 8. Cuotas y presupuestos exactos
Cada importe de línea es el exacto redondeado al céntimo, con las mitades
lejos del cero. Las cuotas más la entrada suman el total. Presupuestos,
facturas, caja y remesas suman al céntimo. Son pruebas de propiedades con
miles de casos al azar y semilla fija (E-078, D-29).
- `propiedades::line_amounts_are_exact_to_the_cent`
- `propiedades::installments_plus_down_payment_add_up_to_the_total`
- `propiedades::budgets_and_invoices_add_up_line_by_line`
- `propiedades::cash_and_cancellations_balance_to_the_cent`
- `propiedades::a_remittance_adds_up_its_items`
- `cents::tests::sums_are_exact_to_the_cent`

## 9. Trazabilidad de implantes
Cada implante va del pedido y su lote al paciente en que se colocó. Los lotes
caducados no se usan. El material estéril solo sale de un ciclo validado y
queda unido al paciente.
- `stock::tests::an_implant_lot_goes_from_the_order_to_the_patient`
- `stock::tests::lots_about_to_expire_are_flagged_and_expired_ones_are_not_used`
- `compras::tests::receiving_in_parts_with_lots_never_exceeds_the_order`
- `esterilizacion::tests::packs_are_only_used_from_a_validated_cycle_and_traced_to_the_patient`
- `ui_tests::receive_implants_in_two_lots_from_the_orders_screen`

## 10. Fusión completa de pacientes
Al fusionar dos fichas, todo lo del paciente que se va pasa al que se
conserva. La prueba busca cualquier referencia en todos los datos, así que
cubre también las listas nuevas: cobros con datáfono, financiaciones y
autorizaciones, añadidos a la prueba en esta tarea. Lo firmado solo cambia de
paciente mediante una fusión.
- `bank::tests::merging_moves_everything_of_the_removed_patient` (falla si una lista no se pasa, E-083)
- `server::tests::signed_records_only_change_patient_through_a_merge_on_the_server`
- `shared::tests::signed_records_change_patient_only_through_a_merge_in_the_shared_folder`
- `migracion::tests::merging_patients_keeps_their_origin`

## 11. Importaciones y copias seguras
Repetir una importación no duplica. Un fichero cortado no se importa. Las
rutas de red solo se siguen dentro de la carpeta de documentos. La copia se
cifra con su clave, aparte. El ensayo de restauración comprueba el contenido.
- `migracion::tests::repeating_or_finishing_a_migration_never_duplicates`
- `migracion::tests::duplicates_are_decided_and_a_shared_nif_never_joins_two_people`
- `migracion::tests::network_paths_are_recognised_and_only_trusted_inside_the_documents_folder`
- `extra::tests::import_patients_from_csv_without_duplicates`
- `bank::tests::a_cut_norma43_file_is_not_imported`
- `golive::tests::only_imports_after_emptying_count`
- `tools::tests::backup_roundtrip_and_prune`
- `tools::tests::second_copy_is_encrypted_with_the_backup_password`
- `tools::tests::a_restore_drill_checks_the_contents_and_the_key_lives_apart`
- `local::tests::restoring_an_old_backup_is_not_lost`
- `paths::tests::tests_never_use_the_real_folders`

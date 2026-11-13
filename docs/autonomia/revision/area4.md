# Área 4: bancos y conciliación

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Importar un extracto Norma 43 (registros 11, 22, 23, 33 y 88) o Excel/CSV | `parse_norma43_for`, `parse_bank_table` y `read_statement` en src/screens/bancos.rs |
| Añadir los movimientos sin repetir los ya importados | `Db::add_bank_moves`, `Db::import_bank_moves` |
| Conciliación automática:<br>- cobros por transferencia, Bizum o financiación;<br>- liquidaciones de tarjeta;<br>- remesas SEPA;<br>- recibos devueltos y devoluciones a pacientes;<br>- gastos | `auto_candidates`, `auto_plan` y `auto_reconcile` |
| Conciliación a mano, gasto nuevo con un cargo, «otro ingreso» o traspaso | `match_candidates`, `match_bank_move` y `expense_for_move`; panel `detail` |
| Deshacer una conciliación | `unmatch_bank_move` y `Db::remittance_uncollected` |
| Trabajo en red y carpeta compartida | `ChainOp::ImportBank`, `MatchBank`, `UnmatchBank`, `BankExpense` y `Reconcile` (permiso «Gastos») |
| Cuentas: nombre, IBAN, saldo inicial y saldo según extracto | pantalla Bancos y `bank_balance` |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Formatos de Norma 43 | Solo leía los registros 22 y 23. Un fichero con varias cuentas metía todos los movimientos en una (B1). Un fichero cortado se importaba a medias (B2). |
| Signos | En Norma 43, la clave debe/haber era correcta. En Excel/CSV se perdía el signo con «−12,50» o «(12,50)». Con columnas «Importe cargo» e «Importe abono», los cargos entraban como abonos (B4, B5). |
| Fechas | «05/11/26» se leía como el año 26 (B6). |
| Céntimos | «1,234.56» se leía como 1,23 € (B4). Norma 43 va en céntimos enteros, y los duplicados se comparan en céntimos. |
| Importaciones repetidas | Dos movimientos iguales del mismo día en un extracto se quedaban en uno (B3). Dos puestos que importaban el mismo extracto lo duplicaban (B10). |
| Coincidencias ambiguas | Con dos cobros posibles, o un cobro para dos movimientos, se elegía el primero (B7). |
| Devoluciones | Los recibos SEPA devueltos ya se conciliaban. Las devoluciones a pacientes por transferencia, no (B8). |
| Deshacer conciliaciones | No deshacía lo que la conciliación había cambiado (B9). |
| Concurrencia | La conciliación a mano no comprobaba nada en el servidor (B10). |
| Anchura a 1280 px | La fecha salía cortada en la tabla, y no había forma de poner el IBAN ni de añadir otra cuenta (B11). |

## Defectos demostrados y corregidos
- **B1. Norma 43 con varias cuentas.**
  - **Ahora:** se lee el registro 11 y se importa la cuenta con el IBAN de la cuenta elegida. Sin IBAN, el fichero tiene que ser de una sola cuenta; si trae más, se pide el IBAN.
  - **Prueba:** `bank::tests::norma43_with_several_accounts_and_control_totals`.
- **B2. Norma 43 cortado o dañado.** En cualquiera de estos casos no se importa nada:
  - los totales del registro 33 no cuadran con los movimientos;
  - falta el registro 33 de una cuenta;
  - hay un movimiento cortado.
  - **Pruebas:** `norma43_with_several_accounts_and_control_totals` y `bank::tests::a_cut_norma43_file_is_not_imported`.
- **B3. Movimientos iguales.** Al importar se cuenta cuántos de cada había: dos transferencias de 50 € del mismo día son dos, y reimportar no repite ninguna.
  - **Prueba:** `bank::tests::identical_moves_in_a_statement_are_all_kept_and_not_reimported`.
- **B4. Importes de Excel/CSV.** `parse_money` admite:
  - el separador decimal que va último;
  - miles repetidos;
  - el signo al final, entre paréntesis o tipográfico.

  Entre las cifras no admite nada que no sea separador, así que una fecha no pasa por un importe.
  - **Prueba:** `bank::tests::statement_amounts_keep_their_sign_and_cents`.
  - **Demostración con la función anterior (E-010):** «1,234.56» → 1,23456; «(12,50)» y «−12,50» → +12,50; «12,50-» y «1.234.567» → fila perdida.
- **B5. Cargo y abono en columnas.** Si existen las dos columnas, se usan, aunque una se llame «Importe…». Si no hay ninguna columna de importe, se avisa; antes salían movimientos de 0 €.
  - **Prueba:** `bank::tests::statement_tables_are_read_whole_or_not_at_all`.
- **B6. Filas que no se leen y años de dos cifras.**
  - Si una fila con fecha o importe no se puede leer, no se importa nada y se dice cuál (antes se saltaba sin avisar).
  - Las fechas tienen que estar entre 1990 y 2100. «05/11/26» es el 5/11/2026; `chrono` con `%Y` lo leía como 0026, demostrado en E-010.
  - Las filas de totales sin fecha se ignoran.
  - **Prueba:** `statement_tables_are_read_whole_or_not_at_all`.
- **B7. Coincidencias dudosas.**
  - **Ahora:** la conciliación automática solo concilia lo que tiene una única posibilidad que ningún otro movimiento pendiente comparte. Lo dudoso queda marcado («— dudoso: elija con qué —») y la clínica elige entre lo que no está ya conciliado.
  - **Pruebas:** `bank::tests::ambiguous_matches_are_left_for_the_user` y `ui_tests::a_doubtful_bank_move_is_chosen_by_hand_and_can_be_undone`.
- **B8. Anulaciones y devoluciones.**
  - Un cobro anulado y su anulación no se concilian: antes se elegía el anulado si se había apuntado antes.
  - La devolución a un paciente por transferencia o Bizum se concilia con su cargo. Antes solo cabía crear un gasto, que la contaba dos veces.
  - **Prueba:** `bank::tests::refunds_are_reconciled_and_cancelled_payments_are_not`.
- **B9. Deshacer.** La conciliación anota si marcó el gasto como pagado o la remesa como cobrada (`set_by_match`). Al deshacer:
  - el gasto vuelve a pendiente;
  - la remesa vuelve a «generada»: sus cobros se anulan con otro en negativo, sin borrarlos, y el saldo de los pacientes vuelve a lo de antes;
  - lo que ya estaba pagado o cobrado antes de conciliar sigue así.
  - **Pruebas:** `bank::tests::undoing_a_reconciliation_restores_what_it_changed` y `bank::tests::undoing_a_remittance_match_uncollects_it`.
- **B10. Concurrencia.**
  - **Antes:** la importación y la conciliación a mano se hacían en el puesto y se enviaban.
    - Dos puestos que importaban el mismo extracto lo duplicaban: el servidor pasaba de 3 a 7 movimientos con 2 importados por 2 puestos (demostración en E-010).
    - El panel ofrecía cobros ya conciliados con otro movimiento, y no comprobaba importes.
  - **Ahora:** son operaciones del servidor, o de la carpeta compartida con el fichero bloqueado, con el permiso «Gastos». El servidor rechaza:
    - lo que ya está conciliado;
    - lo que no cuadra;
    - el cobro anulado;
    - deshacer dos veces;
    - un segundo gasto con el mismo cargo.
  - Las entradas están acotadas: hasta 50.000 movimientos y textos de 500 letras.
  - **Pruebas:** `server::tests::a_statement_imported_and_reconciled_by_two_stations_at_once_counts_once` y `shared::tests::a_statement_imported_by_two_stations_counts_once_in_the_shared_folder`.
- **B11. Pantalla.**
  - A 1280 px el panel lateral es más estrecho y las columnas están reequilibradas, así que la fecha se lee entera.
  - Se pueden editar el nombre y el IBAN (con aviso si no es válido) y añadir cuentas.
  - Los botones de las posibilidades ajustan el texto al panel.
  - **Capturas:** `capturas/f1_area4_bancos_1280x800.png` y `capturas/f1_area4_bancos_1280x720.png`.

## Rendimiento
Calcular los movimientos dudosos se hace en cada fotograma de la pantalla. La primera versión recorría todos los cobros por cada movimiento pendiente: unos 700 ms con 100.000 cobros y 300 movimientos.

Con índices por importe y por día de tarjeta baja a 12–18 ms (medido, E-010). Queda como prueba con un límite holgado: `bank::big_bank_tests::finding_doubtful_moves_in_a_big_clinic_is_quick`.

## Límites
- Un importe con un único separador sin decimales («1.500») se sigue leyendo como 1,5: es ambiguo y los bancos exportan con decimales («1.500,00»). Se deja anotado.
- La liquidación del datáfono se busca por la suma exacta del día. Si el banco descuenta la comisión, se concilia a mano con «Otro ingreso» o se crea el gasto de la comisión.
- El panel no deja elegir varios cobros para un movimiento que no sea una liquidación de tarjeta (por ejemplo, una transferencia que paga dos recibos). El servidor ya lo admite (`Match::Payments` con la suma comprobada); falta la selección múltiple en pantalla y queda anotado.

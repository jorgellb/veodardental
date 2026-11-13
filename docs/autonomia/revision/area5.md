# Área 5: informes

Revisado el 13/11/2026.

## Flujos inventariados
| Informe o total | Dónde |
|---|---|
| Pendientes de cobro (deuda por paciente) | Informes › Pendientes de cobro |
| Libro diario de tratamientos realizados | Informes › Libro diario |
| Producción y liquidación de honorarios por colaborador; liquidación imprimible | Informes › Producción; `settlement` y `settlement_html` |
| Cobros y producción de los últimos 12 meses | Informes › Ingresos |
| Presupuestos pendientes, captación y consultas | Informes |
| Cobros del periodo por forma de pago | Cobros |
| Facturado, rectificativas y realizado sin facturar | Facturas |
| Caja: efectivo esperado y arqueo | Caja; `cash_expected` |
| Cobrado hoy (del centro) | Inicio |
| Mutuas: pendiente de liquidar y de cobro | Mutuas; `insurer_summary` |

## Qué entra en cada total
Está definido en `src/informes.rs` y se muestra en Informes › Resumen económico.

| Total | Definición |
|---|---|
| Cobrado | Cobros con fecha en el periodo, ambos días incluidos, del centro elegido. Incluye las remesas. Las anulaciones y devoluciones (a pacientes y recibos devueltos) restan el día en que se hacen. El filtro de profesional no les afecta. |
| Producción | Tratamientos realizados en el periodo, en el centro donde se hicieron y del profesional elegido: lo que paga el paciente más el baremo de la mutua. Lo previsto no entra. |
| Facturado | Facturas por fecha de expedición; las rectificativas, en negativo. No tienen centro ni profesional. |
| Deuda a una fecha | Por paciente, lo realizado hasta ese día menos lo cobrado hasta ese día. Lo que deben los deudores y los anticipos se dan por separado: lo uno no compensa lo otro. |
| Mutuas | Liquidado (por fecha de la liquidación), cobrado (por fecha de cobro), pendiente hoy y sin liquidar (baremo de los actos del periodo). |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Cuadre independiente con cobros, caja, facturas y rectificativas | Conjunto sintético de noviembre con totales calculados a mano. Incluye dos centros, dos profesionales, una mutua, una anulación, una devolución, un recibo devuelto, dos facturas con una rectificativa, y cobros en el último segundo de octubre y el primero de diciembre. Cada total cuadra al céntimo, y las formas de pago suman el neto. |
| Cobros por forma de pago | Las casillas no sumaban el total (I1). |
| Mutuas en la producción | No contaba lo que paga la mutua (I2). |
| Comisión de los colaboradores | Era solo sobre lo que paga el paciente, sin decirlo y sin poder cambiarlo (I3). |
| Filtros por centro y profesional | No había (I4). |
| Fechas de corte | La deuda solo era la de hoy (I5). |
| Rendimiento de «Pendientes de cobro» | Calculaba el saldo de cada paciente recorriendo todos los tratamientos y cobros (I6). |
| Zona horaria | Las fechas son las locales del ordenador que registra: un servidor en otra zona fecharía con la suya, sin avisar (I7). |
| Caja | `cash_expected` = fondo inicial + cobros en efectivo de la sesión + movimientos. Las anulaciones en efectivo restan: van en negativo a la caja abierta. Sin defecto, por lectura del código (`cancel_payment` → `add_payment` con la caja abierta). |

## Defectos demostrados y corregidos
- **I1. Cobros: las formas de pago no sumaban el total.**
  - **Antes:** las casillas eran las cinco formas de la clínica. Lo cobrado por remesa (domiciliación) entraba en «Total cobrado» pero en ninguna casilla.
  - **Ahora:** aparece la casilla «Remesas» cuando hay remesas.
  - **Prueba:** `screens::cobros::tests::the_payment_methods_add_up_to_the_total`.
- **I2. Producción sin la mutua.** El libro diario, la producción por colaborador y el gráfico de ingresos solo sumaban lo que paga el paciente: un acto cubierto por la mutua (copago 0) figuraba como 0 €.
  - **Ahora:** se da la parte del paciente y la de la mutua (el baremo), y el total.
  - **Prueba:** `informes::tests::production_matches_the_operations`.
- **I3. Base de la comisión.**
  - **Ahora:** la comisión sigue siendo sobre lo que paga el paciente, que es lo que se pagaba (D-12), pero la pantalla y la liquidación impresa lo dicen. Cada colaborador puede marcar «también sobre lo que paga la mutua».
  - **Prueba:** `informes::commission_tests::the_commission_counts_the_insurer_part_only_if_marked`.
- **I4. Filtros por centro y profesional.**
  - **Ahora:**
    - los tratamientos guardan el centro donde se hacen (`MedEntry::center_id`, el del puesto al apuntarlos o al marcarlos como hechos);
    - libro diario, producción, ingresos y resumen se filtran por centro;
    - libro diario y resumen, también por profesional.
  - Los tratamientos de antes cuentan en el centro principal, como en el resto del programa.
  - **Pruebas:** `informes::tests::*` y `ui_tests::the_economic_summary_shows_its_totals_and_filters`.
- **I5. Deuda a una fecha.**
  - **Ahora:** «Pendientes de cobro» tiene «Deuda a» y dice aparte los anticipos.
  - **Prueba:** `informes::tests::debt_at_a_cut_off_date`, a tres fechas.
- **I6. Rendimiento.**
  - **Medida en E-011:** 368 ms con 5.000 pacientes y 100.000 cobros, y crece con el número de pacientes.
  - **Ahora:** una sola pasada (`balances_at`), 2,6 ms, con el mismo resultado.
- **I7. Zona horaria.**
  - **Ahora:** el servidor dice su diferencia con UTC al conectar, y el puesto avisa si no es la suya.
  - **Prueba:** `server::tests::a_station_is_warned_when_the_server_has_another_time_zone`.

## Límites
- Los cobros no son de un profesional y las facturas no tienen centro: esos filtros no les afectan, y la pantalla lo dice.
- Una liquidación de mutua cobrada en varios pagos guarda solo la fecha del último. La pantalla solo registra el cobro completo, así que hoy no ocurre.
- El servidor en la nube (fase 6) tendrá que configurarse con la zona horaria de la clínica: el aviso lo detecta, pero no lo corrige.

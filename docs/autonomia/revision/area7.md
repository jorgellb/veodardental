# Área 7: ortodoncia

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Casos: alta, aparato, clase, meses, inicio, honorarios, diagnóstico y objetivos | src/screens/ortodoncia.rs; `OrthoCase` en src/extra.rs |
| Fases: estudio, activo, retención, finalizado y abandonado | `set_ortho_state` |
| Controles con higiene, y recall del siguiente | `add_ortho_visit` |
| Evolución: progreso, próximo control y porcentaje de higiene | `OrthoCase::progress`, `next_control` y `hygiene_pct` |
| Cobro con cuotas | `ortho_billing` y `ChainOp::PayInstallment` |
| Permisos: datos clínicos; el servidor no los manda ni los acepta de quien no tiene permiso clínico | `vista::CLINICAL_LISTS` |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Controles a la vez en dos gabinetes | Se perdía uno (T1). |
| Fases | El cambio de fase no dejaba historia (T2). |
| Retención y fin | Un caso finalizado seguía recordando controles y admitiendo controles nuevos (T3). |
| Quién lo lleva | Un caso nuevo siempre era del colaborador nº 3 y no se podía cambiar (T4). |
| Cuotas y cobros | Los honorarios eran un número suelto, sin relación con el presupuesto ni con lo cobrado (T5). |
| Permisos | Correctos: los casos son datos clínicos (comprobado por `server::tests` de vista clínica). |
| Fechas | Los controles se fechan el día en que se registran. El progreso y el próximo control salen de las fechas. |
| Anchura a 1280 px | La fase tapaba «Próximo control» en la lista (T6). |

## Defectos demostrados y corregidos
- **T1. Pérdida de controles.**
  - **Antes:** los controles eran una lista sin identificador dentro del caso, y al combinar dos puestos ganaba la lista del último.
  - **Demostración:** prueba en rojo antes del arreglo, con 5 controles en vez de 6; se perdía el «Cambio de arco» del primer gabinete.
  - **Ahora:** `visits` y `phases` son listas a las que solo se añade (`shared::APPEND_ONLY`): se juntan las de los dos.
  - **Prueba:** `server::tests::two_ortho_controls_recorded_at_once_are_both_kept`.
- **T2. Historia de fases.**
  - **Ahora:** cada cambio queda en `phases` con fecha y usuario, y se ve en el caso.
  - **Prueba:** `extra::tests::ortho_phases_are_kept_and_a_finished_case_stops_its_controls`.
- **T3. Caso finalizado o abandonado.**
  - **Ahora:**
    - se quitan sus recalls de control pendientes (salvo que el paciente tenga otro caso abierto);
    - no admite controles hasta que se le cambia de fase;
    - la historia sigue entera.
  - **Prueba:** la misma.
- **T4. Quién lo lleva.**
  - **Ahora:** al crear el caso, el ortodoncista activo de la clínica o, si no hay, el doctor del paciente; se puede cambiar en el caso.
  - Antes, sin paciente abierto el botón no hacía nada; ahora avisa.
  - **Prueba:** la misma.
- **T5. Cobro con cuotas.**
  - **Ahora:**
    - el caso se enlaza con un presupuesto del paciente y muestra cuotas pagadas y la próxima;
    - la cuota se cobra desde el caso con la operación del servidor de siempre (`PayInstallment`), así que no se cobra dos veces.
  - **Prueba:** `extra::tests::an_ortho_case_is_charged_through_its_budget_installments`.
- **T6. Pantalla.**
  - **Ahora:** el próximo control va en su propia línea, y solo en casos activos o en retención.
  - **Capturas:** `f1_area7_ortodoncia_1280x{800,720}.png` y `f1_area7_ortodoncia_cobro_1280x800.png`.

## Ajuste de una prueba existente
`booking::tests::web_page_books_a_free_slot` tomaba la segunda hora del primer día libre, y a última hora de la tarde hoy queda una sola. Falló en la batería de las 19:40; ahora toma el primer día con dos horas. Es el mismo caso que en el área 6.

## Límites
- Los honorarios del caso siguen siendo un dato informativo. Lo que se cobra es el presupuesto enlazado.
- Dos gabinetes que cambian a la vez la fase dejan las dos en la historia, y como fase actual la del último que guarda.

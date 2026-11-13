# Área 3: odontograma, periodontograma e historial

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Odontograma: 13 herramientas, piezas FDI adultas (11–48) y temporales (51–85), caras M, D, O/I, V y L | `apply_tool` en src/screens/odontograma.rs |
| Puentes en la misma arcada; prótesis removible sobre las piezas ausentes | `apply_tool` |
| Evolución por visita (navegación por fechas) | `visit_nav` |
| Panel de la pieza: realizar, borrar e imágenes de la pieza | `side_panel` |
| Historial de la ficha: menú contextual de realizar y borrar | src/screens/pacientes.rs |
| Al realizar se gasta el material (lotes) y se crea o alarga el recall; se cierra el presupuesto cuando todo está hecho | `realize_entry` y `after_realized` |
| Borrar un tratamiento: no si está facturado o liquidado a la mutua; devuelve el material | `delete_entry`, de la ronda anterior |
| Periodontograma: 6 puntos por pieza (profundidad 0–15, recesión 0–15, sangrado, placa), movilidad, furca, estadísticas sin piezas ausentes y comparación con el sondaje anterior | — |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Dentición adulta y temporal, piezas y superficies | Correctas. Los puentes exigen la misma arcada. |
| Evolución por visita | Correcta. |
| Coherencia entre dibujo, presupuesto y tratamiento hecho | Marcar como hecho en el odontograma lo que estaba previsto (de un presupuesto) creaba otro tratamiento: el previsto seguía pendiente y se contaba dos veces (O4). |
| Código de obturación automático | Suponía que 0301, 0302 y 0303 son obturaciones de 1, 2 y 3 caras; con un catálogo importado podía cambiar a otro tratamiento (O3). |
| Deshacer | Un tratamiento marcado como hecho por error solo se podía borrar, perdiendo su presupuesto. Facturado o liquidado se protege. Faltaba volver a previsto devolviendo el material (O5). |
| Concurrencia sin romper cobros, facturas ni stock | Dos gabinetes que marcaban a la vez como hecho el mismo previsto gastaban el material dos veces (O6). |
| Periodontograma | Estadísticas correctas. La recesión no admite margen gingival por encima del límite amelocementario (valores negativos). Queda anotado como limitación del formato, sin defecto demostrado. |

## Defectos demostrados y corregidos
- **O4.** Odontograma y presupuesto descoordinados.
  - **Ahora:** `Db::chart_entry` completa el previsto de esa pieza y tratamiento (primero el presupuestado) en vez de crear otro.
  - **Pruebas:** `stock::tests::marking_a_planned_treatment_done_on_the_chart_completes_it` y `ui_tests::the_chart_completes_planned_treatments_and_can_undo_them`.
- **O3.** Código de obturación automático.
  - **Ahora:** `Db::filling_code` solo cambia entre 0301, 0302 y 0303 si los tres son obturaciones en el catálogo de la clínica.
  - **Prueba:** `stock::tests::the_filling_code_follows_the_faces_only_within_fillings`.
- **O5.** Deshacer un tratamiento hecho por error.
  - **Ahora:** `Db::unrealize_entry` («↶ Volver a previsto» en el odontograma y en el historial) devuelve el material a su lote, mantiene el presupuesto (y lo reabre si se cerró con él) y deja registro. Facturado o liquidado, no.
  - **Prueba:** `stock::tests::a_treatment_marked_done_by_mistake_goes_back_to_planned`.
- **O6.** Material gastado dos veces.
  - **Ahora:** en red, el servidor descarta los consumos nuevos de un tratamiento que ya tiene gastado (`drop_double_consumption`). En carpeta compartida lo hace el puesto al combinar, recalculando las existencias desde los movimientos. Deshacer y volver a hacer sí vuelve a gastar.
  - **Pruebas:** `server::tests::the_same_treatment_done_on_two_stations_spends_its_material_once` y `shared::tests::the_same_treatment_done_on_two_stations_spends_once_in_the_shared_folder`.

## Límites
- El recall que se creó o alargó al realizar no se deshace al volver a previsto: un aviso de revisión de más es inocuo y quien lo atiende lo ve.
- Margen gingival por encima del límite amelocementario en el periodontograma: necesitaría cambiar el formato de los datos y no se ha demostrado que se use. Se deja anotado.

# Área 2: recetas y vademécum

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Receta nueva desde la ficha (Documentos › Recetas), con líneas del vademécum de la clínica o de la lista oficial de la AEMPS (Nomenclátor) | — |
| Avisos de seguridad | `rx_warnings`: alergia a penicilina y a AINE, posible alergia por nombre, y AINE en paciente anticoagulado, embarazada o hipertenso con tratamiento largo |
| Receta imprimible | `export_html` |
| Receta oficial por plataforma homologada: copiar datos y anotar el CVE | — |
| Receta oficial por conexión directa (SREP): firma CAdES, repositorio, estado en farmacias | — |
| Datos obligatorios | `erx::missing_data` (plataforma) y `erx::direct_missing` (SREP): paciente, prescriptor, CN o principio activo, forma, dosificación, posología, duración, envases y límites de estupefacientes |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Identidad del prescriptor | Se usaba el colaborador del usuario sin comprobar si puede recetar. Una higienista quedaba como prescriptora y la receta se imprimía (P1). |
| Identidad del paciente | Nombre, DNI/NIE y fecha de nacimiento exigidos para la receta oficial. Correcto. |
| Datos obligatorios, unidades y dosis | Posología estructurada, días, envases y límites de estupefacientes comprobados. Correcto. No se cambia ninguna dosis automáticamente. |
| Alergias e interacciones | Las reglas solo reconocían el grupo del vademécum de la clínica: los medicamentos de la lista oficial no daban avisos (P2). |
| Vigencia de las fuentes | Se mostraba la fecha de la lista oficial, pero no se avisaba si era antigua (P3). |
| Avisos revisables por el profesional | Se mostraban, pero se podía imprimir o emitir sin dejar constancia de su revisión (P4). |
| Trazabilidad | Una receta emitida se podía editar y borrar, y en red cualquier puesto podía cambiarla (P5). |

## Defectos demostrados y corregidos
- **P1.** Prescriptor no habilitado.
  - **Ahora:** `Db::prescriber_problem` y `Db::prescribers` (fuentes en docs/normativa.md). Hay un selector «Prescriptor» que solo ofrece a quien puede recetar, y no se imprime ni se emite sin un prescriptor válido.
  - **Pruebas:** `finance::tests::only_a_dentist_or_doctor_with_licence_prescribes` y `ui_tests::a_prescription_needs_a_prescriber_and_reviewed_warnings`.
- **P2.** Avisos que no llegaban a los medicamentos de la lista oficial.
  - **Ahora:** las líneas guardan el código ATC (J01C penicilinas, M01A AINE) y los avisos existentes se aplican también a ellas.
  - **Prueba:** `finance::tests::official_list_drugs_get_the_same_warnings`.
- **P3.** Lista oficial antigua.
  - **Ahora:** se avisa si tiene más de 60 días o no tiene fecha legible (D-07).
  - **Prueba:** `nomenclator::tests::an_old_official_list_is_flagged`.
- **P4.** Revisión de avisos sin constancia.
  - **Ahora:** «He revisado los avisos», solo para un profesional que pueda recetar, anota quién y cuándo, con registro de cambios.
  - Sin la revisión no se imprime, y en los datos obligatorios de la receta oficial falta «Revisar los avisos». Un aviso nuevo pide revisar otra vez.
  - **Pruebas:** `finance::tests::warnings_are_reviewed_before_issuing` y la prueba de pantalla de P1.
- **P5.** Receta emitida modificable.
  - **Ahora:**
    - en pantalla queda bloqueada y sin «Borrar»;
    - `Db::delete_prescription` la rechaza;
    - en red y en carpeta compartida no se cambia: solo su estado en las farmacias, el código de plataforma y el paso a otra ficha al fusionar (`firmado::keep_issued_rx`).
  - **Prueba:** `finance::tests::an_issued_prescription_is_kept_as_issued`.

Además, el editor de la receta se salía de la ventana a 1280 píxeles por la fila de alertas. Ahora la fila se ajusta (captura `f1_area2_receta_1280x800.png`).

## Límites y pendientes externos
- Las reglas de aviso no se amplían: las alertas nuevas son de la fase 5 (T-F5-07). Las existentes necesitan revisión profesional (B-16).
- La higienista se reconoce por la especialidad del colaborador (D-08). Una clínica con otro nombre para esa especialidad debe usar las palabras «higienista», «auxiliar» o «protésico».

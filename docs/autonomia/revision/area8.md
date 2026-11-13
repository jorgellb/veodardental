# Área 8: IA de radiografías

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Agentes: proveedor, modelo, clave, gratuito o de pago; modos «solo gratuitos», «solo de pago» y «primero gratuitos» | `IaConfig` y `chain` en src/ia.rs; Configuración › Inteligencia artificial |
| Preparación: la imagen reducida y vuelta a comprimir en JPEG, sin metadatos; tipo, piezas y, si se quiere, la edad | `jpeg_for_ai`, `Request` y `prompt` |
| Llamada con tiempo máximo por agente, errores claros y paso al siguiente agente si falla | `call`, `send` y `analyze` |
| Lectura de la respuesta: JSON flexible, piezas FDI válidas, confianza acotada y respuesta vacía rechazada | `parse_analysis` y `tooth_of` |
| Informe guardado en la ficha, con agente, modelo, intentos y usuario | `IaReport`; panel bajo el visor de imágenes |
| Paso de las propuestas al plan de tratamiento, y resumen a las notas | `plan_from_ia` y `report_view` |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Qué sale de la aplicación | Solo los píxeles (JPEG nuevo, sin metadatos), el tipo de imagen, las piezas y la edad si se activa. Sin nombre ni identificadores, comprobado en `ui_tests::ai_analysis_falls_back_and_adds_to_plan`. Una radiografía con el nombre impreso en la imagen sí lo lleva: el aviso decía lo contrario (A4). |
| Autorización | Los informes son datos clínicos (el servidor no los da ni los acepta sin permiso clínico). No había revisión del profesional (A1). |
| Tiempos máximos | Cada agente tiene su tiempo máximo (20 a 300 s) y conexión de 10 s. |
| Errores y respuestas incompletas | Se rechazan la respuesta vacía, la que no es JSON y la cortada; se descartan las piezas no FDI; se pasa al agente siguiente. Pruebas existentes: `reads_loose_answers` y `falls_back_to_next_free_agent_and_never_pays`. |
| Límites de coste | Cada respuesta tiene un máximo de tokens, y en «solo gratuitos» nunca se paga. No había límite de análisis de pago (A3). |
| Trazabilidad del modelo | Agente, modelo, intentos, usuario y hora. Faltaba la versión de las instrucciones (A5). |
| Revisión antes de incorporarlo | Las propuestas venían marcadas y pasaban al plan sin revisión registrada; el resumen se copiaba a las notas igual (A1, A2). |

## Defectos demostrados y corregidos
- **A1. Revisión del profesional.**
  - **Ahora:** cada análisis es un borrador hasta que un profesional colegiado lo da por bueno o lo descarta, como para recetar. Queda quién y cuándo, y no se cambia después.
  - **En el servidor:** solo cuenta la revisión de un usuario así, con su nombre (`ia_reports` en `save`).
  - Hasta la revisión no pasa nada al plan ni a las notas. Un análisis ya usado en el plan no se borra: es la constancia de dónde salió.
  - **Pruebas:** `ia::tests::an_analysis_is_a_draft_until_a_professional_reviews_it` y `server::tests::only_a_licensed_professional_reviews_an_ai_analysis` (la higienista no lo revisa; el nombre es el de quien entró; una revisión hecha no se cambia).
- **A2. Propuestas.**
  - **Antes:** venían marcadas, y un clic las pasaba todas al plan.
  - **Ahora:**
    - se marcan una a una;
    - las de piezas que no salen en la imagen se señalan;
    - cada tratamiento añadido dice «Propuesto por IA (agente, modelo) y revisado por …».
  - **Prueba:** la misma de A1, y `ui_tests::ai_analysis_falls_back_and_adds_to_plan` con el flujo nuevo: sin revisar no añade nada; revisado y marcado, sí.
- **A3. Coste.**
  - **Ahora:** máximo de análisis de pago al mes (`max_paid_month`, 20 por defecto, configurable; 0 = ninguno). Pasado el máximo, los agentes de pago no se consultan y queda dicho en los intentos.
  - **Prueba:** `ia::tests::paid_agents_are_not_called_past_the_monthly_limit`, con un servicio de prueba que cuenta las llamadas.
- **A4. Aviso de privacidad.**
  - **Ahora:** «sin nombre ni datos del paciente en el fichero; si la radiografía lleva el nombre impreso, recórtela antes».
- **A5. Trazabilidad.**
  - **Ahora:** cada análisis guarda la versión de las instrucciones (`PROMPT_VERSION`).

## Pruebas existentes ajustadas
`ui_tests::ai_analysis_falls_back_and_adds_to_plan` pulsaba «Añadir al plan» sin revisión. Ahora comprueba que así no añade nada, y que tras la revisión de una profesional y marcar la propuesta, sí. Es el comportamiento nuevo, A1 y A2.

## Límites y pendientes
- Las claves de los agentes van con los datos de la clínica, como la contraseña del correo. Que los secretos no lleguen a los puestos es la tarea de la carpeta compartida y los secretos de la fase 4: allí queda.
- No se detecta texto impreso dentro de la imagen (sería reconocimiento de texto): se avisa para recortarla.
- El tiempo total de un análisis es el máximo por agente multiplicado por el número de agentes activos.

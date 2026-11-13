# Área 6: comunicaciones

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Mensajes del día según plantillas: recordatorio de cita, revisión, cumpleaños y seguimiento de presupuesto | `generate_messages` y `push_msg` en src/comms.rs |
| Envío por correo desde el programa, manual o automático una vez al día en un solo ordenador | `start_outbox`, `outbox_tick` y `run_daily` en src/screens/comunicaciones.rs; `mail::send_tracked` |
| WhatsApp automático (Meta o pasarela): recordatorio con respuesta «sí» o «no», estados de entrega y avisos repetidos | src/whatsapp.rs (`wa_due`, `wa_prepare`, `wa_sent`, `wa_apply`) |
| SMS y WhatsApp manual: se copia o se abre y se marca como enviado | `send` en la pantalla Comunicaciones |
| Aviso de huecos libres a la lista de espera | src/slot_offer.rs |
| Plantillas de mensajes y de documentos | Comunicaciones › Plantillas |
| Consentimientos del paciente | Ficha › Filiación |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Consentimiento | Protección de datos firmada más el canal (SMS, correo o WhatsApp). Si el paciente no acepta el canal de la plantilla, va por SMS solo si acepta SMS. No había baja por finalidad (C4). |
| Bajas | No había forma de pedir la baja respondiendo al mensaje (C5). |
| Horarios y festivos | El correo automático solo sale entre las horas configuradas, y el WhatsApp automático dentro de su horario y el día laborable anterior. Los recordatorios por correo y SMS no seguían los días laborables (C3). |
| Reintentos | Correo: hasta 3 intentos. WhatsApp: no se reintenta y queda una tarea para recepción. |
| Fallos parciales y resultados inciertos | Un envío cortado sin respuesta del servidor se trataba como «no enviado» (C2). |
| Colas y duplicados | `drop_auto_duplicates` quita los recordatorios repetidos por dos puestos. Los correos se podían enviar dos veces desde dos puestos (C1). |
| Plantillas | Se mostraban comodines que en un mensaje no se sustituyen, y los datos de salud podían salir en un mensaje (C6). |
| Trazabilidad | Cada mensaje guarda quién lo creó, cuándo, intentos y error. No guardaba quién lo dio por enviado (C7). |

## Defectos demostrados y corregidos
- **C1. Correos enviados dos veces.**
  - **Antes:** cada puesto enviaba los pendientes que veía. Dos puestos que pulsaban «Enviar por correo» a la vez (o el automático y uno manual) mandaban los mismos.
  - **Ahora:** antes de enviar, el puesto reserva los correos con una marca suya (`ChainOp::ClaimEmails`), en el servidor o con el fichero compartido bloqueado, y solo envía los que ha reservado.
  - Si un puesto se cierra a medias, pasados 30 minutos sus correos quedan como «no se sabe si llegó».
  - **Pruebas:** `server::tests::two_stations_sending_emails_at_once_send_each_one_once` y `comms::tests::emails_are_claimed_once_and_uncertain_ones_wait_for_a_decision`.
- **C2. Resultados inciertos.**
  - **Correo** (`mail::send_tracked`) distingue:
    - lo que seguro que no salió (sin conexión, o rechazado con un código), que se reintenta;
    - lo que se cortó sin la confirmación del servidor, que puede haber llegado: queda «¿Llegó?» y no se reintenta solo. Recepción decide «Sí, consta como enviado» o «No llegó: volver a enviar».
  - **WhatsApp**, igual: si la petición salió y se cortó, o si Meta o la pasarela la aceptaron sin identificador, el mensaje queda enviado pero «sin confirmar». No se reenvía, y la respuesta del paciente se recoge por su número; recepción recibe una tarea.
  - **Antes:** el correo se reintentaba (podía llegar dos veces) y el WhatsApp quedaba «No se pudo enviar».
  - **Pruebas:**
    - `mail::tests::a_cut_after_the_data_is_uncertain_and_a_rejection_is_not`: servidor SMTP de prueba que recibe el correo y se corta, otro que lo rechaza con 550, y un puerto cerrado;
    - `whatsapp::tests::network_errors_tell_not_sent_from_uncertain`;
    - `whatsapp::tests::an_uncertain_send_is_not_retried_and_replies_still_count`.
- **C3. Recordatorios tras fines de semana y festivos.**
  - **Antes:** «un día antes» era el día natural anterior. Las citas del lunes se recordaban el domingo, y con la clínica cerrada (el ordenador apagado) no se recordaban nunca.
  - **Ahora:** es el día laborable anterior, como en el WhatsApp automático.
  - **Prueba:** `comms::tests::reminders_skip_weekends_and_holidays`: viernes → lunes, lunes festivo → martes, sin repetir y sin adelantar.
- **C4. Baja por finalidad.**
  - **Ahora:** en la ficha, «Avisos que quiere recibir» (recordatorio de cita, revisión, seguimiento de presupuesto y felicitación). Lo desmarcado no le llega por ningún canal, tampoco por el WhatsApp automático.
  - **Prueba:** `comms::tests::a_patient_can_opt_out_of_one_kind_of_message`.
- **C5. Bajas.**
  - Responder «BAJA» o «STOP» (o «darme de baja», «no quiero recibir…») por WhatsApp:
    - quita el WhatsApp a ese número;
    - queda en el registro de cambios;
    - crea una tarea para recepción;
    - se le confirma si las respuestas automáticas están activas;
    - no anula su cita.
  - Las felicitaciones y los seguimientos de presupuesto por correo terminan con cómo darse de baja. Los recordatorios de cita y de revisión, que son de su atención, no.
  - **Pruebas:** `whatsapp::tests::replying_baja_stops_whatsapp` y `a_patient_can_opt_out_of_one_kind_of_message`.
- **C6. Plantillas.**
  - La lista de comodines depende de la clase de mensaje. Si el texto lleva uno que no se sustituiría (mal escrito, como «[NOMBER]», o de otra clase, como «[HORA]» en una revisión), se avisa: llegaría tal cual al paciente. Antes se ofrecía «[PIEZAS]», que en un mensaje no se sustituye.
  - Los datos de salud (alertas, tratamientos, pendientes y fecha de nacimiento) se quitan de cualquier mensaje (`fill_message`), aunque la plantilla los lleve. En los documentos siguen.
  - **Prueba:** `comms::tests::templates_only_use_placeholders_that_are_filled_and_no_health_data`.
- **C7. Trazabilidad.**
  - **Ahora:** el mensaje guarda quién lo dio por enviado (`sent_by`).

## Ajustes de pruebas existentes
Dos pruebas de pantalla dependían del día y de la hora en que se ejecutan:
- `daily_emails_are_sent_automatically` suponía que solo hay recordatorios «para mañana». Ahora fija sus dos citas como las únicas por venir: un viernes tocan también las del lunes, que es justo el arreglo C3.
- `patient_link_actions_reach_the_agenda` pedía la segunda hora libre del primer día, y a última hora de la tarde hoy ya no quedan. Ahora toma el primer día con dos horas libres.

No se ha cambiado ninguna comprobación.

## Límites
- Las bajas por correo («responda BAJA») llegan al buzón de la clínica y se aplican a mano: el programa no lee el correo entrante.
- El SMS sale por la pasarela o el móvil de la clínica (se copia el texto): no hay confirmación de entrega que tratar.
- Con la pasarela (no oficial), la clínica ve los chats en su propio WhatsApp, y las respuestas que no son de un recordatorio no crean tarea.

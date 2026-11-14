# Para el responsable o delegado de protección de datos (DPO)

Medidas del programa relacionadas con el RGPD y la Ley 41/2002. Las fuentes,
con fecha, están en `docs/normativa.md`. No se afirma que el programa cumpla
el RGPD: eso lo valora usted (B-13).

## 1. Qué datos guarda
- **Identificación y contacto del paciente:** nombre, documento, fecha de
  nacimiento, teléfono, correo y dirección.
- **Datos de salud (artículo 9 del RGPD):**
  - historia, tratamientos, odontograma, periodontograma y ortodoncia;
  - cuestionarios de salud, alergias y medicación;
  - recetas, documentos firmados y consentimientos;
  - imágenes y radiografías, con los informes de IA.
- **Económicos:** presupuestos, facturas, cobros, remesas y mutuas.
- **Del personal:** usuarios y perfiles (Dirección, Doctor, Higienista,
  Recepción) y registro de jornada.
- **Registros de control:**
  - auditoría de cambios;
  - registro de accesos a cada historia;
  - registro de errores, minimizado.

## 2. Dónde están y cómo se protegen
- **En la clínica.** Un ordenador hace de servidor de datos y los puestos se
  conectan a él por TLS 1.3 (`src/server.rs`).
- **Cifrado de los datos, del diario de cambios y de las copias locales:**
  - AES-256-GCM, con claves gestionadas, rotación y clave de recuperación
    guardada aparte (`src/datos_cifrados.rs`, E-039 y E-040);
  - con el cifrado puesto, no se aceptan datos sin cifrar en su lugar.
- **Segunda copia (USB o nube):** cifrada con contraseña, PBKDF2 de 600 000
  vueltas y AES-256-GCM (`src/cifrado.rs`). La restauración se ensaya en una
  carpeta aparte (`tools::tests::a_restore_drill_checks_the_contents_and_the_key_lives_apart`).
- **Imágenes:** están en carpetas que comparten los programas de radiología, y
  cifrarlas allí rompería esos programas. Las protege el cifrado del disco,
  que la puesta en marcha comprueba. Es un límite conocido.
- **Carpeta compartida (modo antiguo, que reparte el fichero entero):**
  - ya no se pueden crear instalaciones nuevas en ese modo;
  - hay un paso comprobado para pasar los datos al servidor (`src/shared.rs`,
    E-034, `shared::tests::a_shared_folder_moves_to_the_server_and_stays_untouched`);
  - las instalaciones que ya existen se conservan hasta una migración autorizada.

## 3. Acceso a la historia (Ley 41/2002, artículo 16)
- **Perfiles:**
  - Recepción ve lo administrativo, no lo clínico (`src/vista.rs`);
  - cada profesional, lo suyo;
  - Dirección, todo.
- **Registro de accesos:**
  - apunta quién consulta y quién modifica cada historia, cuándo, desde qué
    puesto y en qué parte, sin copiar la historia;
  - va encadenado, así que borrar o cambiar una línea se detecta;
  - en red lo escribe el servidor, y solo lo lee Dirección
    (`src/accesos.rs`, E-032 y E-033).
- **Pruebas:**
  - `accesos::tests::the_log_is_chained_and_tampering_shows`;
  - `accesos::tests::changes_tell_which_patient_and_part_without_content`;
  - `server::tests::the_server_records_who_reads_and_changes_each_history`.
- **Decide:** cuánto se conserva el registro de accesos y quién más puede leerlo.

## 4. Derechos del paciente (artículos 15 a 18 y 20)
Todo está en la ficha del paciente, pestaña Filiación (`src/rgpd.rs`, E-033 y E-034).

- **Solicitudes:**
  - se registra quién pide, en calidad de qué (el paciente, su representante
    legal o un autorizado) y con qué lo acredita;
  - el plazo de respuesta es de un mes;
  - un menor de 14 años necesita representante.
- **Acceso y portabilidad:** exportación íntegra en ZIP, con los datos, los
  ficheros y un manifiesto con la huella SHA-256 de cada uno. Solo salen los
  ficheros del propio paciente.
- **Rectificación:** se corrige en la ficha y queda la solicitud.
- **Limitación:** se conservan los datos, pero no se le envían mensajes.
- **Bloqueo:** el paciente sale de las listas, y solo Dirección lo quita
  (también en el servidor).
- **Supresión:** desactivada (`rgpd::SUPPRESSION_ENABLED` es falso). Antes de
  activarla, el programa ya comprueba dos cosas:
  - que hayan pasado al menos cinco años desde el último proceso asistencial;
  - que las facturas se conservan por ley.
- **Pruebas:**
  - `rgpd::tests::requests_have_a_deadline_and_check_who_asks`;
  - `rgpd::tests::the_export_is_complete_traceable_and_private`;
  - `rgpd::tests::restriction_stops_messages_and_only_management_unblocks`;
  - `rgpd::tests::suppression_is_disabled_and_explains_what_must_be_kept`;
  - `server::tests::only_management_blocks_or_unblocks_a_patient_on_the_server`.
- **Decide (B-10):**
  - el plazo de conservación de la clínica: el mínimo estatal es de cinco
    años desde el alta de cada proceso, y la comunidad autónoma puede fijar más;
  - las excepciones;
  - si se activa la supresión, y cómo.

## 5. Lo que sale de la clínica (encargados del tratamiento)
Nada se envía sin que la clínica lo configure con su proveedor. Ninguno está
contratado en este trabajo.

| Servicio | Qué se envía | Dónde |
|---|---|---|
| Correo (SMTP de la clínica) | Lo que se envíe: avisos, órdenes al laboratorio con adjuntos. La orden al laboratorio va de serie con las iniciales y el número de historia; la clínica puede poner el nombre completo (D-30) | `src/mail.rs`, `src/lab_order.rs` |
| WhatsApp Business (Meta) | Recordatorios y mensajes al paciente | `src/whatsapp.rs` |
| SMS (pasarela que contrate la clínica) | Teléfono y texto del mensaje | `src/sms.rs` |
| IA de imágenes (proveedor que elija la clínica, o uno local) | Solo la imagen y, si se quiere, la edad; nunca el nombre | `src/ia.rs` |
| Transcripción de dictados (servicio que elija la clínica, o uno local) | La grabación del dictado; el programa no la guarda | `src/evolucion.rs` |
| IA para resumir dictados (desactivado de serie) | El texto del dictado sin el nombre del paciente ni los identificadores que se hayan dicho; el resumen es una propuesta que el profesional revisa | `src/evolucion.rs` |
| Cloudflare Tunnel (si se publica la página de citas) | Las páginas de cita online y «Mi cita»: datos de esa cita y nombre de pila | `src/tunnel.rs`, `src/portal.rs` |
| AEAT (VERI*FACTU) | Registros de facturación | `src/verifactu.rs` |
| TPV virtual del banco (Redsys) | El pedido, el importe y el concepto del pago; la tarjeta la pide la página del banco, no el programa | `src/pago.rs` |
| Soporte | El informe de errores, minimizado; solo a mano, visto entero antes y con consentimiento | `src/registro.rs` |

- Quien da de baja la recepción de mensajes («BAJA») deja de recibirlos
  (`whatsapp::tests::replying_baja_stops_whatsapp`).
- **Decide:**
  - los contratos de encargado del tratamiento con cada proveedor;
  - si la IA con proveedores externos es admisible y con qué garantías;
  - los textos informativos al paciente.

## 5 bis. Notas de evolución
- Solo entran en la historia cuando el profesional las confirma. Guardan su procedencia (escrito, plantilla, dictado, resumen con IA) y quién las confirmó.
- Del dictado solo queda la huella; el audio no se guarda.
- Una nota confirmada no se cambia ni se borra, ni desde un puesto en red (`server::tests::a_confirmed_evolution_note_cannot_be_changed_by_a_station`): se corrige con otra que la cita.
- Recepción no las recibe. Su lectura queda en el registro de accesos y salen en la exportación del paciente.
- **Decide:** si se puede usar un proveedor externo para transcribir o resumir.

## 5 ter. Campañas (promociones)
- Solo le llegan a quien ha aceptado expresamente recibir promociones, con la fecha (Filiación), además del consentimiento de protección de datos y el del canal. Quien se da de baja de las campañas no las recibe. Las dos cosas se vuelven a mirar al pasar cada mensaje a la cola.
- Al fusionar dos fichas, el consentimiento para promociones solo se conserva si lo tenían las dos; una baja dada en cualquiera de ellas sigue valiendo.
- Cada mensaje dice cómo darse de baja, en el idioma del paciente. Un «BAJA» (o la palabra de su idioma) contestado por WhatsApp lo da de baja.
- La programa Dirección, también en red. Los mensajes los pasa a la cola el servidor, con su hora; ningún puesto marca destinatarios (`server::tests::only_direction_schedules_campaigns_and_the_server_queues_them`). El texto no puede llevar datos de salud.
- Pruebas: `campanas::tests::the_preview_shows_who_and_why_not_and_scheduling_freezes_the_list` y `campanas::tests::queueing_respects_hours_weekdays_daily_limit_and_late_unsubscribes`.
- **Decide:**
  - el texto del consentimiento para promociones;
  - si se puede elegir a quién va una promoción por los tratamientos hechos (son datos de salud) y con qué consentimiento;
  - cuánto se guarda la lista de a quién se le envió cada campaña.

## 5 quater. Grupos de clínicas
- Cada sociedad tiene su propia instalación: sus pacientes no pasan a otra (D-25).
- El grupo solo recibe cifras por centro (producción, cobros, facturación, citas y altas), sin ningún dato de pacientes. Lo comprueba `grupo::tests::the_summary_matches_the_reports_and_carries_no_patient_data`, con una lista cerrada de campos.
- Lo da cada clínica con una clave que crea y retira su Dirección.
- Dentro de una sociedad, los centros comparten pacientes e historia. Un usuario puede limitarse a unos centros para lo que hace (citas, cobros, caja).
- **Decide:** si compartir pacientes entre los centros de una misma sociedad necesita informar al paciente de algo más.

## 6. Minimización: registros, portales, mensajes e IA (T-R7-7)
Todas las pruebas usan datos inventados.
- **Registro de errores:** antes de escribir se quitan correos, teléfonos
  (también escritos por grupos, «612 34 56 78»), documentos, IBAN, rutas y
  nombres. Tiene un tamaño acotado y no sale solo del ordenador.
  - `registro::tests::personal_data_never_reaches_the_log`;
  - `registro::tests::phones_written_in_groups_are_removed_too`;
  - `registro::tests::the_support_report_is_previewed_and_only_sent_with_consent`.
- **Páginas con enlace personal** (cita, presupuesto, documento, recetas,
  pago, encuesta, «Mi espacio», hueco libre): antes de que la persona dé su
  documento y su fecha de nacimiento, enseñan el nombre de pila y lo que trata
  el enlace. Nunca enseñan apellidos, documento, teléfonos, correo, dirección,
  fecha de nacimiento, número de historia o cuenta, ni alertas, alergias,
  antecedentes o notas (`booking::tests::link_pages_show_only_the_first_name_and_what_the_link_is_for`).
  Se miran la página y los datos que pide.
- **Mensajes automáticos** (citas, revisiones, cumpleaños, presupuestos y el
  recordatorio de WhatsApp): sin identificadores ni datos de salud
  (`comms::tests::automatic_messages_carry_no_identifiers_or_health_data`).
- **IA de imágenes:** solo viajan los píxeles, sin los metadatos del fichero,
  con el tipo, las piezas y, si se quiere, la edad. El estudio DICOM no se
  envía, porque su cabecera lleva la identidad
  (`ia::tests::only_pixels_and_non_identifying_data_reach_the_agent`).
  **Límite:** un texto quemado en la propia imagen (el nombre que algunos
  equipos imprimen en la radiografía) viaja con los píxeles.
- **Dictados para resumir con IA:** sin el nombre del paciente ni los
  identificadores que se hayan dicho
  (`evolucion::tests::identifiers_said_in_a_dictation_do_not_reach_the_ai`).
- **Laboratorio:** la orden va de serie con iniciales y número de historia
  (D-30), con las alergias, que hacen falta para elegir el material.
- **Mediciones del piloto:** tiempos y pasos sin datos de pacientes
  (`medidas::tests::measures_carry_no_patient_data`).

## 7. Registro de jornada (personal)
- Entradas, salidas y pausas con la hora del servidor.
- Las correcciones conservan el original y dicen por qué.
- Cada persona ve lo suyo y Dirección todo (`src/jornada.rs`, E-035 y E-036).
- La revisión laboral está en B-26.

## 8. Lo que no se ha hecho
- No se han abierto ni tratado datos reales de pacientes.
- No se ha suprimido nada.
- Los plazos y las excepciones no se han activado sobre datos reales.

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
| Correo (SMTP de la clínica) | Lo que se envíe: avisos, órdenes al laboratorio con adjuntos | `src/mail.rs` |
| WhatsApp Business (Meta) | Recordatorios y mensajes al paciente | `src/whatsapp.rs` |
| SMS (pasarela que contrate la clínica) | Teléfono y texto del mensaje | `src/sms.rs` |
| IA de imágenes (proveedor que elija la clínica, o uno local) | Solo la imagen y, si se quiere, la edad; nunca el nombre | `src/ia.rs` |
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

## 6. Minimización en los registros
- **Registro de errores:** antes de escribir se quitan correos, teléfonos,
  documentos, IBAN, rutas y nombres. Tiene un tamaño acotado y no sale solo
  del ordenador.
  - `registro::tests::personal_data_never_reaches_the_log`;
  - `registro::tests::the_support_report_is_previewed_and_only_sent_with_consent`.
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

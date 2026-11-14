# Para la auditoría de seguridad externa

Lo que el auditor necesita para fijar el alcance y empezar. Es una revisión
propia con revisiones automáticas de cada commit; no sustituye la auditoría
(B-13).

## 1. Arquitectura
- **Programa de escritorio** en Rust (egui), para Linux, Windows y macOS.
- **Servidor de datos** en un ordenador de la clínica (`src/server.rs`):
  - HTTP sobre TLS 1.3 (rustls). El servidor tiene una identidad Ed25519
    propia; el puesto la fija la primera vez, tras un reto firmado con la clave
    de la clínica, y después no acepta otra;
  - cada petición lleva la clave de la clínica (`X-Clave`) y, salvo el acceso,
    una sesión de usuario (`X-Sesion`), que caduca a las 24 horas sin uso;
  - el servidor aplica los permisos: el puesto no elige el usuario ni el
    puesto que se apuntan;
  - sincronización registro a registro, con revisión y comprobación de cómo
    estaba cada cosa (`src/sync.rs`);
  - una sola ruta sin la clave de la clínica, además de la prueba de identidad: `GET /grupo/resumen` (D-25).
    - Va con una clave de grupo propia (`X-Grupo`), guardada solo como huella, que crea y retira Dirección.
    - Devuelve cifras por centro, sin datos de pacientes, y no abre nada más.
    - Los intentos con claves incorrectas se frenan como los de la clave de la clínica.
    - El ordenador del grupo solo conecta con la huella del servidor ya conocida.
    - Pruebas: `server::tests::the_group_key_opens_only_the_summary` y `server::tests::a_group_of_two_clinics_adds_up_their_summaries`.
- **Modo de un solo ordenador:** fichero de datos local bloqueado para una
  ventana, con diario de cambios (`src/local.rs`).
- **Carpeta compartida (modo antiguo):** retirada para instalaciones nuevas,
  con paso comprobado al servidor (`src/shared.rs`).
- **Páginas públicas** (cita online, «Mi cita», presupuestos para firmar, pago por internet):
  - salen por Cloudflare Tunnel, sin abrir puertos (`src/tunnel.rs`);
  - cada enlace es un código aleatorio de 128 bits (`src/portal.rs`).

## 2. Autenticación y permisos
- **Contraseñas:** PBKDF2-HMAC-SHA256 de 600 000 vueltas con sal, comparadas
  en tiempo constante. Las huellas antiguas se rehacen al entrar
  (`src/users.rs`).
- **Bloqueo por intentos fallidos:** a partir de 5 fallos
  (`users::tests::lockout_after_failures_and_pin_change`,
  `users::tests::alternating_users_does_not_reset_the_lockout`). Además el
  servidor limita los intentos por ordenador (429).
- **Segundo factor:** TOTP, RFC 6238, que no acepta dos veces el mismo código
  (`src/totp.rs`).
- **Perfiles:** Dirección, Doctor, Higienista y Recepción. La vista de
  Recepción no incluye lo clínico (`src/vista.rs`).
- **Dinero:** el servidor no acepta de quien no tiene permiso de caja un cobro en negativo ni una anulación. Un cobro no se borra ni cambia de importe, paciente, forma de pago o fecha salvo Dirección, y el banco solo con su permiso. La caja y las devoluciones con datáfono, solo con permiso de caja. Una financiación ya pedida solo cambia por las operaciones del servidor, con su fecha y el permiso de cada paso (`server::tests::money_out_and_the_bank_need_their_permission`).
- **Autorizaciones y liquidaciones de las compañías:** solo cambian por las operaciones del servidor, con permiso de facturación, y el servidor apunta quién lo hizo: crear, enviar, responder, anular, cobrar. Un puesto no las crea, no las cambia ni las borra por su cuenta (`server::tests::two_stations_cannot_ask_twice_for_the_same_authorization`).
- **Configuración:** la cambia Dirección. De otro perfil, el servidor solo acepta los campos de `USER_CONFIG` (las iniciales en la pizarra y su usuario y su centro); lo demás, también lo que se añada después, se queda como está.
- **Pedidos a proveedores:** desde un puesto solo se edita un borrador. El envío automático a un proveedor, y su correo mientras lo tenga, solo lo cambia Dirección. Un proveedor nuevo no puede repetir el NIF ni el correo de otro, y para la aprobación cuentan juntos los pedidos al mismo proveedor aunque estén en otra ficha. Proponer, confirmar, aprobar (con permiso de gastos), enviar, recibir, cerrar y cancelar los hace el servidor: dos puestos no proponen ni reciben dos veces lo mismo (`server::tests::orders_are_proposed_and_received_once_and_approved_by_expenses`).
- **Centros:** un usuario puede trabajar solo en unos centros (D-25). El servidor no acepta de él citas, cajas, cobros, citas pedidas por internet ni huecos de otro centro, ni operaciones en otro centro, y no le manda las cajas de los demás (`server::tests::a_user_of_one_center_cannot_act_in_another`). Los pacientes, su historia y su saldo son de toda la clínica.
- **Comprobado en el servidor:**
  - `server::tests::only_direction_changes_users_and_secrets_stay_on_the_server`;
  - `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station`;
  - `server::tests::only_management_blocks_or_unblocks_a_patient_on_the_server`;
  - `server::tests::only_a_prescriber_marks_prescription_warnings_reviewed`;
  - `server::tests::the_time_record_only_grows_on_the_server`;
  - `server::tests::a_confirmed_evolution_note_cannot_be_changed_by_a_station` (las listas protegidas están en `firmado::PROTECTED_LISTS`).

## 2 bis. Rutas del servidor y lo que exige cada una (T-R7-5)
| Ruta | Sin la clave de la clínica | Sin sesión | Lo que se comprueba |
|---|---|---|---|
| `GET /prueba` | sí | sí | Firma un reto con la clave de la clínica. Limitada por ordenador. |
| `GET /grupo/resumen` | sí, con la clave de grupo | sí | Clave de grupo en uso; solo cifras (D-25). Limitada por ordenador. |
| `GET /acceso`, `POST /registro` | no | sí | La clave de la clínica. Los usuarios van sin contraseñas ni claves del móvil. |
| `POST /entrar`, `POST /codigo` | no | sí | Contraseña y código del móvil. Bloqueo por intentos fallidos. |
| `POST /parar` | no | sí | Solo desde el propio ordenador servidor. |
| `GET /estado` | no | no | — |
| `GET /datos`, `GET /cambios` | no | no | Lo que ve el perfil (`vista`): sin lo clínico para recepción, sin las cajas de otros centros. |
| `POST /cambios` | no | no | Reglas por lista (`State::save`). La matriz por perfil, abajo. |
| `POST /cadena` | no | no | Permiso de cada operación (`ChainOp::needs`), el centro del usuario y lo que comprueba cada una. |
| `POST /historia` | no | no | Apunta quién consulta cada historia. |
| `GET /historia` | no | no | Solo Dirección. |
| `POST /contrasena` | no | no | La propia, con la actual; la de otro, solo Dirección. |
| `POST /movil` | no | no | Solo Dirección. |

- **Matriz de escrituras directas** (`server::tests::the_server_applies_each_write_rule_to_each_profile`): con Dirección, Recepción, Doctor e Higienista se prueba:
  - un cobro en negativo y cambiar el importe de un cobro;
  - conciliar a mano y una salida de caja;
  - el nombre de la clínica, otro usuario y una clave de grupo;
  - una liquidación;
  - un pedido en borrador y uno aprobado a mano;
  - el periodontograma;
  - el fichaje de otro.

  Cada cambio se acepta o no según el perfil de la sesión, no según lo que diga el puesto.
- **Matriz de operaciones** (`server::tests::the_server_applies_each_operation_permission_to_each_profile`): facturar, remesa, banco, anular un cobro, fusionar, proponer y aprobar pedidos y campañas, con los cuatro perfiles. Además, a ningún perfil le llegan contraseñas, sales ni claves del móvil.
- **Páginas públicas** (cita online, Mi espacio, presupuestos, pagos, encuestas): van aparte (`src/booking.rs`). Cada enlace es un código aleatorio de 128 bits, y cada página solo enseña lo de su enlace (`booking::tests::the_patient_portal_never_shows_another_patient_by_changing_an_id`).

## 3. Criptografía (sin criptografía propia: `ring` y `rustls`)

| Uso | Algoritmo | Dónde |
|---|---|---|
| Datos, diario y copias locales | AES-256-GCM, clave aleatoria de 256 bits con identificador; la cabecera va como datos asociados | `src/datos_cifrados.rs` |
| Segunda copia (USB o nube) | PBKDF2-HMAC-SHA256 de 600 000 vueltas y AES-256-GCM | `src/cifrado.rs` |
| Contraseñas | PBKDF2-HMAC-SHA256 de 600 000 vueltas | `src/users.rs` |
| Red | TLS 1.3 e identidad Ed25519 del servidor | `src/server.rs` |
| Paquetes de actualización | Firma Ed25519 del manifiesto y SHA-256 de cada fichero | `src/actualizar.rs` |
| Registro de accesos | Cadena SHA-256 línea a línea | `src/accesos.rs` |
| Pago por internet (Redsys) | HMAC-SHA512 con clave por pedido (AES-CBC del pedido), comprobado con los ejemplos de Redsys; comparación en tiempo constante | `src/pago.rs` |
| Segundo factor | TOTP (HMAC-SHA1, por compatibilidad con las apps) | `src/totp.rs` |

- **Claves de los datos:**
  - están en la configuración del usuario (0600), no en la carpeta de los datos;
  - la clave de recuperación no se deja guardar junto a los datos.
- **Con el cifrado puesto:** no se aceptan un fichero ni una línea del diario
  sin cifrar, y un cambio de cifrado cortado se termina o se deshace
  (`datos_cifrados::tests::plain_data_is_refused_when_encrypted_and_cut_conversions_recover`, E-040).
- **Actualizaciones:**
  - solo se acepta un paquete firmado, completo y más nuevo
    (`actualizar::tests::only_a_signed_complete_and_newer_package_is_accepted`);
  - aún no hay clave de versiones (B-27), así que hoy se rechaza todo paquete.

## 4. Robustez
- **Entradas al azar contra el servidor:**
  `server::tests::the_server_survives_random_requests_and_its_data_stays_readable`
  y `server::tests::the_request_reader_survives_random_bytes`.
- **Diario de cambios:**
  - una línea cortada al final se descarta;
  - una dañada antes del final, o líneas desordenadas, se avisan y no se guarda encima
    (`server::tests::a_cut_or_damaged_journal_never_applies_changes_out_of_order`).
- **Concurrencia entre puestos:** reenvíos, numeración de facturas y cobros
  únicos (`server::tests::a_push_resent_after_a_lost_answer_changes_nothing`,
  `server::tests::invoices_from_two_stations_keep_their_numbers_and_the_chain`).
- **Ficheros privados:** datos, diario, registros y claves se crean con
  permisos 0600 (`src/paths.rs`).

## 4 bis. Límites de entradas, ficheros y tareas (T-R7-6)
Cada límite, dónde está y la prueba que lo comprueba. Lo que pasa del límite
se rechaza antes de leerlo entero o se recorta, con un mensaje que explica qué
hacer.

| Entrada | Límite | Prueba |
|---|---|---|
| Petición al servidor de la clínica | cabecera acotada, cuerpo de 512 MB como mucho, sin envío por partes | `server::tests::requests_are_limited` |
| Conexiones al servidor | 64 a la vez; 16 desde un mismo ordenador; una conexión quieta se cierra a los 15 s | `server::tests::idle_connections_cannot_take_the_server` (corregido en E-080) |
| Respuestas que lee un puesto | 1 GB de datos; 64 KB de prueba de identidad, también comprimida (bomba gzip) | `server::tests::answers_are_read_with_a_limit`, `server::tests::big_answers_travel_compressed` |
| Números de historia | hasta 999 999 999 (con ellos se nombran carpetas) | `server::tests::two_new_patients_never_share_a_history_number` |
| Operaciones que manda un puesto | extracto del banco: 50 000 movimientos y 500 caracteres por texto; 10 000 mensajes por reserva de correo; 500 líneas por recepción de pedido; 200 actos por autorización | `app::tests::a_bank_statement_from_a_station_is_bounded`, `app::tests::lists_in_station_operations_are_bounded` |
| Páginas públicas (citas, portal, recetas, pagos) | 16 KB por petición; intentos por enlace y por dirección | `booking::tests::public_pages_refuse_oversized_requests`, `booking::tests::the_patient_portal_never_shows_another_patient_by_changing_an_id`, `booking::tests::my_prescriptions_link_needs_id_and_birth_date` |
| Sesión del portal del paciente | 20 min sin uso y 2 h en total | `portal_paciente::tests::a_portal_session_expires_when_idle_and_after_its_maximum` |
| Avisos de la pasarela de WhatsApp | 1 MB por aviso | `whatsapp::tests::the_gateway_hook_refuses_oversized_bodies` |
| Llamadas de la centralita | 30 por minuto | `phone::tests::web_pages_cannot_fake_calls_and_floods_are_cut` |
| Inicios de sesión | bloqueo tras 5 fallos | `users::tests::lockout_after_failures_and_pin_change` |
| DICOM | 512 MB de píxeles por estudio (también la serie entera de una carpeta); número de imágenes que no cuadra con el tamaño | `dicom::tests::invalid_compressed_huge_or_random_files_are_refused_without_panicking` |
| STL | 5 millones de triángulos; el fichero, como mucho lo que ocupan en binario, comprobado antes de leerlo | `stl::tests::a_file_over_the_limit_is_refused_before_reading_it`, `stl::tests::invalid_cut_or_huge_models_are_refused_without_panicking` |
| Paquete del laboratorio por correo | 18 MB de adjuntos | `lab_order::tests::attachments_too_big_for_an_email_are_left_out` |
| Registro del programa | 1 MB por fichero, tres ficheros, 500 caracteres por línea | `registro::tests::the_log_rotates_and_keeps_three_files_at_most` |
| Respuestas de las aseguradoras | 8 000 caracteres de la respuesta y 300 de la nota | `autorizaciones::tests::a_huge_answer_is_kept_trimmed` |
| Resumen del grupo | 400 días por consulta | `grupo::tests::the_connection_text_round_trips_and_needs_the_server_fingerprint` |
| IA de pago | tope mensual | `ia::tests::paid_agents_are_not_called_past_the_monthly_limit` |

**Tareas largas:** van en segundo plano y la pantalla sigue respondiendo:
cargar el visor 3D, el datáfono, el resumen del grupo, la IA, el dictado y
los envíos de correo. Si el usuario pasa a otra cosa, lo que traiga la tarea
anterior se descarta. Lo que dura cada una está acotado por los límites de la
tabla y por los tiempos de espera de red.
- `screens::visor3d::tests::a_stuck_load_neither_blocks_nor_replaces_the_next_model`:
  una carga que no acaba no bloquea, y no sustituye al modelo abierto después.
- `datafono::tests::a_lost_reply_is_recovered_by_asking_not_by_resending`:
  una respuesta perdida del datáfono no cobra dos veces.

## 5. Hallazgos de las revisiones propias, ya corregidos
Cada uno tiene su corrección y su prueba en la evidencia indicada:
- E-008: primeras revisiones automáticas de seguridad.
- E-015: IA de radiografías.
- E-022: rutas de red en la migración y ficheros con permisos 0644.
- E-024: la bandeja de cambios pendientes (confused deputy y cambios perdidos).
- E-025: secretos de la CI y sesión caducada que enviaba como otro usuario.
- E-026: nombres escritos a mano en el registro.
- E-033: el registro de accesos (autoría, puesto tomado de la cabecera, cadena
  que fallaba abierta).
- E-034: desbloqueo desde el puesto y lectura de ficheros arbitrarios al exportar.
- E-035: borrado de pacientes bloqueados.
- E-030: fórmulas en los CSV.
- E-036: orden de los fichajes y autor de las correcciones.
- E-040: vuelta atrás a datos sin cifrar.

## 6. Límites conocidos (para centrar la auditoría)
- **Imágenes:** no están cifradas por el programa, porque las comparten los
  programas de radiología. Las protege el cifrado del disco.
- **Volver a un estado anterior:** quien pueda escribir en la carpeta de los
  datos puede quitar las últimas líneas del diario o el diario entero. Para
  impedirlo haría falta un contador guardado fuera de la carpeta.
- **Clave de la clínica:** la comparten todos los puestos, y por eso además
  hace falta usuario y sesión.
- **Firma de código e instaladores:** sin certificado de firma de código
  (B-25) ni clave de versiones (B-27).
- **Sin pruebas reales:** no se ha probado en Windows ni en macOS reales
  (B-24), ni contra servicios reales (AEAT B-06, Meta B-07).

## 7. Cómo reproducir
- `tools/aislado.sh cargo test --workspace --all-targets --locked`: toda la batería, aislada de los datos del ordenador.
- `tools/aislado.sh cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Ensayo de instalación: `server::tests::installation_rehearsal_on_linux`.
- Se recomienda incluir en el alcance:
  - el servidor y su protocolo, con un cliente propio;
  - la carpeta de datos con un usuario local distinto;
  - el túnel y las páginas públicas, incluida la notificación de pago (`/api/tpv/notificacion`, `booking::tests::online_payment_from_the_page_to_the_signed_notice`);
  - la actualización con paquetes manipulados.

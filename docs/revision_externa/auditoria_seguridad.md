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
- **Dinero:** el servidor no acepta de quien no tiene permiso de caja un cobro en negativo ni una anulación. Un cobro no se borra ni cambia de importe, paciente, forma de pago o fecha salvo Dirección, y el banco solo con su permiso. Lo que mueve dinero de la financiación lo hace el servidor con el permiso de cada paso (`server::tests::money_out_and_the_bank_need_their_permission`).
- **Centros:** un usuario puede trabajar solo en unos centros (D-25). El servidor no acepta de él citas, cajas, cobros, citas pedidas por internet ni huecos de otro centro, ni operaciones en otro centro, y no le manda las cajas de los demás (`server::tests::a_user_of_one_center_cannot_act_in_another`). Los pacientes, su historia y su saldo son de toda la clínica.
- **Comprobado en el servidor:**
  - `server::tests::only_direction_changes_users_and_secrets_stay_on_the_server`;
  - `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station`;
  - `server::tests::only_management_blocks_or_unblocks_a_patient_on_the_server`;
  - `server::tests::only_a_prescriber_marks_prescription_warnings_reviewed`;
  - `server::tests::the_time_record_only_grows_on_the_server`;
  - `server::tests::a_confirmed_evolution_note_cannot_be_changed_by_a_station` (las listas protegidas están en `firmado::PROTECTED_LISTS`).

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

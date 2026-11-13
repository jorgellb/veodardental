# Área 1: documentos, consentimientos y cuestionarios

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Crear un documento desde una plantilla, con los comodines rellenos al crearlo | `Db::new_document` (src/comms.rs) |
| Firma en la clínica, en pantalla o tableta, con el firmante propuesto (el representante si es menor de 16) | src/screens/documentos.rs → `Db::sign_document` |
| Firma por internet: enlace con token, comprobación de la huella de lo leído, nombre, NIF opcional, IP y fecha | src/doc_link.rs (`doc_remote_sign`) y src/doc_page.html |
| Presupuesto firmado por internet, que genera un documento firmado | src/budget_link.rs |
| Cuestionario de salud: en la clínica, por internet (sin firmar) y revisiones nuevas como copia | src/health.rs, src/screens/salud.rs y src/portal.rs |
| Descarga e impresión: HTML con firma, evidencias y, ahora, huella y revocación | `doc_html`, `export_html` |
| En red, Recepción recibe los documentos (para firmarlos) y no las respuestas de los cuestionarios | src/vista.rs |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Versión y contenido exacto firmado | El texto se rellena al crear el documento y cambiar la plantilla no lo toca. Faltaba la huella de lo firmado en la clínica (D1). |
| Integridad e inmutabilidad | Un documento firmado podía cambiar sin que se notara (D1). En red, un puesto podía cambiarlo o borrarlo (D2), y en carpeta compartida también. |
| Identidad y representación de menores | En la clínica se propone al representante. Por internet, un menor podía firmar con su propio nombre (D5). |
| Relación entre firma, documento y paciente | El sello une contenido, paciente, firmante, fecha y trazo. Fusionar fichas mueve lo firmado sin invalidarlo, porque se guarda el paciente para quien se firmó. |
| Caducidad y actualización del cuestionario | Caduca a los 365 días y «Nueva revisión» copia sin firma, como debe. El firmado seguía editable (D4). |
| Permisos, revocación y descarga | Solo se quita un documento sin firmar. No había revocación aunque el texto la ofrece (D3). La descarga funciona y ahora incluye la huella y la revocación. |

## Defectos demostrados y corregidos
- **D1.** La firma en la clínica no guardaba la huella de lo firmado.
  - **Ahora:** `content_hash` y `seal` (src/firmado.rs) se guardan al firmar, también en el presupuesto firmado por internet. La pantalla y la hoja impresa dicen si el contenido cambió.
  - **Prueba:** `doc_link::tests::a_signed_document_detects_later_changes`.
- **D2.** En red, un puesto podía cambiar o borrar un documento o cuestionario firmado.
  - **Ahora:** el servidor (`firmado::protect_json`) y la carpeta compartida (`Db::keep_signed`) solo aceptan la revocación, las observaciones del profesional del cuestionario y el paso a la ficha que se conserva al fusionar.
  - **Pruebas:** `server::tests::signed_documents_and_forms_cannot_be_changed_by_a_station` y `shared::tests::a_station_cannot_change_signed_records_in_the_shared_folder`.
- **D3.** No se podía revocar un consentimiento.
  - **Ahora:** `Db::revoke_document` y el botón «Revocar…» conservan el documento y su firma y anotan quién, cuándo y por qué, con registro de cambios. Revocar el de protección de datos quita la autorización de comunicaciones.
  - **Pruebas:** `doc_link::tests::a_signed_consent_can_be_revoked_without_losing_it` y `ui_tests::a_signed_consent_is_revoked_from_the_patient_file`.
- **D4.** El cuestionario firmado se podía editar y desmarcar.
  - **Ahora:** `Db::sign_health` sella las respuestas; en pantalla, las de un firmado no se pueden cambiar y solo cambian las observaciones.
  - **Pruebas:** `health::tests::a_signed_health_form_is_sealed` y `ui_tests::a_signed_health_form_cannot_be_edited_in_the_file`.
- **D5.** Un menor de 16 años podía firmar su propio documento por internet.
  - **Ahora:** se rechaza, y la página avisa y propone el nombre del representante.
  - **Prueba:** `doc_link::tests::a_minor_does_not_sign_their_own_document_online`.

Además, la cabecera y las columnas de la pestaña Salud se salían de la ventana a 1280 píxeles. Ahora caben (capturas `f1_area1_salud_*`).

## Límites y pendientes externos
- La validez jurídica definitiva de la firma manuscrita digitalizada y por enlace requiere revisión externa (B-22).
- Los documentos firmados antes de este cambio no tienen huella. Se muestran como «no se puede comprobar», salvo los firmados por internet, cuya huella está en su enlace.

# Área 9: cita online y portales

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Página pública de cita: motivos, profesionales y huecos; reservar | `/`, `/api/datos`, `/api/huecos` y `/api/reservar` en src/booking.rs (`book`) |
| «Mi cita»: ver, confirmar, cambiar, anular, cuestionario de salud, `.ics` | `/mi-cita/{código}` y `/api/cita/{código}/…` (`portal`) |
| Hueco libre ofrecido a la lista de espera | `/hueco/{código}` (src/slot_offer.rs) |
| Documentos y consentimientos para firmar | `/documento/{código}` (src/doc_link.rs) |
| Presupuesto para revisar y aceptar con firma | `/presupuesto/{código}` (src/budget_link.rs) |
| Recetas del paciente, con segundo dato y bloqueo tras fallos | `/recetas/{código}` (src/rx_link.rs) |
| Publicación en internet con Cloudflare Tunnel | src/tunnel.rs |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Tokens | 128 bits del generador del sistema (`portal::new_token`); se exige el formato antes de buscarlos. |
| Caducidad | Los de cita, un día después de la cita; los de hueco, tras el día del hueco; las recetas, con segundo dato y 366 días. Los de documento y presupuesto no caducaban (P1). |
| Acceso por paciente y responsable | Cada enlace es de un objeto (una cita, un documento, un presupuesto) y muestra el nombre de pila. Un menor no firma su documento (área 1). Las recetas piden un segundo dato. |
| Enumeración | 20 códigos erróneos por dirección y hora; después, 429. |
| Intentos repetidos y abuso | 240 peticiones por minuto y dirección; 5 reservas por hora y dirección; 15 a 20 acciones por enlace y hora; campo trampa para robots; cuerpo acotado. |
| Dirección real tras el túnel | Las cabeceras solo se creen si la petición viene del propio ordenador (el túnel). Primero `CF-Connecting-IP`, que pone Cloudflare y el cliente no puede falsear. |
| Reservas concurrentes | Las solicitudes guardan su hueco (`held`) bajo el cerrojo: la misma hora pedida dos veces da 409 (prueba existente `web_page_books_a_free_slot`). Al confirmar, el servidor vuelve a comprobar el hueco (`ChainOp::Confirm`). |
| Horario validado en el servidor | `book` solo acepta una hora de las libres que calcula él con el horario publicado, los festivos, los bloqueos, la antelación mínima y los días visibles, no la que mande el navegador. |
| Cancelaciones | Con la antelación de la clínica (`can_modify`). |
| Móvil | Las seis páginas llevan `viewport` y diseño adaptable. |

## Defectos demostrados y corregidos
- **P1. Enlaces de documento y de presupuesto sin caducidad.** Seguían abriendo indefinidamente, y muestran nombre, tratamiento, importes y el texto del documento.
  - **Demostración:** pruebas en rojo antes del arreglo; a los 31 días seguían abiertos.
  - **Ahora:**
    - abren 30 días (`portal::LINK_DAYS`) desde la última vez que se envían, se firman o se deciden;
    - un presupuesto pendiente abre además mientras está en plazo;
    - volver a enviarlo renueva el mismo enlace si aún no está firmado o decidido;
    - firmado o decidido, abre 30 días más para verlo.
  - **Pruebas:** `doc_link::tests::document_links_expire_and_resending_renews_them` y `budget_link::tests::budget_links_expire`.

## Límites
- Si el generador aleatorio del sistema fallara (no pasa en los sistemas soportados), `new_token` usaría la hora y los códigos serían adivinables. Se deja anotado.
- El portal del paciente con sesión (todas sus citas, facturas y documentos, con autorización por responsable) es una función de la fase 5, no de esta revisión.

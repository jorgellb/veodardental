# Área 10: telefonía y laboratorio digital

Revisado el 13/11/2026.

## Flujos inventariados
| Flujo | Dónde |
|---|---|
| Aviso de llamada desde la centralita o el programa de teléfono: `veodardental --llamada <número>` o `http://127.0.0.1:8897/llamada?numero=…` | `phone::listen`, `forward` y `number_from_url` |
| Identificación de quién llama (pacientes con ese número, contactos, proveedores, mutuas), registro de llamadas y aviso en pantalla | `patients_by_phone`, `other_caller`, `log_call`; src/screens/llamadas.rs |
| Orden de trabajo digital: documento imprimible con el esquema de piezas, paquete ZIP con escaneados, fotos y radiografías, envío por correo, anonimización (iniciales y nº de historia) | src/lab_order.rs; src/screens/laboratorio.rs |
| Estados del trabajo (por enviar, en laboratorio, prueba, recibido, colocado, repetición) y coste, que pasa a Gastos al recibirlo | `set_lab_state` y `sync_lab_expense` |

## Comprobaciones hechas
| Comprobación | Resultado |
|---|---|
| Autenticación y límites de la integración de telefonía | Escucha solo en 127.0.0.1, pero sin más control: una página web abierta en el navegador podía hacer saltar avisos y llenar el registro (F1). Tampoco había límite de avisos (F2). |
| Formatos | Aceptaba cualquier texto como número (F3). |
| Eventos repetidos | La misma llamada avisada dos veces se apuntaba dos veces (F4). |
| Paquetes grandes | Leía en memoria y mandaba por correo adjuntos de cualquier tamaño (L1). |
| Rutas y archivos maliciosos | Los nombres dentro del ZIP los pone el programa y se limpian (`safe`): no hay rutas del exterior dentro del paquete. Las carpetas DICOM se rechazan con un aviso. No hay recepción de ficheros del laboratorio que tratar. |
| Estados y costes | El gasto del trabajo se apunta una vez (por su etiqueta) y se corrige mientras no esté pagado. Pero dos puestos que lo marcaban como recibido a la vez creaban dos gastos (L2). |

## Defectos demostrados y corregidos
Todos, con pruebas en rojo antes del arreglo.
- **F1. Avisos falsos desde el navegador.**
  - **Ahora:** se rechazan (403) las peticiones que el navegador marca como de otro sitio (`Sec-Fetch-Site: cross-site` o `same-site`). La centralita, el programa de teléfono y la dirección escrita a mano siguen funcionando.
  - **Prueba:** `phone::tests::web_pages_cannot_fake_calls_and_floods_are_cut`.
- **F2. Avalanchas.**
  - **Ahora:** más de 30 avisos por minuto se cortan (429) y no tapan las llamadas de verdad.
  - **Prueba:** la misma.
- **F3. Números.**
  - **Ahora:** solo cifras (de 3 a 17), separadores habituales y un «+» delante. Por ejemplo, «<script>» no pasa.
  - **Prueba:** `phone::tests::only_real_numbers_and_no_repeated_calls`.
- **F4. Avisos repetidos.**
  - **Ahora:** la misma llamada avisada otra vez en menos de 15 segundos es la misma: un solo registro y un solo aviso.
  - **Prueba:** la misma.
- **L1. Paquetes grandes.**
  - **Ahora:** los adjuntos que no caben en un correo (18 MB en total) no se meten en el paquete ni se leen, y se avisa para enviarlos por la plataforma o la nube del laboratorio.
  - **Prueba:** `lab_order::tests::attachments_too_big_for_an_email_are_left_out`, con un fichero disperso de 18 MB + 1 byte que no llega a leerse.
- **L2. Gasto doble.**
  - **Demostración:** antes de arreglarlo, el servidor quedaba con 2 gastos de un mismo trabajo.
  - **Ahora:** el gasto de un trabajo lleva su mismo número, y dos puestos que lo crean a la vez crean el mismo registro.
  - **Prueba:** `server::tests::a_lab_work_received_on_two_stations_is_one_expense`.

## Límites
- El laboratorio no tiene integración propia: se le envía la orden por correo. Para ficheros grandes se avisa para usar su plataforma (de cada laboratorio, sin especificación común).
- La centralita no se autentica: está en el mismo ordenador. Una centralita en red que avise a otro puesto necesitaría una clave compartida. No es el caso de las integraciones documentadas (programa local o línea de órdenes).

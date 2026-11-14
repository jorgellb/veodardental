# Aseguradoras: autorizaciones y liquidaciones

Cómo lleva VeodarDental las autorizaciones y las liquidaciones con las
compañías aseguradoras (T-F6-04, `src/autorizaciones.rs`, `src/insurer.rs`).
No hay conexión directa con ninguna compañía: cada una tiene su canal (portal,
correo o API), y hace falta su acuerdo y su entorno de pruebas (B-20,
T-F6-04X). Hasta entonces se usa el canal de cada compañía y aquí se apunta
cada paso.

## Autorizaciones
- **Pedirla:** en la cuenta de un paciente asegurado, se eligen los tratamientos previstos y «Pedir autorización». La crea el servidor:
  - un tratamiento está como mucho en una solicitud viva (no denegada ni anulada), aunque dos puestos la pidan a la vez;
  - no se piden tratamientos que ya están en una liquidación.
- **Enviarla:** por el portal o el correo de la compañía. Se apunta el canal y la referencia que da la compañía.
- **Respuesta:** aprobada (con su número y, si la da, la fecha hasta la que vale), aprobada en parte o denegada.
  - Se guardan todas las respuestas, también cuando la compañía corrige una anterior, con la fecha, quién la apuntó y el texto que mandó la compañía.
  - El número pasa a los tratamientos aprobados y sale en la liquidación.
  - Si después se deniega o se anula, se quita.
- **Caducidad:** pasada la fecha de validez, consta como caducada.
- **Anular:** con motivo, mientras ningún acto esté liquidado. Los actos quedan libres.

## Liquidaciones
- Se emiten como hasta ahora: el servidor las numera y un acto no entra en dos.
- **Envío:** «Enviada» apunta cuándo y por dónde. La liquidación se imprime o se baja en CSV para el portal.
- **Respuesta:** los actos que rechaza la compañía, cada uno con su motivo, y el texto de la respuesta tal cual.
  - Los rechazados dejan de contar en lo que debe pagar la compañía.
  - Se pueden liquidar de nuevo, ya corregidos, en otra liquidación, una sola vez.
  - No se acepta un rechazo que deje lo cobrado por encima de lo que queda.

## Contrato para una conexión directa
Cuando una compañía dé su API, el adaptador tendrá que traducir a y desde esto:

| Operación | Envía VeodarDental | Espera de la compañía |
|---|---|---|
| Pedir autorización | referencia (`AUT-…`), póliza, paciente, actos (código, descripción, piezas) | referencia de la compañía |
| Consultar autorización | referencia | estado (`aprobada`, `parcial`, `denegada`, `pendiente`), número, actos denegados, fecha de validez y texto |
| Enviar liquidación | número (`LIQ-…`), actos (fecha, póliza, código, piezas, nº de autorización, importe del baremo) | referencia del lote |
| Consultar liquidación | número | actos rechazados con su motivo, importe aceptado y texto |

**Reglas:**
- **Una referencia es una sola solicitud o un solo lote:** repetir el envío no duplica.
- **La respuesta de la compañía se guarda tal cual** (hasta 8 000 caracteres), además de lo que se entiende de ella.
- **Sin respuesta, se consulta:** no se reenvía.

**Probado:**
- `autorizaciones::tests::an_authorization_is_asked_once_and_its_answers_are_kept`;
- `autorizaciones::tests::only_insured_patients_and_their_own_treatments`;
- `insurer::tests::a_rejected_act_leaves_the_claim_and_can_be_claimed_again_once`;
- `server::tests::two_stations_cannot_ask_twice_for_the_same_authorization`;
- `ui_tests::an_authorization_is_asked_from_the_insured_patient_account`.

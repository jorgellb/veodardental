# Datáfono y financiación: contrato de integración

Lo que VeodarDental espera de un datáfono conectado y cómo lleva la
financiación (T-F6-03, `src/datafono.rs`, `src/financiacion.rs`). La
conexión con un proveedor concreto está pendiente de su especificación y su
entorno de pruebas (B-19, T-F6-03X). Hasta entonces:
- el datáfono se usa sin conectar: se cobra en él y se apunta el resultado con
  el número de autorización;
- la financiación se apunta a mano desde el portal de la entidad.

## Datáfono conectado
Una pasarela del proveedor (o un puente local) atiende estas tres peticiones.
Van por HTTPS o, si es un puente en el mismo ordenador, por `http://127.0.0.1`,
con `Authorization: Bearer <clave>`. La clave se guarda solo en el puesto
(`datafono.json`).

| Petición | Cuerpo | Respuesta |
|---|---|---|
| `POST /operaciones` (con `Idempotency-Key: <referencia>`) | `{"referencia", "tipo": "venta" o "devolucion", "importe" (céntimos), "moneda": "EUR", "original" (referencia de la venta, en una devolución)}` | la operación |
| `GET /operaciones/<referencia>` | — | la operación, o 404 si no consta |
| `POST /operaciones/<referencia>/cancelar` | `{}` | la operación (si ya estaba cobrada, sigue aprobada) |

La operación: `{"referencia", "estado": "en_curso" | "aprobada" | "denegada" | "cancelada", "id", "autorizacion", "tarjeta", "mensaje"}`.

**Reglas que tiene que cumplir el proveedor:**
- **Una referencia es una sola operación.** Repetir el `POST` con la misma referencia devuelve la misma operación y no cobra dos veces.
- **404 al preguntar** quiere decir que la operación no llegó y no se cobró. VeodarDental la da por no hecha.
- **`tarjeta`:** a lo sumo los últimos dígitos. VeodarDental guarda solo los cuatro últimos.

**Lo que hace VeodarDental:**
- Antes de enviar, deja la operación «en el datáfono». Si se corta la conexión, queda «sin respuesta» y **pregunta por la referencia**; nunca reenvía a ciegas.
- Comprueba las que llevan más de dos minutos en el datáfono.
- **El cobro (o la devolución) se apunta una sola vez**, al quedar aprobada, en la caja del centro donde se hizo.
- Una operación aprobada no vuelve atrás. Si se canceló pero el datáfono dice que se cobró, manda el datáfono.
- **Devolución:** de una venta aprobada del mismo paciente, como mucho por lo que quede sin devolver.

**Probado** con un datáfono simulado que cumple este contrato (`datafono::fake`):
- `datafono::tests::a_connected_terminal_charges_once`;
- `datafono::tests::a_lost_reply_is_recovered_by_asking_not_by_resending`;
- `datafono::tests::cancelling_and_refunding_follow_the_terminal`;
- `datafono::tests::a_standalone_terminal_is_recorded_by_hand`.

## Financiación con una entidad
Estados y pasos:
- **Solicitada:** se pasa a aprobada o denegada.
- **Aprobada:** se pasa a firmada (con la fecha de la firma) o denegada.
- **Firmada:** se pasa a «pagada a la clínica».
- **Pagada a la clínica.**
- **Cancelada.**

Cada paso queda en la historia, con quién y cuándo, y con la referencia de la entidad.
- **Al pagar la entidad:**
  - el paciente queda pagado por el total financiado, con un cobro «Financiación» que se apunta una sola vez;
  - lo que no llegó (lo que se queda la entidad) es un gasto de la clínica, «Comisión de financiación»;
  - el ingreso del banco se concilia con la financiación por lo recibido, en los 15 días de alrededor del pago.
- **Desistimiento:** dentro de los 14 días siguientes a la firma (Ley 16/2011, artículo 28; lo confirma la gestoría). Si la entidad ya había pagado, se apunta la devolución a la entidad, y el paciente vuelve a deber el tratamiento. Pasado ese plazo, no se cancela desde aquí.
- **Lo que no hace el programa:** las altas, los contratos y las decisiones de financiación son de la entidad, y el programa no las toma ni las simula.

**Probado:**
- `financiacion::tests::a_financing_goes_from_request_to_payment_once`;
- `financiacion::tests::withdrawing_follows_the_deadline_and_returns_the_money`.

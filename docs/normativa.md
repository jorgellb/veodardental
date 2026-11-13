# Normativa consultada

Cada entrada indica la fuente oficial, cuándo se consultó, la versión y el
efecto sobre el código. Lo que no se ha podido confirmar en una fuente oficial
queda marcado como pendiente.

## Ley 37/1992, del IVA: artículo 20.Uno, números 3.º y 5.º
- **Fuente:** BOE, texto consolidado BOE-A-1992-28740, https://www.boe.es/buscar/act.php?id=BOE-A-1992-28740#a20
- **Consultado:** 13/11/2026. Versión: «Última actualización publicada el 07/10/2026».
- **Texto del 3.º:** «La asistencia a personas físicas por profesionales médicos o sanitarios, cualquiera que sea la persona destinataria de dichos servicios.»
- **Texto del 5.º:** «Las prestaciones de servicios realizadas en el ámbito de sus respectivas profesiones por estomatólogos, odontólogos, mecánicos dentistas y protésicos dentales, así como la entrega, reparación y colocación de prótesis dentales y ortopedias maxilares realizadas por los mismos, cualquiera que sea la persona a cuyo cargo se realicen dichas operaciones.»
- **Efecto en el código:**
  - La referencia de exención se elige en Configuración › Facturación y cobros (`ClinicConfig::vat_exemption`). Por defecto se mantiene el 20.Uno.3º de siempre.
  - Se guarda en cada factura al emitirla (`Invoice::exemption`); las ya emitidas imprimen exactamente la leyenda que imprimían.
  - En VERI*FACTU se sigue declarando `OperacionExenta` E1 (exenta por el artículo 20); el número concreto solo va en la leyenda.
- **Pendiente:** la elección y activación es de la gestoría (B-01). Este registro no es una interpretación fiscal.

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

## Quién puede recetar
- **Real Decreto 1718/2010, sobre receta médica y órdenes de dispensación, art. 1.a)**
  - **Fuente:** https://www.boe.es/buscar/act.php?id=BOE-A-2011-1013, consultado el 13/11/2026. Versión: «Última actualización publicada el 23/12/2015».
  - **Texto:** la receta es el documento «mediante el cual los médicos, odontólogos o podólogos, legalmente facultados para ello, y en el ámbito de sus competencias respectivas» prescriben medicamentos o productos sanitarios sujetos a prescripción médica.
- **Real Decreto Legislativo 1/2015 (Ley de garantías y uso racional de los medicamentos), art. 79.1**
  - **Fuente:** https://www.boe.es/buscar/act.php?id=BOE-A-2015-8343, consultado el 13/11/2026. Versión: «Última actualización publicada el 31/07/2026».
  - **Texto:** el médico, el odontólogo o el podólogo son los «únicos profesionales con facultad para recetar medicamentos sujetos a prescripción médica».
- **Efecto en el código:** `Db::prescriber_problem`.
  - No pueden recetar higienistas, auxiliares, protésicos, recepción o administración (según la especialidad del colaborador), ni quien esté de baja o no tenga nº de colegiado.
  - Sin un prescriptor válido no se imprime ni se emite.
  - La receta nueva propone a quien puede recetar.

## Clasificación ATC de los avisos de receta
- Los avisos existentes (alergia a penicilina; antiinflamatorio en paciente anticoagulado, embarazada o hipertenso) se aplican ahora también a los medicamentos de la lista oficial de la AEMPS por su código ATC:
  - J01C: antibacterianos betalactámicos, penicilinas;
  - M01A: antiinflamatorios y antirreumáticos no esteroideos.
- Es la clasificación ATC/DDD de la OMS. No se ha añadido ninguna regla clínica nueva.
- **Pendiente:** revisión profesional de las reglas (B-16).

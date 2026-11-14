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

## VERI*FACTU: calendario vigente
- **Fuente:** Real Decreto-ley 15/2025, de 2 de diciembre, que modifica el Real Decreto 1007/2023 (BOE-A-2025-24446, https://www.boe.es/buscar/doc.php?id=BOE-A-2025-24446). También la nota de la AEAT «Ampliación del plazo de adaptación de los sistemas informáticos de facturación» y sus preguntas frecuentes (sede.agenciatributaria.gob.es).
- **Consultado:** 13/11/2026. No se han encontrado cambios posteriores en las fuentes consultadas.
- **Contenido:**
  - plazos: 1 de enero de 2027 para quien tributa por el Impuesto sobre Sociedades, y 1 de julio de 2027 para los demás (una clínica de un profesional autónomo);
  - según las preguntas frecuentes de la AEAT, después de esas fechas tener un sistema de facturación no adaptado ya es sancionable;
  - quien opte por VERI*FACTU tras la obligatoriedad lo mantiene al menos hasta el final del año natural del primer envío.
- **Efecto en el código:** ninguno nuevo. El envío VERI*FACTU ya está hecho y probado contra una AEAT simulada (E-027). La fecha desde la que se envía es configurable (`VfConfig::since`). La prueba con la AEAT real sigue en B-06.

## Historia clínica: acceso y conservación (Ley 41/2002)
- **Fuente:** Ley 41/2002, de 14 de noviembre, básica reguladora de la autonomía del paciente (BOE-A-2002-22188, https://www.boe.es/buscar/act.php?id=BOE-A-2002-22188), artículos 16 y 17.
- **Consultado:** 13/11/2026, texto consolidado.
- **Artículo 16:**
  - accede a la historia el profesional que diagnostica o trata al paciente;
  - el personal de administración y gestión, solo a los datos relacionados con sus funciones;
  - quien accede tiene deber de secreto;
  - para fines judiciales, de salud pública, investigación o docencia, se separan los datos identificativos de los clínicos.
- **Artículo 17:** la historia se conserva **como mínimo cinco años** desde el alta de cada proceso asistencial, y además por razones judiciales, epidemiológicas o de investigación.
- **Normas autonómicas con más plazo:** por ejemplo, Cataluña (Ley 21/2000, modificada por la Ley 16/2010) fija quince años para la documentación más relevante. No se ha revisado cada comunidad.
- **Efecto en el código:**
  - **Registro de accesos (T-F4-02):** quién consulta y quién modifica cada historia, sin copiar la historia al registro.
  - **Vista de recepción:** limitada a lo administrativo, ya existente (`vista`).
  - **Herramientas RGPD (T-F4-03):** la supresión queda desactivada hasta la revisión externa; se puede bloquear y restringir. El plazo mínimo de conservación es de cinco años desde el último proceso, y la comunidad autónoma de la clínica puede alargarlo.
- **Pendiente:** el plazo concreto de cada clínica y las excepciones, con el responsable de protección de datos (B-10, T-F4-03X).

## Protección de datos
- **Referencia:** RGPD (Reglamento (UE) 2016/679), artículos 5, 9, 15 a 18, 20, 30 y 32, y Ley Orgánica 3/2018 (LOPDGDD).
- **Pendiente:** no se ha hecho en esta fecha una consulta nueva de guías de la AEPD.
- **Efecto en el código:**
  - medidas de seguridad propias del artículo 32: cifrado, registro de accesos y minimización en registros y medidas;
  - derechos de acceso, rectificación, limitación y portabilidad del paciente (T-F4-03).
- **Lo que no se afirma:** que el producto cumpla el RGPD. Lo revisa el responsable de protección de datos (B-13).

## Registro de jornada
- **Fuente:** Estatuto de los Trabajadores, artículo 34.9 (introducido por el Real Decreto-ley 8/2019), y la «Guía sobre el registro de jornada» del Ministerio de Trabajo (https://www.mites.gob.es/ficheros/ministerio/GuiaRegistroJornada.pdf).
- **Consultado:** 13/11/2026.
- **Contenido:**
  - registro diario con el horario concreto de inicio y de final de la jornada;
  - según la guía, la norma no fija una modalidad concreta (papel, digital…).
  - el Real Decreto 951/2026 (residentes de Ciencias de la Salud, BOE-A-2026-23266) remite al 34.9 y a su normativa de desarrollo.
- **No aprobado a esta fecha:** el borrador de real decreto que desarrollaría el 34.9 con registro digital obligatorio y el anteproyecto que lo pasaría a un artículo 34 bis no constan publicados en el BOE en la búsqueda de esta fecha. Se registran como «en tramitación».
- **Efecto en el código (T-F4-06):**
  - registro digital, objetivo y fiable de entradas, salidas y pausas;
  - correcciones trazables, que conservan el original;
  - acceso por rol (cada persona lo suyo, Dirección todo) y exportación.
  - La conservación de cuatro años y la disposición para la persona trabajadora, sus representantes y la Inspección son del texto del artículo 34.9 que se recuerda. **Hay que confirmarlas con la lectura literal en el BOE** antes de presentarlas como requisito.
- **Pendiente:** la revisión de la asesoría laboral (B-26, T-F4-06X). Una pantalla de fichaje no demuestra el cumplimiento.

## Libros registro de IVA e IRPF (exportación contable)
- **Fuente:** AEAT, «Formatos de los Libros Registro de IVA e IRPF», formato electrónico común, versión del 18/02/2026 (https://www3.agenciatributaria.gob.es/static_files/Sede/Tema/IVA/Fact_registro/Libros_registro/Formato_Electronico_Comun_Libros_Registro_IVA_IRPF.pdf), y preguntas frecuentes de la importación de libros en el Pre303.
- **Consultado:** 13/11/2026.
- **Contenido:**
  - XLSX con una hoja por libro (EXPEDIDAS, RECIBIDAS…), de 4 MB como mucho;
  - un libro por ejercicio;
  - campos de número y serie, fechas, NIF, base, tipo, cuota, cuota deducible y total.
- **Diseños y plantilla (consultados el 14/11/2026):**
  - «Diseños de registro normalizados para los Libros Registro del IVA y del IRPF presentados en formato XLXS (actualizado 01-01-2026)», en https://sede.agenciatributaria.gob.es/static_files/AEAT/LSI.xlsx: columnas A a AJ (expedidas e ingresos) y A a AP (recibidas y gastos), con tipos y longitudes y 40 validaciones numeradas;
  - «Plantilla libros soporte electrónico (actualizada a 09-09-2026)», en https://sede.agenciatributaria.gob.es/static_files/Sede/Tema/IVA/Fact_registro/Libros/PLANTILLA_LIBROS_UNIFICADOS.xlsx: hojas finales EXPEDIDAS_INGRESOS y RECIBIDAS_GASTOS con cabecera en las filas 1 a 3 y datos desde la 4, y las listas oficiales de claves (actividad A05 y otras, F1, F2, R1 a R5, I01, G01 a G48, E1 a E6).
- **Efecto en el código (T-F4-07):** `src/libros.rs` sigue este formato oficial, que es el que se puede documentar. Detalles:
  - libros unificados («T») para una persona física; del IVA («C») para una sociedad, sin las columnas del IRPF;
  - facturas exentas E1 con concepto I01;
  - gastos con su concepto G (propuesta por categoría) y la cuota soportada no deducible;
  - se comprueban las validaciones 2, 4, 5, 11, 32, 33 y 37 antes de entregarlo.
- **A3, ContaSol y Sage:** no se ha encontrado una especificación oficial pública de sus ficheros de importación (SUENLACE.DAT de A3 y otros), solo descripciones de terceros. No se implementan sin especificación: queda en B-11.

## TicketBAI (territorios forales del País Vasco)
- **Fuente:** Diputación Foral de Gipuzkoa, «Normas Forales que regulan la Obligación TicketBAI» (Norma Foral 3/2020) y Decreto Foral 32/2020, con sus preguntas frecuentes (actualizadas el 20/06/2025). Álava: Decreto Foral 48/2021 (BOTHA). Bizkaia: Batuz. Especificaciones técnicas: «Especificaciones funcionales y técnicas del sistema TicketBAI 1.0» (euskadi.eus).
- **Consultado:** 13/11/2026.
- **Contenido:**
  - obliga a quien ejerce actividades económicas y está sujeto a la normativa foral del IRPF o del IS, según el Concierto Económico (no por tener el domicilio allí);
  - en Gipuzkoa es obligatorio por sectores desde el 1/7/2022.
- **No confirmado:** si alcanza a los servicios sanitarios exentos de IVA, en la fuente foral concreta de cada territorio.
- **Especificación técnica consultada el 14/11/2026:** «Especificaciones funcionales y técnicas del sistema TicketBAI 1.0», de 19/12/2019:
  - 4.3.2: identificativo TBAI de 39 caracteres, «TBAI-NIF-DDMMAA-13 primeros caracteres de la firma-CRC»;
  - 4.3.3: QR con nivel de corrección M y de 30 a 40 mm, con la dirección de cada hacienda y los parámetros id, s, nf, i y cr;
  - anexo 3: CRC-8 de polinomio x⁸+x²+x+1, valor inicial 0 y sin reflejar.
- **Efecto en el código (T-F4-08, E-041):**
  - arquitectura separada (`src/foral.rs`), sin activar por defecto;
  - el territorio solo cuenta con la confirmación de la gestoría; entonces VERI*FACTU deja de enviar;
  - identificativo y QR comprobados con los ejemplos de la especificación;
  - sin inventar un formato común con VERI*FACTU;
  - el fichero TBAI firmado (XAdES), el envío y la homologación quedan pendientes.
- **Navarra:** sin consultar a esta fecha; pendiente.
- **Pendiente:** la homologación y activación (B-12).

## Receta electrónica privada: organismos
- **Fuentes:**
  - documentos del Consejo General de Colegios Oficiales de Médicos (cgcom.es) sobre el Sistema de Receta Electrónica Privada (SREP): procedimiento de certificación de plataformas de prescripción y especificación funcional (versión de marzo de 2022);
  - Consejo General de Colegios Farmacéuticos (farmaceuticos.com, «e-Receta privada», especificación DF ereceta v2.05 de noviembre de 2024).
- **Consultado:** 13/11/2026.
- **Contenido:**
  - el sistema lo impulsan los Consejos Generales de Médicos, Dentistas (CGCOE), Podólogos y Farmacéuticos;
  - las plataformas de prescripción las certifica el Consejo del prescriptor, el de Dentistas para los odontólogos, con recertificación por auditoría cada dos años;
  - las farmacias validan las recetas a través de Nodofarma contra los repositorios de los Consejos de prescriptores.
- **No encontrado:** nada fechado en 2026 sobre cambios.
- **Efecto en el código (T-F4-01):** la trazabilidad, la hoja del paciente y las pruebas con una farmacia simulada ya existen (repositorio `srep`). La certificación es del CGCOE: B-09.

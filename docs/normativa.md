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

## Pago por internet: TPV virtual de Redsys y Bizum
- **Fuente:** documentación de desarrolladores de Redsys (pagosonline.redsys.es), consultada el 14/11/2026:
  - «Realizar un pago»: redirección a `realizarPago`; parámetros en Base64URL y firma `HMAC_SHA512_V2`; Bizum con `Ds_Merchant_PayMethods` = «z»; el resultado solo vale por la notificación HTTP POST, nunca por la vuelta del navegador;
  - «Firmar una operación»: clave de la operación con AES-CBC (vector de ceros) del número de pedido con los 16 primeros caracteres de la clave, y HMAC-SHA512;
  - «Devolver o anular un pago»: petición REST a `trataPeticionREST` con tipo 3; «0900» si se hace;
  - «Parámetros de entrada y salida»: 0000 a 0099, autorizada; 9915, cancelada por el usuario; `Ds_ProcessedPayMethod` 68, Bizum; el pedido tiene hasta 12 caracteres y se recomienda que los 4 primeros sean cifras;
  - «Manual de integración REST», versión 4.0.1 de 17/10/2025: firma `HMAC_SHA512_V1` y la anterior `HMAC_SHA256_V1`, que no se usan.
- **Comprobado:** la firma del programa da exactamente las de los dos ejemplos de la documentación, con la clave pública de pruebas (`pago::tests::the_signature_matches_the_redsys_examples`).
- **Efecto en el código (T-F5-02):** `src/pago.rs`, la página `/pagar/{código}` y la notificación `/api/tpv/notificacion`.
  - Ningún dato de tarjeta pasa por el programa.
  - El importe lo pone el programa.
  - Solo cuenta la notificación con firma válida, del comercio y terminal configurados, en euros y por el importe pedido.
- **Pendiente:** el contrato del TPV y la prueba en el entorno de pruebas del banco (B-14, T-F5-02X).

## Alertas clínicas ampliadas (T-F5-07)
- **Consultado:** 14/11/2026. Solo para citar la fuente de cada alerta. Las alertas dicen qué comprobar; no recomiendan tratamientos ni recetan.
- **Profilaxis de endocarditis:** Wilson WR et al., «Prevention of Viridans Group Streptococcal Infective Endocarditis: A Scientific Statement From the American Heart Association», Circulation 2021;143:e963–e978 (PubMed 33853363).
  - Mantiene las indicaciones de 2007: profilaxis solo en los pacientes de alto riesgo (válvula protésica o material de reparación, endocarditis previa, determinadas cardiopatías congénitas y trasplante cardíaco con valvulopatía).
  - Solo en procedimientos que manipulan la encía o la región periapical o perforan la mucosa.
- **Osteonecrosis por medicamentos:** Ruggiero SL et al., «AAOMS Position Paper on Medication-Related Osteonecrosis of the Jaws—2022 Update», J Oral Maxillofac Surg 2022;80(5):920-943 (PubMed 35300956).
- **Anticoagulantes y antiagregantes:** SDCEP, «Management of Dental Patients Taking Anticoagulants or Antiplatelet Drugs», 2.ª edición, marzo de 2022 (sdcep.org.uk).
- **Embarazo:**
  - para las radiografías, se remite a los criterios de selección de pacientes de la ADA y la FDA (fda.gov), sin afirmar nada propio;
  - para los medicamentos, a la sección 4.6 de la ficha técnica en CIMA (AEMPS).
  - No se encontró en las fuentes consultadas una recomendación específica del embarazo que citar.
- **Pendiente:** la revisión de los textos por un odontólogo de la clínica (B-16, T-F5-07X).

## Reseñas públicas: no pedir solo las positivas (T-F5-09)
- **Fuente:** Google, «Prohibited & restricted content» de las contribuciones de Maps (support.google.com/contributionpolicy/answer/7400114), consultada el 14/11/2026.
  - Prohíbe a los negocios desanimar o impedir las reseñas negativas y pedir de forma selectiva las positivas.
  - Permite pedir reseñas que reflejen una experiencia real, sin incentivos y sin intentar influir en la nota ni en el contenido.
  - Una noticia de 2022 decía que esa línea se había quitado; la página oficial consultada la incluye.
- **Efecto en el código (`src/encuestas.rs`):**
  - el enlace a la reseña se ofrece igual a todos, antes y después de responder y con cualquier nota;
  - la encuesta no condiciona nada y no hay incentivos.


## Comunicaciones comerciales: campañas (T-F5-12)
- **Fuentes, citadas de memoria y sin consultar en esta sesión; las confirma el DPO (`docs/revision_externa/dpo.md`):**
  - Ley 34/2002 (LSSI), artículos 21 y 22. Una comunicación comercial por correo electrónico o un medio equivalente (SMS, mensajería) necesita una solicitud o una autorización expresa previa. En cada una hay que ofrecer una forma sencilla y gratuita de oponerse.
  - RGPD:
    - artículos 6.1.a y 7: el consentimiento tiene que poder demostrarse;
    - artículo 9.2.a: los datos de salud necesitan un consentimiento explícito para esa finalidad;
    - artículo 21.2 y 21.3: oposición a la mercadotecnia directa.
  - LOPDGDD, artículo 23.4. Antes de la mercadotecnia directa hay que consultar los sistemas de exclusión publicitaria (Lista Robinson), salvo que el afectado haya consentido recibirla de quien la envía.
- **Efecto en el código (`src/campanas.rs`, D-24):**
  - solo a quien ha consentido expresamente, con fecha, y no se ha dado de baja;
  - la forma de darse de baja va en cada mensaje;
  - el texto no lleva datos de salud;
  - elegir por tratamiento queda a decisión del DPO.

## Alojamiento del servidor en la nube (T-F6-02)
- **Fuentes, citadas de memoria y sin consultar en esta sesión; las confirma el DPO:**
  - RGPD, artículo 28: contrato de encargado del tratamiento;
  - RGPD, artículo 32: seguridad;
  - RGPD, artículo 33: notificación de brechas en 72 horas;
  - RGPD, artículos 44 a 49: transferencias fuera del EEE;
  - RGPD, artículo 9: categorías especiales (salud).
- **Efecto:**
  - el despliegue (`deploy/nube/`, `docs/despliegue_nube.md`) pide una región del EEE, el contrato de encargo, copias fuera del servidor y acceso solo por VPN o desde las direcciones de la clínica;
  - el servidor no escribe la clave de la clínica en el registro del sistema y arranca con la zona horaria de la clínica;
  - la contratación sigue pendiente (B-18).

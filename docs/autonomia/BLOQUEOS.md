# Bloqueos externos

Cada bloqueo afecta solo a la parte que necesita la dependencia externa. La
preparación técnica se hace aparte (tareas de ingeniería enlazadas en
`TAREAS.json`) y el resto del trabajo sigue. «Preparación terminada» indica
qué tareas la componen; el verificador comprueba que estén HECHA.

## B-01 Exención de IVA en las facturas
- Falta: decisión de la gestoría sobre el artículo citado (20.Uno.3º o 20.Uno.5º de la Ley 37/1992) y fecha de activación.
- Por qué: es una decisión fiscal; el programa no debe presentar una interpretación propia como aprobación.
- Parte afectada: solo el texto de exención de las facturas nuevas (T-F0-05).
- Preparación terminada: T-F0-02, T-F0-03 y T-F0-04 (referencia configurable para facturas nuevas; las emitidas no cambian).
- Responsable: gestoría de la clínica.
- Paso para desbloquear: que la gestoría elija la referencia y la clínica la active en Configuración › Facturación.
- Prueba posterior: emitir una factura de prueba en modo demostración y comprobar la leyenda; las facturas anteriores conservan la suya.

## B-02 Copias de prueba del 11 y 12 de noviembre
- Falta: decisión de la clínica de conservar o borrar las dos copias «automática del servidor» de `~/.local/share/VeodarDental/copias/`.
- Por qué: son copias reales del ordenador; el encargo prohíbe acceder a esa carpeta o borrarlas.
- Parte afectada: solo esas dos copias (T-F0-06).
- Preparación terminada: T-INI-02 (pruebas y ensayos aislados: no se tocan las carpetas reales).
- Responsable: clínica (Dirección).
- Paso para desbloquear: que Dirección decida y, si quiere borrarlas, lo haga desde Configuración › Copias o lo autorice expresamente.
- Prueba posterior: la lista de copias en Configuración › Copias refleja la decisión.

## B-03 GESDEN: esquema o exportación autorizada y simulacro real
- Falta: acceso autorizado de solo lectura a una base de GESDEN (o su esquema documentado y una exportación) con alcance y destino definidos.
- Por qué: la historia, citas, saldos y documentos de GESDEN viven en tablas propietarias cuyo esquema no está en el repositorio; inventarlo no certifica compatibilidad.
- Parte afectada: mapeo real de historia, tratamientos, citas, saldos, cobros y documentos de GESDEN y el simulacro real (T-F2-10).
- Preparación terminada: T-F2-05 y T-F2-09 (motor, mapeo configurable, validaciones e informe con fixtures sintéticos).
- Responsable: clínica (propietaria de los datos de GESDEN) con autorización escrita.
- Paso para desbloquear: facilitar una copia de prueba de la base de GESDEN (o el listado de tablas y columnas) en un entorno autorizado.
- Prueba posterior: simulacro de migración en carpeta aislada con el informe de conciliación cuadrado.

## B-04 Novaclinic: exportación autorizada
- Falta: base o exportación autorizada de Novaclinic.
- Por qué: mismas razones que GESDEN.
- Parte afectada: mapeo real de Novaclinic (T-F2-11).
- Preparación terminada: T-F2-06 y T-F2-09.
- Responsable: clínica que use Novaclinic.
- Paso para desbloquear: facilitar una copia de prueba autorizada.
- Prueba posterior: simulacro conciliado.

## B-05 Clinic Cloud y Dentalink: exportaciones reales
- Falta: ficheros reales de exportación (Excel/CSV) de Clinic Cloud y Dentalink, autorizados y sin datos reales o anonimizados.
- Por qué: los nombres de columna y formatos de esos programas no están documentados públicamente de forma verificable.
- Parte afectada: comprobar los perfiles de mapeo (T-F2-12).
- Preparación terminada: T-F2-07 (perfiles configurables y asistente genérico probados con fixtures sintéticos).
- Responsable: clínica o proveedor del programa de origen.
- Paso para desbloquear: facilitar una exportación de ejemplo de cada programa.
- Prueba posterior: lectura con el perfil y simulacro sin errores.

## B-06 AEAT: entorno de pruebas con el certificado de la clínica
- Falta: certificado electrónico de la clínica instalado en un puesto autorizado y permiso para usar el entorno de pruebas de VERI*FACTU.
- Por qué: la AEAT solo responde con certificado válido; el encargo prohíbe acceder a los certificados de la clínica.
- Parte afectada: prueba real de envío (T-F3-06X).
- Preparación terminada: T-F3-06 (envío, rechazo, caída, reintento e idempotencia probados con un servidor simulado).
- Responsable: clínica y gestoría.
- Paso para desbloquear: Dirección configura el certificado en Configuración › VERI*FACTU con el entorno de pruebas y lanza «Enviar ahora».
- Prueba posterior: respuesta «Correcto» del entorno de pruebas con su CSV.

## B-07 WhatsApp Business de Meta
- Falta: cuenta de WhatsApp Business de la clínica, número verificado y plantilla «recordatorio_cita» aprobada por Meta.
- Por qué: los envíos reales requieren cuenta y plantilla aprobada; el encargo prohíbe crear cuentas o aceptar condiciones.
- Parte afectada: prueba real de WhatsApp (T-F3-07X).
- Preparación terminada: T-F3-07 (envío, fallos, reintentos y resultados inciertos probados con servidor simulado).
- Responsable: clínica.
- Paso para desbloquear: crear la cuenta y la plantilla siguiendo Configuración › WhatsApp y enviar a un número de prueba propio.
- Prueba posterior: recordatorio de prueba recibido y respuesta leída por el programa.

## B-08 Piloto de un mes en una clínica real
- Falta: una clínica que trabaje un mes solo con VeodarDental, con autorización.
- Por qué: el cierre del piloto exige actividad real; no se puede sustituir por una simulación.
- Parte afectada: cierre del piloto y mediciones de mejora (T-F3-10).
- Preparación terminada: T-F3-01, T-F3-02, T-F3-05, T-F3-08 y T-F3-09 (instalación, actualizaciones, copias, guías e instrumentos de medida).
- Responsable: clínica piloto y propietario del producto.
- Paso para desbloquear: acordar el piloto, instalar con la lista de instalación y activar los instrumentos de medida.
- Prueba posterior: informe del mes con caja, facturación AEAT, copias restauradas y mediciones exportadas.

## B-09 Receta electrónica privada: certificación
- Falta: certificación del sistema por los organismos competentes y prueba de interoperabilidad con Nodofarma.
- Por qué: sin certificación no se pueden dispensar recetas reales en farmacias.
- Parte afectada: conexión real con farmacias (T-F4-01X).
- Preparación terminada: T-F4-01 (circuito completo probado con farmacia simulada y expediente).
- Responsable: propietario del producto con el Consejo General de Dentistas y el CGCOM.
- Paso para desbloquear: presentar el expediente de Configuración › Receta electrónica › Camino a las farmacias reales.
- Prueba posterior: prueba conjunta con Nodofarma y resolución de certificación.

## B-10 Plazos de conservación y supresión (RGPD)
- Falta: revisión del DPO de los plazos de conservación de la historia clínica y de las excepciones aplicables en la comunidad autónoma de la clínica.
- Por qué: suprimir datos de salud sin base revisada puede vulnerar la ley de autonomía del paciente o la normativa autonómica.
- Parte afectada: activación de la supresión sobre datos reales (T-F4-03X).
- Preparación terminada: T-F4-03 (herramientas completas, supresión desactivada hasta la revisión).
- Responsable: DPO de la clínica.
- Paso para desbloquear: que el DPO revise docs/revision_externa y fije los plazos en la configuración.
- Prueba posterior: supresión de un paciente ficticio respetando los plazos configurados.

## B-11 Exportación contable aceptada por la gestoría
- Falta: importación de los ficheros exportados en el programa contable de la gestoría (A3, ContaSol o Sage).
- Por qué: la aceptación final depende del programa y la configuración de la gestoría.
- Parte afectada: validación de la exportación contable (T-F4-07X).
- Preparación terminada: T-F4-07 (ficheros generados y validados contra la especificación disponible).
- Responsable: gestoría.
- Paso para desbloquear: enviar a la gestoría un fichero de prueba generado con datos ficticios.
- Prueba posterior: la gestoría confirma la importación sin errores.
- Estado tras T-F4-07: ya se generan los libros registro en el formato común de la AEAT (`src/libros.rs`). Quedan para la gestoría tres cosas:
  1. comprobar que acepta el fichero en su programa o en el Pre303;
  2. revisar la correspondencia de categorías de gasto con los conceptos G y el tratamiento en el IRPF del IVA soportado no deducible;
  3. aportar, si los necesita, la especificación oficial o una muestra autorizada de los formatos propios de A3 (SUENLACE.DAT), ContaSol o Sage, que no tienen documentación pública oficial.


## B-12 Facturación foral: homologación y activación
- Falta: alta en el entorno de pruebas de la hacienda foral, certificado y, en su caso, homologación del software.
- Por qué: TicketBAI exige entorno y certificados de cada diputación; la activación solo procede si la clínica tributa allí.
- Parte afectada: homologación y activación foral (T-F4-08X).
- Preparación terminada: T-F4-08 (E-041). Territorio elegido y confirmado por la gestoría; VERI*FACTU deja de aplicar; identificativo TBAI y QR con CRC-8 comprobados con los ejemplos de la especificación 1.0.
- Falta por hacer, además del alta:
  - el fichero TBAI (XML con firma XAdES), según el XSD y la política de firma vigentes de cada diputación;
  - el envío (Araba, Gipuzkoa) o el modelo 140/240 de Batuz (Bizkaia);
  - el alta en el registro de software garante de cada hacienda (`foral::HOMOLOGATED` sigue en falso).
- Navarra: tiene su propio sistema, que no se ha consultado, y queda fuera de TicketBAI.
- Responsable: propietario del producto y clínica con domicilio fiscal foral.
- Paso para desbloquear: solicitar el alta en el entorno de pruebas de la diputación correspondiente.
- Prueba posterior: envío aceptado por el entorno de pruebas foral.

## B-13 Revisiones externas: gestoría, DPO y auditoría de seguridad
- Falta: revisión de la gestoría, del DPO y una auditoría de seguridad externa.
- Por qué: una revisión propia no sustituye esas aprobaciones.
- Parte afectada: cierre normativo de la fase 4 (T-F4-09X).
- Preparación terminada: T-F4-09 (E-042). Paquete de evidencias para cada revisor en `docs/revision_externa`: `gestoria.md`, `dpo.md` y `auditoria_seguridad.md`.
- Responsable: propietario del producto.
- Paso para desbloquear: encargar las revisiones con el paquete de docs/revision_externa.
- Prueba posterior: informes externos archivados.

## B-14 TPV virtual y Bizum: contrato y credenciales de pruebas
- Falta: contrato de comercio con un banco (TPV virtual) y sus credenciales del entorno de pruebas.
- Por qué: las firmas y notificaciones reales usan la clave secreta del comercio; el encargo prohíbe contratar.
- Parte afectada: prueba con el entorno de pruebas del banco y activación (T-F5-02X).
- Preparación terminada: T-F5-02 (E-044). Flujo completo con Redsys por redirección (HMAC_SHA512_V2, comprobado con los ejemplos de su documentación), notificaciones y devoluciones simuladas, sin cargos ni datos de tarjeta.
- Para la prueba con el banco: el número de comercio, el terminal y la clave de pruebas van en Configuración › Cita online › TPV virtual de este ordenador, con la página de citas publicada en internet (la notificación tiene que poder llegar).
- Responsable: clínica.
- Paso para desbloquear: solicitar al banco el TPV virtual y copiar sus datos de pruebas en Configuración › Pago por internet.
- Prueba posterior: pago de prueba con tarjeta de pruebas del banco y notificación conciliada.

## B-15 Transcripción de voz: proveedor o modelo
- Falta: elección y alta de un proveedor de transcripción (o descarga autorizada de un modelo local) y su prueba real.
- Por qué: transcribir audio requiere un servicio o un modelo; el encargo prohíbe gastar créditos o crear cuentas.
- Parte afectada: transcripción real de notas y periodontograma por voz (T-F5-04X).
- Preparación terminada: T-F5-04 (E-046). Las notas de evolución transcriben con cualquier servicio compatible con la API de OpenAI (`/v1/audio/transcriptions`), probado con un doble de prueba. T-F5-05 (E-048), con el mismo servicio, o con el dictado del teclado del móvil.
- Grabar desde el micrófono del ordenador dentro del programa también queda pendiente. Hoy se transcribe una grabación hecha con el móvil o con la grabadora del sistema; el programa no guarda el audio y puede borrar la grabación al terminar.
- Responsable: clínica.
- Paso para desbloquear: elegir proveedor o modelo y configurarlo en Configuración › Inteligencia artificial.
- Prueba posterior: dictado de prueba transcrito y confirmado por el profesional.

## B-16 Revisión clínica de vademécum y alertas
- Falta: revisión por un odontólogo de las reglas del vademécum y de las alertas clínicas ampliadas.
- Por qué: el programa no debe inventar reglas clínicas; las fuentes y su aplicación necesitan revisión profesional.
- Parte afectada: validación clínica (T-F1-A2-EXT, T-F5-07X).
- Preparación terminada: T-F1-A2-FIX y T-F5-07 (E-051). Cuatro alertas (endocarditis, antirresortivos, anticoagulantes y embarazo) con su fuente citada y el visto del profesional. Los textos están en `src/alertas.rs` y en `docs/normativa.md`; `alertas::REVIEWED` sigue en falso.
- Responsable: dirección médica de la clínica.
- Paso para desbloquear: revisar el listado de reglas y fuentes en docs/revision_externa.
- Prueba posterior: reglas marcadas como revisadas con fecha y responsable.

## B-17 Validación de traducciones clínicas y legales
- Falta: revisión de las traducciones al catalán, euskera, gallego e inglés de documentos clínicos y legales.
- Por qué: un consentimiento o documento legal mal traducido no es válido.
- Parte afectada: uso de documentos traducidos con pacientes (T-F5-11X).
- Preparación terminada: T-F5-11 (E-059).
  - Catálogo en los cinco idiomas: navegación, avisos a pacientes y páginas del paciente.
  - Consentimientos traducidos solo con validación (`DocTemplate::validated`).
  - `i18n::REVIEWED` sigue en falso.
- Responsable: clínica (traductores o profesionales nativos).
- Paso para desbloquear: revisar los textos marcados en docs/revision_externa/traducciones.
- Prueba posterior: documentos marcados como validados.

## B-18 Alojamiento en la nube
- Falta: proveedor de alojamiento contratado y contrato de encargo de tratamiento.
- Por qué: alojar datos de salud requiere contrato y garantías; el encargo prohíbe contratar.
- Parte afectada: servidor en la nube real (T-F6-02X).
- Preparación terminada: T-F6-02 (despliegue documentado y ensayado en local).
  - Están la unidad de systemd endurecida y el aprovisionamiento con modo de ensayo (`deploy/nube/`).
  - La guía con las condiciones del alojamiento está en `docs/despliegue_nube.md`, pendiente de confirmar por el DPO.
  - El ensayo con el programa real como proceso aparte está en `tests/ensayo_nube.rs` (E-064).
- Responsable: propietario del producto o clínica.
- Paso para desbloquear: elegir proveedor en la UE y firmar el encargo de tratamiento.
- Prueba posterior: despliegue en el proveedor con copias y acceso remoto comprobados.

## B-19 Datáfono y financiación: proveedor
- Falta: especificación y entorno de pruebas de un proveedor de datáfono integrado y de una entidad de financiación.
- Por qué: cada proveedor tiene su protocolo; sin especificación no se puede validar la integración.
- Parte afectada: conexión real (T-F6-03X).
- Preparación terminada: T-F6-03 (adaptadores con proveedor simulado).
- Responsable: clínica.
- Paso para desbloquear: solicitar al proveedor su especificación y credenciales de pruebas.
- Prueba posterior: operación de prueba conciliada.

## B-20 Aseguradoras: API o acuerdo
- Falta: API documentada, acuerdo y entorno de pruebas de cada aseguradora.
- Por qué: las autorizaciones y liquidaciones directas solo se pueden hacer por canales permitidos por la aseguradora.
- Parte afectada: conexión real con aseguradoras (T-F6-04X).
- Preparación terminada: T-F6-04 (flujo interno y contrato de integración).
- Responsable: clínica y aseguradoras.
- Paso para desbloquear: solicitar a cada aseguradora su canal de integración.
- Prueba posterior: autorización de prueba enviada y respuesta guardada.

## B-21 Grupo real de clínicas
- Falta: un grupo de clínicas autorizado para trabajar con datos e informes conjuntos.
- Por qué: el cierre real de la fase 6 exige uso real.
- Parte afectada: T-F6-09X.
- Preparación terminada: T-F6-01 y T-F6-09 (escenario sintético con permisos y totales verificados).
- Responsable: propietario del producto.
- Paso para desbloquear: acordar el despliegue con un grupo.
- Prueba posterior: informes consolidados validados por el grupo.

## B-22 Validez jurídica de firmas y consentimientos
- Falta: revisión jurídica del proceso de firma (manuscrita en pantalla y por enlace) y de los consentimientos.
- Por qué: la validez jurídica definitiva requiere revisión externa.
- Parte afectada: T-F1-A1-EXT.
- Preparación terminada: T-F1-A1-FIX (integridad, versión firmada y representación de menores probadas).
- Responsable: jurista o DPO de la clínica.
- Paso para desbloquear: revisar el informe del área 1 en docs/autonomia/revision/area1.md.
- Prueba posterior: informe jurídico archivado.

## B-23 Centralita y laboratorio reales
- Falta: acceso a la centralita telefónica y al laboratorio protésico de la clínica para una prueba real.
- Por qué: cada centralita y laboratorio usa su propio sistema.
- Parte afectada: T-F1-A10-EXT.
- Preparación terminada: T-F1-A10-FIX (integraciones probadas con eventos simulados y paquetes sintéticos).
- Responsable: clínica.
- Paso para desbloquear: configurar la centralita en Configuración › Telefonía y enviar una orden de prueba al laboratorio.
- Prueba posterior: llamada de prueba identificada y orden recibida por el laboratorio.

## B-24 Instalación en Windows y macOS reales
- Falta: máquinas o runners de Windows y macOS.
- Por qué: compilar en Linux no prueba Windows ni macOS.
- Parte afectada: T-F3-01X.
- Preparación terminada: T-F3-01 (ensayo en Linux) y el flujo de CI multiplataforma.
- Responsable: propietario del producto.
- Paso para desbloquear: lanzar el flujo «Instaladores» en GitHub o probar en máquinas reales.
- Prueba posterior: instalación, servidor y puesto funcionando en cada sistema.

## B-25 Certificados de firma de código
- Falta: certificado de firma de código para Windows y cuenta de Apple Developer con notarización.
- Por qué: tienen coste y requieren identidad legal; el encargo prohíbe comprarlos.
- Parte afectada: firma y notarización de los instaladores (T-F3-03X).
- Preparación terminada: T-F3-03 (pasos de firma en CI condicionados a los secretos).
- Responsable: propietario del producto.
- Paso para desbloquear: comprar los certificados y guardarlos como secretos del repositorio.
- Prueba posterior: instaladores firmados que Windows y macOS abren sin aviso.

## B-26 Revisión legal del registro de jornada
- Falta: revisión por asesoría laboral del registro de jornada.
- Por qué: una pantalla de fichaje no demuestra cumplimiento legal.
- Parte afectada: T-F4-06X.
- Preparación terminada: T-F4-06 (registro, correcciones trazables y exportación probados).
- Responsable: clínica y su asesoría laboral.
- Paso para desbloquear: revisar el registro y la exportación con la asesoría.
- Prueba posterior: conformidad de la asesoría.

## B-27 Clave de firma de las versiones
- Falta: el par de claves Ed25519 con que se firman las versiones. La privada va en un secreto de CI, nunca en el repositorio; la pública, en `actualizar::RELEASE_KEYS`.
- Por qué: la clave es del responsable del producto, y una clave inventada aquí no daría autenticidad a nada.
- Parte afectada: instalar versiones con `--actualizar`. Hasta entonces se instalan con su instalador.
- Preparación terminada: T-F3-02 (manifiesto firmado, comprobación, copia previa, cambio por pasos, vuelta atrás y pruebas de corte entre pasos).
- Responsable: propietario del producto.
- Paso para desbloquear: generar la clave, guardar la privada como secreto y añadir al flujo de CI la firma del manifiesto.
- Prueba posterior: un paquete firmado en CI se instala con `--actualizar` y uno cambiado se rechaza.

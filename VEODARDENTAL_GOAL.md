# VeodarDental: instrucciones de ejecución autónoma

Este documento contiene el encargo completo para Claude Code. Guárdalo en la raíz del repositorio y utiliza el comando `/goal` del final. No es una descripción de trabajo ya realizado: los resultados deben comprobarse en el repositorio.

## 1. Objetivo y alcance

Desarrolla VeodarDental hasta completar todo el trabajo de ingeniería del plan de este documento: revisión de calidad, migraciones, preparación del piloto, cumplimiento técnico, funcionalidades diferenciales y crecimiento. Implementa y verifica los cambios; redactar propuestas o interfaces vacías no cumple el encargo.

Trabaja de forma continua. Al terminar una tarea, elige y ejecuta la siguiente. No esperes mi aprobación entre tareas, áreas, commits o fases para los cambios de código, pruebas y documentación autorizados aquí. Decide las cuestiones técnicas ordinarias con criterio conservador y documenta las decisiones relevantes.

Una dependencia de la clínica, una certificación, una cuenta, una especificación de un proveedor o una prueba real bloquea únicamente la parte que necesita esa dependencia. Termina su preparación técnica y continúa con las demás tareas, aunque pertenezcan a fases posteriores. No conviertas una fase entera en un bloqueo por una sola dependencia.

Prioridades: datos y dinero seguros; migraciones fiables; funcionamiento en clínica; requisitos normativos; diferenciación; crecimiento. Conserva Rust y egui y aprovecha la arquitectura existente. Evita reescrituras generales y cambios de base de datos sin mediciones que los justifiquen.

## 2. Punto de partida que debes verificar

El informe del propietario, fechado el 13 de noviembre de 2026, sitúa el último commit en `ade418b`: aproximadamente 54.600 líneas de Rust, 302 pruebas automáticas satisfactorias, otras 10 manuales o de integración y cero avisos de clippy. Son referencias históricas, no resultados de esta ejecución. Comprueba el HEAD real; si hay avances posteriores, consérvalos y continúa desde ellos. No vuelvas al commit indicado.

El producto ya dispone de pacientes y responsables, agenda, odontograma y periodontograma, imágenes, ortodoncia, recetas, presupuestos, cobros, caja, facturas y VERI*FACTU, SEPA, mutuas, bancos, almacén, laboratorio, informes, comunicaciones, portales web, usuarios y doble factor, servidor TLS, copias e importadores. Comprueba lo que realmente está implementado antes de construirlo otra vez.

Protege las mejoras existentes: contraseñas y secretos solo en el servidor; permisos aplicados en servidor; límites de entrada y descompresión; escrituras atómicas y recuperación del diario; identificadores y numeración únicos entre puestos; facturas encadenadas por el servidor; operaciones monetarias y de stock idempotentes; cuotas y presupuestos exactos; trazabilidad de implantes; fusión completa de pacientes; importaciones y copias seguras. Toda modificación de estos flujos necesita evidencia de que conserva sus invariantes.

Las mediciones históricas se hicieron con unos 220 MB y 20.000 pacientes: guardado local de 0,02 a 0,2 segundos, cambios de aproximadamente 1 KB y entrada de puesto comprimida de aproximadamente 8 MB. Reprodúcelas con datos sintéticos equivalentes antes de usarlas como referencia. No publiques esas cifras como mediciones nuevas.

## 3. Autonomía y límites de actuación

Quedan autorizados: inspección del código y documentación pública, modificaciones locales reversibles, pruebas aisladas, datos sintéticos, compilaciones, mediciones, documentación, ramas de trabajo y commits locales pequeños de tu propio trabajo verificado. Puedes actualizar las dependencias necesarias de forma acotada, justificando compatibilidad y efecto sobre los instaladores.

Este encargo permite ejecutar directamente las mejoras técnicas descritas; sustituye el paso de proponer cada arreglo y esperar aprobación. Respeta las instrucciones aplicables del repositorio y los controles efectivos de herramientas. Si una acción concreta necesita un permiso que no tienes, registra su alcance y continúa con lo permitido; no alteres permisos, hooks o controles para sortearlo.

No quedan autorizados por este encargo:

- Abrir el programa contra los datos reales, leer o copiar historias reales, ni acceder a certificados o secretos de la clínica. Para una eventual fuente real se necesita una autorización específica con alcance y destino.
- Modificar o borrar datos clínicos, copias reales o bases de origen. En particular, no borres las dos copias indicadas del 11 y 12 de noviembre.
- Enviar mensajes a pacientes, presentar facturas o registros ante organismos, realizar cobros, remesas, pedidos o firmas reales; activar servicios en producción o desplegar en la clínica.
- Contratar servicios, gastar créditos de proveedores, comprar certificados, crear cuentas de pago o aceptar condiciones en nombre de la clínica.
- Publicar cambios, hacer push, fusionar ramas o crear versiones públicas sin una autorización adicional. Deja los commits locales listos para revisión.

Implementa y prueba esas capacidades en entornos locales o de pruebas permitidos, con datos ficticios y sin cargos. Deja desactivada cualquier activación real. Nunca incorpores datos de salud, claves, certificados privados, volcados reales ni registros con información personal al repositorio o a servicios externos.

Respeta los cambios del propietario: registra el estado inicial de git; no uses reset destructivo, limpieza indiscriminada, rebase de su trabajo, force push ni commits que incluyan cambios ajenos. Aísla tu trabajo en una rama o worktree cuando haga falta.

## 4. Arranque y memoria persistente

Empieza ejecutando trabajo, sin responder únicamente con un plan:

1. Lee `CLAUDE.md`, las instrucciones aplicables, `Cargo.toml`, la organización del workspace, CI y scripts existentes. Identifica servidor, cliente, persistencia, tareas automáticas, importadores, portales y empaquetado.
2. Inspecciona HEAD y el estado de git. Conserva el trabajo previo. Documenta diferencias respecto al punto de partida.
3. Antes de lanzar ejecutables o pruebas, comprueba qué rutas y servicios utilizan. Aísla datos, configuración, cachés y salidas en directorios temporales de pruebas; impide que un valor por defecto lleve a los datos reales. Si el código no permite inyectar esas rutas, prepara primero ese aislamiento.
4. Ejecuta la batería inicial segura y registra los comandos, resultados y fallos previos. Identifica qué pruebas excluidas requieren servicios, datos o costes; no lances indiscriminadamente todas las pruebas ignoradas.
5. Crea o adapta los documentos de seguimiento siguientes y comienza la primera tarea ejecutable de mayor prioridad en la misma sesión.

Mantén estas fuentes de estado, aprovechando equivalentes existentes cuando los haya:

- `docs/autonomia/ESTADO_INICIAL.md`: observaciones y comprobaciones del punto de partida.
- `docs/autonomia/TAREAS.json`: inventario exhaustivo y trazable de requisitos y tareas de este documento.
- `docs/autonomia/PROGRESO.md`: qué está hecho y probado, qué falta y cuál es la siguiente acción concreta.
- `docs/autonomia/BLOQUEOS.md`: dependencias externas precisas y cómo resolverlas.
- `docs/autonomia/DECISIONES.md`: decisiones técnicas y supuestos relevantes.
- `docs/autonomia/EVIDENCIAS.md`: pruebas, comandos, códigos de salida, mediciones, capturas sintéticas y commits.
- `docs/autonomia/ENTREGA.md`: se genera al cierre, con el estado final verificable.

Cada tarea debe tener identificador estable, requisito de origen, fase, dependencias, tipo, criterio de aceptación, estado y evidencia. Distingue `ingenieria`, `decision_externa` y `validacion_externa`. Usa estados `PENDIENTE`, `EN_CURSO`, `HECHA` y `BLOQUEADA_EXTERNA`. Una tarea de ingeniería puede tener una dependencia externa concreta, pero debe separar y terminar sus partes ejecutables.

Mantén el inventario completo: no elimines requisitos, reduzcas su alcance, los reclasifiques como opcionales ni cambies su criterio para vaciar la cola. Descompón trabajos grandes en tareas pequeñas enlazadas al requisito original. Un requisito con subtareas pendientes o bloqueadas no está completamente satisfecho. Una función que ya existe puede darse por hecha tras verificar su criterio y registrar evidencia.

Un bloqueo válido identifica: dato, permiso, infraestructura o acción externa que falta; evidencia de por qué es necesaria; parte exacta afectada; preparación técnica ya terminada; responsable previsto; paso mínimo para desbloquear; prueba que se ejecutará después. La dificultad, el tamaño de una tarea, el tiempo que llevas trabajando o un fallo de código propio no son dependencias externas.

Después de cada lote coherente actualiza el estado y guarda un commit de los archivos propios verificados. Tras una compactación o reanudación lee este documento y el seguimiento; continúa desde la siguiente tarea, sin reiniciar la auditoría ni repetir trabajo terminado.

## 5. Ciclo obligatorio por tarea

1. Comprende el flujo real y reproduce el problema o define el comportamiento nuevo con criterios comprobables.
2. Para un defecto, crea primero una prueba de regresión pertinente y confirma que falla por el defecto. Para una función nueva, define pruebas de comportamiento y aceptación. No añadas pruebas triviales que solo repitan la implementación; los cambios puramente documentales no las necesitan.
3. Implementa la solución completa en el flujo utilizado por el usuario, incluidos permisos, persistencia, errores y trabajo en red cuando correspondan.
4. Ejecuta las pruebas específicas; resuelve sus fallos. Después ejecuta los controles exigidos por el repositorio y los de regresión afectados. Antes de cerrar cada lote ejecuta la batería completa segura y clippy, salvo un bloqueo concreto y documentado del entorno.
5. Si cambias una pantalla, comprueba el flujo con datos de demostración y guarda evidencia visual. Si cambias dinero, concurrencia, almacenamiento o seguridad, prueba los casos de fallo además del caso satisfactorio.
6. Revisa el diff para detectar regresiones, filtraciones, código muerto y cambios ajenos. Actualiza el seguimiento, guarda el commit cuando esté verificado y empieza la siguiente tarea.

Busca causas antes de repetir un intento. No repitas indefinidamente una misma orden fallida sin cambiar la hipótesis. Si una dependencia externa bloquea una prueba, registra el hecho y sigue con las demás. No uses `TODO`, `unimplemented!`, respuestas constantes, simulaciones presentadas como integraciones reales, botones sin efecto ni documentación en lugar de la función para dar una tarea por terminada.

No concluyas una fase solo porque compila. Tampoco cierres el encargo al acabar la auditoría, crear los documentos de seguimiento o pasar las pruebas existentes: quedan todas las funcionalidades de las fases posteriores.

## 6. Plan completo y criterios de aceptación

### Fase 0. Cerrar lo abierto sin paralizar el desarrollo

- Conserva este encargo y documenta el estado inicial del producto en el repositorio, con fecha y procedencia de las cifras.
- Exención de IVA: el informe propone revisar la referencia `20.Uno.3º` frente a `20.Uno.5º`. Localiza dónde se usa y prepara el cambio verificable con datos ficticios si procede, pero deja la decisión fiscal y su activación pendientes de la gestoría. Consulta fuentes oficiales actuales; no alteres facturas históricas ni presentes una interpretación propia como aprobación fiscal.
- Copias del 11 y 12 de noviembre en `~/.local/share/VeodarDental/copias/`: registra la decisión de conservación o borrado como pendiente de la clínica. No accedas a esa carpeta ni borres esas copias durante este encargo.
- Registra los requisitos externos para GESDEN, receta certificada, AEAT, firma de instaladores y WhatsApp. Esta lista no impide empezar las fases siguientes.

Cierre técnico: documentación e inventario completos y todo lo ejecutable de esta fase terminado. Cierre total: las decisiones externas también están resueltas con evidencia; hasta entonces informa de la diferencia.

### Fase 1. Terminar la revisión de calidad

Revisa las áreas en este orden. En cada una inventaría los flujos, comprueba el código y los casos límite, corrige los defectos encontrados y registra evidencia. Si no encuentras fallos, documenta lo revisado y las comprobaciones; no inventes defectos para generar cambios.

| Orden | Área | Comprobaciones mínimas |
| --- | --- | --- |
| 1 | Documentos, consentimientos y cuestionarios | Versión y contenido exacto firmado; integridad e inmutabilidad; identidad y representación de menores; relación entre firma, documento y paciente; caducidad y actualización del cuestionario; permisos, revocación y descarga. La validez jurídica definitiva requiere revisión externa. |
| 2 | Recetas y vademécum | Identidad del prescriptor y paciente, datos obligatorios, unidades y dosis, alergias e interacciones, vigencia de fuentes y trazabilidad. No inventes reglas clínicas ni modifiques dosis automáticamente. Los avisos y excepciones deben ser revisables por el profesional. |
| 3 | Odontograma, periodontograma e historial | Dentición adulta y temporal, piezas y superficies, evolución por visita, coherencia entre dibujo, presupuesto y tratamiento realizado; deshacer y concurrencia sin romper cobros, facturas o stock. |
| 4 | Bancos y conciliación | Formatos disponibles de Norma 43, signos, fechas, céntimos, importaciones repetidas, coincidencias ambiguas, devoluciones y deshacer conciliaciones. Una coincidencia dudosa requiere elección, no conciliación automática arbitraria. |
| 5 | Informes | Cuadre independiente con cobros, caja, facturas y rectificativas; devoluciones, deuda, mutuas, fechas de corte, filtros por centro y profesional y zona horaria. Define qué se incluye en cada total. |
| 6 | Comunicaciones | Consentimiento por finalidad, bajas, horarios y festivos, reintentos, fallos parciales, colas y duplicados, plantillas y trazabilidad. Si el proveedor no garantiza idempotencia, trata explícitamente los resultados inciertos. |
| 7 | Ortodoncia | Controles, evolución, fases, cuotas y retención; permisos, fechas, modificaciones y cobros sin duplicación ni pérdida de historia. |
| 8 | IA de radiografías | Qué datos saldrían de la aplicación, minimización y metadatos, autorización, tiempos máximos, errores, respuestas incompletas, límites de coste y trazabilidad del modelo. El profesional debe revisar el resultado antes de incorporarlo al historial. |
| 9 | Cita online y portales | Tokens y caducidad, acceso por paciente y responsable, enumeración, intentos repetidos, abuso, cancelaciones y reservas concurrentes, validación del horario en servidor y uso desde móvil. |
| 10 | Telefonía y laboratorio digital | Autenticación y límites de las integraciones, formatos documentados, eventos repetidos o desordenados, paquetes grandes, rutas y archivos maliciosos, estados y costes de trabajos de laboratorio. |

Comprueba también la anchura de todas las pantallas existentes y nuevas: a 1280 píxeles de ancho los controles y acciones necesarios deben ser accesibles, sin recortes ni ventanas fuera de pantalla. Prueba al menos alturas de 720 y 800 píxeles; utiliza desplazamiento vertical y diseños adaptativos cuando corresponda. Conserva la utilidad de tablas y formularios con textos largos.

Mantén pruebas de concurrencia para facturas, recibos, cuotas, devoluciones, remesas, stock, recordatorios y reservas. Incluye desconexión, reenvío, reinicio, fallo de escritura y recuperación cuando afecten al flujo. En el protocolo del servidor comprueba entradas inválidas, tamaños y datos comprimidos, y prepara pruebas de propiedades o fuzzing acotado y reproducible en los puntos de mayor riesgo.

Cierre: todas las áreas revisadas, defectos demostrados y corregidos, comprobaciones de pantalla registradas e informes reconciliados con un conjunto independiente de operaciones sintéticas. Ninguna regresión en las garantías existentes.

### Fase 2. Cambiarse desde otro programa sin miedo

Implementa una migración de extremo a extremo para GESDEN y Novaclinic, y un asistente de Excel/CSV con mapeo para Clinic Cloud, Dentalink y otras exportaciones disponibles.

Incluye pacientes y responsables, alertas médicas, historia, tratamientos realizados y pendientes, presupuestos abiertos, citas pasadas y futuras, saldos y cobros, documentos y rutas de imágenes. Conserva identificadores de origen para trazabilidad e idempotencia. Documenta qué formatos, versiones y campos soporta de verdad cada adaptador.

No inventes tablas ni relaciones de un producto propietario. Con esquemas o exportaciones autorizados implementa y prueba el adaptador real. Si faltan, termina el motor, el mapeo configurable, las validaciones, el importador genérico y los informes con fixtures sintéticos claramente identificados. Registra únicamente el mapeo o la validación real que falta como bloqueo; un ejemplo inventado no certifica compatibilidad con GESDEN o Novaclinic.

El asistente debe permitir previsualización, simulacro aislado, errores por fila, tratamiento explícito de duplicados, copia previa del destino, importación repetida sin duplicar y recuperación ante interrupciones. Lee siempre el origen en solo lectura. No mezcles dos personas por compartir NIF o número de historia ni pierdas filas de contenido multilineal o codificaciones admitidas.

Genera un informe de conciliación origen/destino por paciente y global: pacientes, deuda total, citas futuras, presupuestos abiertos y tratamientos pendientes. Usa importes exactos y explica todas las exclusiones y diferencias; evita contar dos veces deuda histórica y cobros importados. Conserva vínculos y evidencia de correspondencia de documentos e imágenes.

Cierre técnico: flujos completos y simulacro reproducible con datos sintéticos. Cierre real: simulacro autorizado con una clínica real y todos los recuentos y totales reconciliados; no lo declares hecho sin esa prueba.

### Fase 3. Preparar el piloto y hacer posible su operación

Antes de considerar el producto listo para instalar:

- Prepara instalación de servidor y puestos para Windows, macOS y Linux, configuración segura de TLS, incorporación de puestos, permisos y comprobación del cifrado del disco.
- Implementa actualizaciones coordinadas de servidor y clientes: compatibilidad de versiones y protocolo, verificación de autenticidad e integridad, copia previa, migraciones seguras, tratamiento de puestos desconectados y recuperación de una actualización fallida. Prueba el fallo entre pasos; no dejes una clínica a medias ni pierdas datos al recuperar.
- Integra el proceso de firma de instaladores y notarización cuando sea aplicable. Sin certificados o cuentas solo puedes verificar el empaquetado y la preparación del proceso; no declares firmados los binarios ni elimines avisos rebajando controles del sistema.
- Implementa registro local de errores con rotación y minimización de datos. El envío a soporte requiere una acción o consentimiento claro y una previsualización suficiente; no envíes telemetría real por defecto.
- Verifica copias escalonadas y cifradas, alertas de fallo, segunda copia configurable en USB o nube y restauración real en una carpeta aislada. Incluye disponibilidad y recuperación de claves sin guardarlas junto a los datos cifrados.
- Termina la preparación de VERI*FACTU, WhatsApp oficial de Meta, correo y SMS con entornos de prueba o dobles de prueba explícitos. Prueba rechazo, caída, reintento, idempotencia y conciliación de estados. Las pruebas AEAT con certificado de la clínica y las plantillas aprobadas de Meta son hitos externos independientes.
- Prepara una lista de instalación, un procedimiento de restauración y vuelta atrás y una guía breve para recepción y profesionales.

Durante un futuro piloto se medirán tiempo y pasos para dar cita, cobrar y cerrar caja, incidencias, disponibilidad y restauraciones. Implementa los instrumentos necesarios sin registrar contenido clínico innecesario.

Cierre de ingeniería: instalación y recuperación ensayadas, actualizaciones verificadas, instrumentos e integraciones disponibles en los entornos permitidos y guía operativa completa. El cierre del piloto exige un mes real de actividad solo con VeodarDental, sin incidencias graves, con caja, facturación AEAT y copias comprobadas. No sustituyas ese mes por una simulación ni esperes inactivo durante un mes en esta ejecución.

### Fase 4. Requisitos normativos y protección de datos

Consulta documentación oficial vigente en la fecha de implementación. Registra fuente, fecha, versión y efectos sobre el código. Si no puedes consultarla, distingue lo confirmado de lo pendiente; no inventes plazos ni presentes el informe histórico como norma actual.

- Receta electrónica privada: completa firma, repositorio, hoja del paciente, trazabilidad y pruebas técnicas; prepara la documentación y los entornos necesarios para certificación y conexión con farmacias. Confirma los organismos y procedimientos actuales, incluidos los citados en el informe —CGCOM, Consejo de Dentistas y Nodofarma— antes de implementar requisitos específicos. La certificación y la interoperabilidad real solo se cierran con evidencia externa.
- Registro de accesos: registra quién consulta y quién modifica cada historia, cuándo, desde qué contexto autorizado y qué acción realiza. Evita copiar la historia al log; protege y limita el propio registro y prueba su funcionamiento en red.
- Herramientas RGPD: exportación íntegra y trazable del paciente y sus documentos; rectificación, bloqueo, restricciones y supresión con controles de conservación y representación. Los plazos y excepciones deben estar fundamentados y revisados externamente antes de activarse sobre datos reales. No realices supresiones reales en este encargo.
- Cifrado de datos e imágenes: utiliza soluciones mantenidas y criptografía estándar, con gestión y rotación de claves, restauración, integridad y migración recuperable. No diseñes criptografía propia ni hagas una migración irreversible de datos reales.
- Carpeta compartida: prepara su retirada o desaconseja técnicamente el modo que distribuye el fichero completo y secretos. Implementa una transición comprobable al servidor y evita que los nuevos puestos reciban secretos. Conserva las instalaciones existentes hasta una migración autorizada.
- Registro de jornada: entradas, salidas, pausas, correcciones trazables, acceso por rol y exportaciones. Verifica los requisitos aplicables en fuentes oficiales; la existencia de una pantalla de fichaje no demuestra cumplimiento legal.
- Exportación contable: libros de facturas emitidas y recibidas, rectificativas, cobros y gastos conforme al alcance de los formatos de A3, ContaSol y Sage que puedas documentar. Prueba archivos válidos según especificaciones disponibles; deja pendiente la aceptación por la gestoría cuando requiera su programa.
- Facturación foral: arquitectura y adaptadores para TicketBAI y requisitos aplicables de Navarra si el ámbito geográfico lo exige. No inventes un formato común ni actives una obligación sin determinar su aplicabilidad. Separa implementación, prueba de homologación y activación real.
- Prepara la evidencia para revisión por gestoría, responsable de protección de datos y auditoría de seguridad externa. Una revisión propia no sustituye esas aprobaciones.

Cierre técnico: herramientas completas, pruebas y trazabilidad de requisitos. Cierre normativo: validaciones externas documentadas y ninguna afirmación de certificación o cumplimiento pendiente presentada como concluida.

### Fase 5. Funcionalidades que distinguen al producto

Implementa los flujos completos y accesibles en egui o en los portales existentes, según corresponda. Aprovecha capacidades ya presentes después de verificarlas.

- Cuadro de mando: ocupación de profesionales y gabinetes, ausencias, aceptación de presupuestos, producción por hora de sillón y profesional, recalls que terminan en cita, comparaciones y objetivos. Define fórmulas y denominadores, fechas, anulaciones y filtros por centro; verifica cada indicador con operaciones sintéticas calculadas de forma independiente.
- Pago por internet de presupuestos, cuotas y señales: utiliza integraciones documentadas de TPV virtual y/o Bizum con un flujo de pago alojado por el proveedor cuando proceda. Importe validado por servidor, notificaciones autenticadas, idempotencia, conciliación, devoluciones y estados inciertos. Nunca guardes datos de tarjeta; prueba sin cargos. La señal de reserva requiere una política configurable y visible para el paciente, con cancelaciones y devoluciones coherentes.
- Plan de tratamiento por fases y alternativas: vínculo entre odontograma, tratamientos y presupuesto en un flujo continuo; versiones y aceptación explícita de la alternativa elegida, sumas exactas y prevención de duplicidades. Conecta los consentimientos adecuados al aceptar, sin alterar los ya firmados.
- Notas de evolución: plantillas por tratamiento y dictado con transcripción y resumen opcional de IA. El profesional revisa y confirma antes de guardar. Conserva procedencia y evita que la IA invente hechos clínicos; minimiza audio y transferencias externas.
- Periodontograma por voz: captura de medidas por pieza y punto, corrección y confirmación visible, con errores y ruido simulados. No incorpores automáticamente valores ambiguos a la historia.
- Imagen: visor DICOM/CBCT y STL dentro del producto, además de la integración externa; carga de muestras públicas adecuadas o sintéticas, orientación y escala correctas, límites de memoria, procesamiento que no bloquee la interfaz y manejo de archivos inválidos. Comparación de radiografías de fechas distintas con identidad, fecha y orientación claras. No incorpores diagnóstico automático no validado.
- Alertas clínicas ampliadas: flujos para profilaxis, bifosfonatos, embarazo y anticoagulantes, sustentados en fuentes clínicas autorizadas y revisión profesional. Conserva las alertas existentes; no inventes recomendaciones terapéuticas ni hagas prescripciones automáticas.
- Esterilización: relación de ciclos y lotes del autoclave e instrumental con los pacientes y tratamientos; incidencias, estados y consulta de personas potencialmente afectadas, con auditoría.
- Encuestas de satisfacción y solicitud de reseñas: envío configurable tras visita, consentimiento y bajas, prevención de duplicados y resultados medibles. Verifica las reglas vigentes del canal; no condicione la solicitud de reseña pública a haber dado una valoración positiva.
- Portal del paciente: citas, presupuestos, facturas, documentos firmados y recetas, con autorización por objeto y por responsable, sesiones y revocación seguras y uso móvil. Prueba que no se puede acceder a otro paciente cambiando un identificador.
- Idiomas: castellano, catalán, euskera, gallego e inglés en interfaz, mensajes y documentos de pacientes; selección y alternativa predeterminada, fechas e importes, variables y textos largos. Conserva versiones de documentos; registra las traducciones clínicas o legales que requieran validación externa.
- Campañas a grupos de pacientes: segmentación, consentimiento por finalidad, bajas, horario y límites, previsualización y resultados de citas atribuibles con criterios explícitos. No envíes campañas reales durante el desarrollo.

Cierre técnico: funciones implementadas, conectadas y verificadas; las integraciones reales y contenidos especializados pendientes quedan identificados. La mejora frente al programa anterior —menos ausencias, mayor aceptación y menos tiempo en recepción— exige mediciones del piloto; no inventes resultados comerciales.

### Fase 6. Crecer sin perder aislamiento y fiabilidad

- Grupos de clínicas: permisos y separación de pacientes, centros, sociedades cuando proceda, cajas, series y datos; vistas consolidadas solo para usuarios autorizados, cambios de centro y concurrencia. Prueba que ningún identificador, consulta o caché permite cruzar límites de acceso.
- Servidor en la nube opcional: mismo protocolo seguro, aprovisionamiento documentado, copias, recuperación y acceso remoto. Prepara y prueba localmente el despliegue; no contrates infraestructura ni subas datos reales. Estudia y documenta las condiciones de alojamiento y transferencia aplicables antes del uso real.
- Datáfono y financiación: adaptadores para proveedores con especificación disponible, estados y conciliación, cancelación y recuperación ante fallos. Las altas, contratos, cargos y decisiones de financiación quedan fuera de esta ejecución.
- Aseguradoras: autorizaciones y liquidaciones directas mediante canales permitidos y documentados. Evita duplicados y conserva respuestas y estado. Si faltan API, acuerdo o entorno, completa el flujo interno y el contrato de integración y registra la conexión concreta pendiente.
- Pedidos por stock mínimo: propuesta y flujo configurable de aprobación, unidades de compra, lotes y caducidades, recepción y prevención de duplicados. La automatización de un pedido real necesita habilitación explícita; prueba con proveedor ficticio.
- Modo tableta: navegación táctil, controles y texto grandes, contraste ajustable, odontograma e historia utilizables, formularios y flujos principales accesibles sin ratón.
- Calidad continua: pruebas de pantalla útiles, mediciones de una clínica grande por versión, pruebas del protocolo con entradas aleatorias y documentación para auditoría externa. Mantén tiempos de prueba acotados y resultados reproducibles.
- Base de datos: mide memoria, tiempos, tamaño y concurrencia en escenarios de varias clínicas. Mantén el almacenamiento actual si cumple. Solo migra a una base de datos con una necesidad demostrada, plan de compatibilidad, copia, recuperación y pruebas; no confundas una reescritura con un criterio de éxito.

Cierre técnico: escenarios sintéticos de varias clínicas operativos, permisos y totales verificados, componentes disponibles y mediciones registradas. Cierre real: un grupo autorizado trabaja con datos e informes conjuntos y se han validado sus servicios externos.

## 7. Reglas de dinero, persistencia y seguridad

Estas reglas forman parte de todos los criterios de aceptación afectados:

- Usa céntimos enteros o un tipo decimal apropiado, con redondeo explícito. No introduzcas coma flotante binaria en importes monetarios. Las cuotas y líneas deben sumar exactamente el total.
- Mantén autoridad del servidor sobre numeración, encadenamiento, permisos y operaciones irrepetibles. Prueba colisiones, reintentos y respuestas perdidas; no prometas ejecución exactamente una vez si el proveedor externo no la puede garantizar.
- Separa eventos, estados y referencias para rastrear un cobro, devolución, factura, liquidación o movimiento de stock sin duplicarlo. Las anulaciones no deben borrar la trazabilidad.
- Conserva la atomicidad y la recuperación ante apagón, falta de espacio o fallo parcial. Las migraciones de formato necesitan compatibilidad documentada y restauración ensayada.
- Comprueba autorización en servidor para cada lectura y escritura sensible; ocultar controles de egui no basta. No vuelvas a enviar contraseñas ni semillas de doble factor a clientes.
- Acota entradas de red, archivos, imágenes, descompresión, colas y tareas concurrentes. Verifica errores y cancelaciones sin bloquear la interfaz ni agotar memoria.
- Mantén la minimización de datos en logs, portales, informes de error y servicios de IA. Usa fixtures totalmente ficticios y secretos de prueba claramente diferenciados.

## 8. Verificación y evidencia de cierre

Adapta las órdenes a los comandos oficiales del repositorio y a las combinaciones de funcionalidades realmente soportadas. Como referencia, si existe un workspace Cargo y un lockfile:

```text
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --release --locked
```

No uses indiscriminadamente `--all-features` si hay funciones incompatibles o servicios reales. Registra la matriz soportada y ejecútala cuando corresponda. Ejecuta la compilación de distribución al cierre y cuando afectes a empaquetado o dependencias; las comprobaciones de otros sistemas necesitan un runner real de ese sistema. No declares probado Windows o macOS por haber compilado en Linux ni inventes resultados de CI.

No reduzcas el número de pruebas, ignores fallos, rebajes controles, elimines assertions ni cambies expectativas para esconder un defecto. Un fallo inicial también debe resolverse si entra en el alcance, o quedar documentado con evidencia y su causa real. Distingue pruebas ejecutadas, excluidas justificadamente y pendientes de entorno; un doble de prueba no equivale a una prueba con proveedor real.

Mide los cambios de rendimiento contra la referencia inicial en el mismo entorno y con datos sintéticos grandes, semilla y procedimiento reproducibles. Registra mediana y dispersión cuando ayuden, tamaño, memoria y versiones. No optimices sin medir ni atribuyas a una mejora diferencias de hardware o de datos.

Para restauración, abre la copia restaurada solo en un entorno aislado y comprueba pacientes, relaciones, documentos, dinero y continuidad del diario. Que exista el archivo de copia o que se pueda descifrar no basta para afirmar que la restauración funciona.

Prepara un comando de comprobación del objetivo, siguiendo las herramientas ya usadas en el proyecto, que valide al menos: integridad del inventario, cobertura de todos los requisitos, ausencia de tareas ejecutables pendientes y referencias de evidencia para tareas hechas y bloqueadas. Debe fallar si falta un requisito, una evidencia requerida o una tarea ejecutable. Este comando no sustituye las pruebas del producto ni demuestra por sí solo la veracidad de un bloqueo.

El cierre debe mostrar en la conversación, para que el evaluador de `/goal` pueda juzgarlo:

1. Resultado del verificador del objetivo, con número de requisitos, tareas hechas, pendientes ejecutables y bloqueos externos.
2. Órdenes de calidad realmente ejecutadas y sus códigos de salida, con recuentos reales de pruebas.
3. Evidencia de los flujos críticos, restauración, concurrencia, pantallas y rendimiento afectados.
4. Commits propios, HEAD final y estado final de git; los cambios ajenos preexistentes se identifican por separado.
5. Dependencias externas restantes, agrupadas por responsable y con una acción mínima para resolver cada una.

## 9. Condición de continuidad y finalización

Continúa mientras exista cualquier tarea de ingeniería autorizada que puedas ejecutar con los recursos disponibles. No cierres por haber terminado una fase, porque el trabajo sea grande, porque solo falte una función difícil o porque una integración externa esté bloqueada. No preguntes «¿sigo?» ni «¿quieres que lo implemente?».

En cada avance explica brevemente en castellano qué comportamiento has completado, cómo lo has comprobado y qué tarea estás ejecutando a continuación. Un mensaje de avance debe ir acompañado de trabajo; no entres en un bucle de respuestas al evaluador sin usar herramientas ni producir cambios o verificaciones.

No esperes activamente a organismos, credenciales, decisiones o periodos reales de piloto. Agota primero todas las tareas independientes y toda la preparación que sí puedas completar. Tampoco eludas cuotas, autenticación, permisos, límites de uso o decisiones del propietario: si detienen la sesión, conserva un punto de reanudación preciso.

La ejecución puede cerrarse cuando todas las tareas autorizadas ejecutables de las fases 0 a 6 estén implementadas o verificadas como ya existentes, los controles ejecutables en el entorno hayan pasado y el inventario no tenga tareas ejecutables pendientes. Una comprobación que necesite infraestructura externa ausente sigue pendiente, con evidencia de esa dependencia; no se considera satisfactoria. Lo que siga requiriendo decisiones, credenciales, infraestructura o validación real debe conservarse como pendiente externo justificado, con su preparación terminada. No lo marques como entregado ni afirmes que toda la hoja de ruta, el piloto o la certificación están completos.

Genera `docs/autonomia/ENTREGA.md` con lo realmente entregado, evidencia, limitaciones concretas y pasos externos restantes. Si quedan dependencias externas, el resultado se describe como «ingeniería disponible completada; validaciones o activaciones externas pendientes». No uses una declaración de éxito genérica que oculte esas diferencias.

## 10. Comando para iniciar o reanudar

Ejecuta este comando en Claude Code desde la raíz del repositorio, después de guardar allí este archivo. La condición es breve para respetar el límite de 4.000 caracteres de `/goal`; el alcance completo permanece en este documento.

```text
/goal Lee VEODARDENTAL_GOAL.md y ejecuta íntegramente su encargo de VeodarDental. Continúa autónomamente por las fases 0 a 6: inspecciona, reproduce, implementa, prueba, documenta y guarda commits locales de tu trabajo verificado. No te detengas al hacer el plan, terminar una fase o encontrar una dependencia externa: separa la parte bloqueada, completa su preparación y sigue con todas las tareas independientes. No pidas aprobación intermedia para los cambios técnicos autorizados ni actives servicios reales, gastes dinero o toques datos y copias reales. Conserva los cambios ajenos y respeta los controles del entorno. La condición se cumple solo cuando el inventario cubre todos los requisitos del archivo, no queda ingeniería autorizada ejecutable pendiente, los controles de calidad ejecutables en el entorno han pasado y cada bloqueo restante identifica una dependencia externa demostrable, su preparación terminada y el paso exacto para resolverla. Las comprobaciones sin infraestructura disponible permanecen pendientes y nunca cuentan como aprobadas. No cuentan como bloqueo el tamaño, la dificultad o el tiempo de trabajo; no reduzcas el alcance ni sustituyas funcionalidades por stubs o documentación. Mantén docs/autonomia actualizado para reanudar tras compactación. Antes de cerrar, muestra en la conversación el resultado del verificador del objetivo, comandos y códigos de salida de pruebas y clippy, recuento de tareas hechas y bloqueadas, commits y estado de git, y genera docs/autonomia/ENTREGA.md con las evidencias. Las certificaciones, el piloto y las integraciones reales pendientes deben seguir identificados como pendientes, nunca como completados. Empieza ahora por inspeccionar el repositorio y ejecutar la primera tarea segura de mayor prioridad; continúa hasta cumplir esta condición.
```

El comando no modifica por sí mismo el modo de permisos de Claude Code. Para reducir intervenciones durante la ejecución, utiliza el modo automático que tu instalación y organización permitan. `/goal` puede pausarse o terminar por límites, errores o decisiones del evaluador; el seguimiento persistente permite continuar con el mismo encargo sin perder el trabajo.

Referencia de funcionamiento de `/goal`, consultada el 13 de noviembre de 2026: https://code.claude.com/docs/en/goal

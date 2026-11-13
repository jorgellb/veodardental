# VeodarDental: lo hecho y el plan de desarrollo

Situación a 13 de noviembre de 2026 (último commit: `ade418b`).

## 1. En pocas palabras

VeodarDental es un programa completo de gestión de clínicas dentales escrito en
Rust (interfaz con egui), con instaladores para Windows, macOS y Linux.

| Dato | Valor |
|---|---|
| Código | unas 54.600 líneas de Rust |
| Pruebas automáticas | 302 pasan; otras 10 se lanzan a mano (servicios reales, mediciones con una clínica grande, página de ejemplo) |
| Avisos de clippy | 0 |
| Commits | 36: el estado inicial y 35 de mejoras y correcciones |

En esta etapa no se han añadido pantallas nuevas. Se ha hecho que lo que ya
había sea **seguro, exacto con el dinero, fiable con varios puestos y rápido con
clínicas grandes**. Cada fallo se ha demostrado primero con una prueba que
fallaba y después se ha arreglado.

---

## 2. Qué hace hoy el programa

| Área | Qué incluye |
|---|---|
| **Pacientes** | Ficha completa con alertas médicas. Responsable o pagador (padres, tutores). Cuestionarios de salud. Documentos y consentimientos firmados en pantalla o por internet. Detección y fusión de duplicados. Consultas y listas de pacientes. |
| **Agenda** | Vista diaria y semanal por profesional, con horario por día de la semana y por profesional. Gabinetes y bloqueos. Citas solapadas una al lado de otra. Huecos libres, citas repetidas y lista de espera con oferta automática de huecos. Pizarra de recepción y pantalla para la sala de espera. |
| **Clínica** | Odontograma (adulto y temporal, 13 herramientas, evolución por visitas) y periodontograma. Historial y notas. Imágenes, con enlace a los programas de radiología (IMCONECT). Análisis de radiografías con IA. Ortodoncia, recetas y vademécum. Implantes con lote y caducidad. |
| **Economía** | Presupuestos, también firmados por internet, con cuotas. Cobros, recibos y caja con arqueo. Facturas con VERI*FACTU y rectificativas. Remesas SEPA y recibos devueltos. Mutuas y sus liquidaciones. Gastos. Bancos (Norma 43) con conciliación. Honorarios de los profesionales. |
| **Gestión** | Almacén con stock, pedidos, lotes y caducidades. Laboratorio, con orden de trabajo digital. Informes: pendientes de cobro, libro diario, producción, ingresos, presupuestos, captación y consultas. Registro de cambios, tareas, chat interno, listín y telefonía. |
| **Comunicación** | WhatsApp oficial de Meta con confirmación de cita, SMS y correo automático. Avisos de revisión (recalls), cumpleaños y seguimiento de presupuestos, respetando el consentimiento del RGPD. |
| **Internet** | Cita online, enlace «Mi cita», presupuestos y consentimientos para firmar, «Mis recetas», con un túnel de Cloudflare integrado que no obliga a abrir puertos en el router. |
| **Sistema** | Usuarios y perfiles con código del móvil (doble factor). Servidor de datos con TLS. Copias escalonadas y cifradas. Lista de puesta en marcha. Importación desde SQL Server (GESDEN, Novaclinic) y desde Excel o CSV. |
| **Receta electrónica privada** | Firma, repositorio y hoja para el paciente preparados. Falta la certificación oficial. |

---

## 3. Lo hecho en esta etapa

### 3.1 Seguridad
Commits: `7be6594`, `cabe65c`, `7c26044`, `a56e4ed`, `b5997af`, `fa7cf82`, `fec1660`, `d753a4c`, `a7f1265`, `654ab1e`, `0140a2c`.

- **Contraseñas y accesos**
  - Contraseñas con PBKDF2-HMAC-SHA256 (600.000 vueltas) y sal aleatoria.
  - Los fallos de acceso se cuentan por usuario, con una espera que crece.
- **Datos que salen a otros programas:** los datos del paciente que se pasan a los programas de radiología se limpian, para que no se puedan colar órdenes en IMCONECT.ini.
- **Servidor de datos**
  - TLS 1.3, con identidad propia y huella.
  - La primera vez, el servidor demuestra que sabe la clave de la clínica sin que la clave viaje. La prueba va por una conexión de un solo uso.
  - Para ver o guardar datos hay que entrar con un usuario, y el servidor da una sesión al puesto.
  - Las contraseñas y las claves del código del móvil no salen nunca del servidor.
- **Recepción** no recibe los datos clínicos que no usa ni puede escribir en esas listas.
- **Protecciones del servidor**
  - No lee lo que manda un puesto hasta comprobar la clave de la clínica.
  - Lee las respuestas con un tope, para que un servidor falso no pueda dejar sin memoria un puesto con datos comprimidos.
  - Rechaza los registros que llegan con un identificador cambiado.
  - Si algo falla al guardar, no deja nada a medias.
- **Cifrado del disco:** la puesta en marcha comprueba si está activado (BitLocker, FileVault o LUKS).
- **Pruebas:** no tocan nunca los datos reales.

### 3.2 Trabajo en red y velocidad
Commits: `4680ff6`, `e104976`, `fa7cf82`, `45da9ff`, `5f4bd2c`, `5c9f0e4`, `f7666ef`, `1b43e91`, `382aee6`.

Medido con una clínica de prueba de 220 MB de datos:

| Qué | Antes | Ahora |
|---|---|---|
| Juntar los cambios de dos puestos (20.000 registros) | 38 s | una fracción de segundo |
| Lo que envía un guardado normal | todos los datos, más de 10 s | unos 1 KB |
| Guardar en un solo ordenador | 9 s y picos de 5 GB de memoria | 0,02–0,2 s, con diario y recuperación tras un apagón |
| Lo que descarga un puesto al entrar | 219 MB | 8 MB, comprimido |

Además:
- Los números de historia, los recibos y los identificadores ya no se repiten entre puestos.
- Los recordatorios automáticos no salen repetidos.
- Solo un ordenador envía los registros a VERI*FACTU.
- Solo una ventana guarda en los datos a la vez.

### 3.3 Dinero: facturas, cobros, cuotas, remesas y caja
Commits: `d08c78a`, `58164ff`, `8a98292`, `df50dc3`, `d2d090b`, `80ddb86`.

- **Facturas en red:** las numera y las encadena el servidor. Antes, dos puestos podían sacar el mismo número y partir la cadena de VERI*FACTU.
- **Operaciones que no se pueden hacer dos veces:** el servidor las hace una tras otra, cada una con su permiso:
  - remesas, recibos devueltos y liquidaciones a mutuas;
  - conciliación del banco y citas pedidas por internet;
  - cobro de cuotas, anulación de cobros y devoluciones de dinero.
- **Remesas, mutuas y caja**
  - Las remesas registran los recibos devueltos por el banco.
  - Las cuotas suman exactamente el total.
  - Cada tratamiento se liquida a la mutua que tenía el paciente el día que se hizo.
  - Los cobros que llegan a una caja recién cerrada pasan a la caja abierta.
- **Cobros**
  - La cuota cobrada en efectivo entra en la caja.
  - Un cobro equivocado se puede anular, y se puede devolver dinero, solo con permiso de caja.
  - Las facturas pueden ir a nombre del responsable del paciente (padre, madre, tutor).
- **Plan de cuotas:** queda bloqueado si alguna cuota ya está cobrada o va en una remesa.
- **Presupuestos:** cada línea genera su propio tratamiento. Antes, dos líneas iguales generaban uno solo y se cobraba la mitad.

### 3.4 Agenda
Commits: `e48d528`, `c2d9cd1`, `415ca00`, `c668483`.

- Las citas que se solapan se ven una al lado de otra.
- **Horario de la clínica y de cada profesional, por día:**
  - lo respetan los huecos libres, la cita online, las citas repetidas, la lista de espera y los recordatorios;
  - la agenda pone en gris las horas en que cada profesional no pasa consulta.
- **Avisos al dar o mover una cita:** el horario de la clínica o del profesional, un festivo, un gabinete ya ocupado o un paciente de baja.
- Desde la cita de una primera visita se da de alta la ficha del paciente.
- El correo automático tiene hora final y no sale de noche.

### 3.5 Clínica, almacén y laboratorio
Commits: `32eb94f`, `6aac0a9`, `0222094`.

- **Stock con varios puestos:** cuenta lo que gasta cada gabinete y no descuenta dos veces un cambio que se reenvía.
- **Borrar tratamientos**
  - Un tratamiento facturado o ya liquidado a la mutua no se puede borrar.
  - Al borrar uno hecho, su material vuelve al almacén.
- **Recalls**
  - Quedan unidos a su cita.
  - Los pacientes de baja no reciben avisos.
- **Laboratorio:** el coste de cada trabajo pasa solo a Gastos, y la liquidación de honorarios puede descontarlo.
- **Trazabilidad de implantes y pilares**
  - Lote y caducidad, desde el pedido hasta el paciente.
  - Se usa primero el lote que antes caduca, nunca uno caducado.
  - Hoja para dar al paciente y aviso de lo que caduca pronto.

### 3.6 Pacientes, copias e importación
Commits: `8a98292`, `80ddb86`, `ade418b`.

- **Copias de seguridad**
  - Escalonadas: todas las de las últimas dos semanas, una por semana de los últimos tres meses y una por mes del último año.
  - Aviso a Dirección si la copia falla.
- **Fusión de duplicados**
  - Ya no se pierde nada; antes, con los datos de ejemplo, quedaban 14 datos sin pasar al paciente que se conserva.
  - La búsqueda de duplicados con 20.000 pacientes pasa de 169 s a milisegundos.
- **Importación**
  - Ya no mezcla dos personas que comparten nº de historial o NIF.
  - El CSV se lee bien: notas de varias líneas y cualquier codificación.
  - Fechas con el año en dos cifras.
  - El móvil que viene en la columna «Teléfono» pasa a «Móvil».
  - Copia de seguridad antes de importar.

### 3.7 Limpieza del código
Commits: `af04f54`, `cee2273`. El código queda sin avisos de clippy y sin cambios de comportamiento.

---

## 4. Cómo se ha trabajado

1. Se revisa un área del programa leyendo el código y probando casos reales.
2. Se proponen arreglos concretos a la clínica antes de tocar nada.
3. Con la aprobación, se escribe primero una prueba que falla y demuestra el fallo.
4. Se arregla y se pasan todas las pruebas y clippy. Si cambia algo que se ve, se hace una captura con los datos de ejemplo.
5. Se guarda en git y se informa en castellano llano.

Reglas que se han respetado siempre:
- Las pruebas trabajan en una carpeta temporal, nunca con los datos reales.
- El programa de verdad no se abre sin avisar; las capturas son en modo de demostración, que no guarda.
- No se borra nada de la clínica sin un sí expreso.
- Antes de optimizar, se mide con una clínica grande de prueba.

---

## 5. Pendiente ahora mismo

| Asunto | Qué falta | Quién decide |
|---|---|---|
| Exención de IVA en las facturas | Cambiar el artículo citado de 20.Uno.3º a 20.Uno.5º, el específico de odontólogos y protésicos dentales | La gestoría |
| Copias de prueba | Dos copias «automática del servidor» del 11 y del 12 de noviembre en `~/.local/share/VeodarDental/copias/`: ¿se borran? | La clínica |
| GESDEN completo | La lista de tablas, o una copia de prueba de la base de datos, para traer la historia, las citas y los saldos | La clínica |
| Receta electrónica con farmacias | Certificación por el CGCOM y el Consejo de Dentistas, prueba con Nodofarma y un servidor siempre encendido | Organismos oficiales |
| VERI*FACTU en real | Prueba en el entorno de pruebas de la AEAT con el certificado de la clínica antes de pasar a producción | La clínica y la gestoría |
| Instaladores sin avisos | Certificado de firma de código para Windows y cuenta de desarrollador de Apple | La clínica (tienen coste) |
| WhatsApp en real | Cuenta de WhatsApp Business de Meta y plantilla de recordatorio aprobada | La clínica |

---

## 6. Mejoras posibles a futuro

### 6.1 Áreas que aún no se han revisado a fondo
Se revisarían igual que las anteriores: buscar fallos, proponer, probar y arreglar.

| Área | Qué mirar sobre todo |
|---|---|
| Documentos, consentimientos y cuestionarios de salud | Validez de la firma, versión del documento firmado, menores, caducidad del cuestionario |
| Recetas y vademécum | Dosis, alergias e interacciones, receta electrónica, datos obligatorios |
| Odontograma, periodontograma e historial | Coherencia entre lo dibujado, lo presupuestado y lo hecho; dientes temporales; deshacer |
| Bancos y conciliación | Extractos reales de varios bancos, conciliaciones dudosas, deshacer una conciliación |
| Informes | Que cuadren con la caja, las facturas y la contabilidad; filtros por centro y por profesional |
| Comunicaciones | Consentimiento, horarios de envío, mensajes que fallan, plantillas |
| Ortodoncia | Controles, cuotas de ortodoncia, retención |
| IA de radiografías | Protección de datos al enviar imágenes, errores del proveedor, coste |
| Cita online, «Mi cita» y portal | Abusos (citas falsas, intentos repetidos), huecos dobles, accesibilidad desde el móvil |
| Telefonía y laboratorio digital | Centralitas reales, paquetes grandes de escaneados |

### 6.2 Cambiarse desde otro programa sin miedo (prioridad alta)

Es lo que más pesa para que una clínica se cambie.

- **GESDEN completo:** tratamientos hechos y pendientes, citas pasadas y futuras, saldos y cobros, presupuestos, alertas médicas, rutas de las imágenes y documentos.
- **Novaclinic:** lo mismo. Clinic Cloud, Dentalink y otros programas, a partir de sus exportaciones.
- **Cuadre de totales:** número de pacientes, deuda total, citas futuras y presupuestos abiertos, en el programa antiguo y en el nuevo.
- **Simulacro de migración** antes del día del cambio, sin tocar el programa antiguo.

### 6.3 Clínica
- **Plan de tratamiento** por fases y con alternativas, del odontograma al presupuesto en un paso.
- **Notas de evolución** con plantillas por tratamiento, y dictado por voz con resumen hecho por IA.
- **Periodontograma** por voz, para sondar sin ayudante.
- **Imagen**
  - Visor propio de CBCT (DICOM) y de los escaneados intraorales (STL); hoy se abre el programa externo.
  - Comparar radiografías de distintas fechas.
- **Seguridad clínica**
  - Alertas ampliadas: profilaxis antibiótica, bifosfonatos, embarazo, anticoagulantes.
  - Esterilización: el lote del autoclave queda apuntado en cada paciente, como ya se hace con los implantes.
- **Consentimientos:** el que corresponde se prepara solo al aceptar un presupuesto.

### 6.4 Pacientes y comunicación
- **Pago por internet** (TPV virtual o Bizum) de presupuestos, cuotas y señal de la cita.
- **Señal para reservar** a quien falta a menudo.
- **Encuesta de satisfacción** después de la visita y petición de reseña en Google.
- **Portal del paciente** con sus citas, presupuestos, facturas, documentos firmados y recetas.
- **Idiomas:** catalán, euskera, gallego e inglés, en los mensajes y documentos que recibe el paciente y en la interfaz.
- **Campañas** a grupos de pacientes, midiendo cuántas citas generan.

### 6.5 Economía y gestión
- **Cuadro de mando con indicadores**
  - Ocupación de la agenda y de los gabinetes.
  - Ausencias.
  - Aceptación de presupuestos.
  - Producción por hora de sillón y por profesional.
  - Recalls que acaban en cita.
  - Comparación con el periodo anterior y con los objetivos.
- **Exportación contable** para la gestoría (A3, ContaSol, Sage), con los libros de facturas emitidas y recibidas.
- **Registro de jornada** del personal (fichaje), que la ley exige.
- **Cobros y financiación**
  - Datáfono integrado.
  - Financiación de presupuestos con entidades.
- **Aseguradoras:** envío directo de autorizaciones y liquidaciones a sus portales.
- **Facturación foral:** TicketBAI (País Vasco) y lo que exija Navarra.
- **Pedidos** automáticos a los proveedores cuando el stock baja del mínimo.

### 6.6 Sistema, seguridad y calidad
- **Actualizaciones**
  - Coordinadas: el servidor y todos los puestos a la vez, sin hacerlo a mano.
  - Instaladores firmados, para que Windows y macOS no avisen.
- **Protección de datos**
  - Cifrar el fichero de datos y la carpeta de imágenes, no solo el disco.
  - Registro de quién consulta cada historia clínica, no solo de quién la cambia.
  - Herramientas del RGPD: entregar todos los datos de un paciente, y bloquear o suprimir respetando el plazo legal de conservación de la historia clínica.
  - Retirar o desaconsejar la carpeta compartida, en la que cada puesto lee el fichero entero, contraseñas incluidas.
- **Uso**
  - Modo tableta para el gabinete, con letra grande y contraste ajustable.
  - Servidor en la nube opcional, para grupos de clínicas o para trabajar desde casa, con el mismo protocolo seguro.
- **Calidad**
  - Registro de errores en el ordenador y envío voluntario a soporte.
  - Más pruebas de pantalla.
  - Mediciones con una clínica grande en cada versión.
  - Auditoría de seguridad externa y pruebas del protocolo del servidor con datos al azar.
- **Base de datos:** hoy los datos van en un fichero que se carga entero en memoria, y una clínica de 220 MB funciona bien. Pasar a una base de datos solo si las mediciones con grupos de clínicas lo piden.

---

## 7. Plan de desarrollo

Orden de prioridades:
1. Que los datos y el dinero estén seguros.
2. Que nada impida a una clínica cambiarse.
3. Que funcione en una clínica real.
4. Que cumpla toda la normativa.
5. Lo que lo distingue de los demás.

Cada fase sigue el mismo método: proponer, probar y arreglar, con pruebas automáticas.

### Fase 0: cerrar lo abierto
- Decidir el artículo de la exención de IVA y las copias de prueba (apartado 5).
- Guardar este documento en el repositorio.

**Hecha cuando** no quede ninguna decisión pendiente de esta etapa.

### Fase 1: terminar la revisión de calidad
Revisar las áreas del apartado 6.1 en este orden:
1. Documentos, consentimientos y cuestionarios (tienen valor legal).
2. Recetas.
3. Odontograma, periodontograma e historial.
4. Bancos.
5. Informes.
6. Comunicaciones.
7. Ortodoncia.
8. IA.
9. Cita online y portal.
10. Telefonía y laboratorio.

**Hecha cuando:**
- cada área está revisada, con sus fallos arreglados y una prueba de cada uno;
- ninguna pantalla se sale de una ventana de 1280 píxeles de ancho;
- los informes cuadran con la caja y las facturas.

### Fase 2: cambiarse sin miedo
- GESDEN y Novaclinic completos, y el asistente de Excel para el resto (apartado 6.2).
- Cuadre de totales y simulacro de migración.

**Hecha cuando** en un simulacro con los datos de una clínica real cuadran:
- los pacientes;
- la deuda total;
- las citas futuras;
- los presupuestos abiertos;
- los tratamientos pendientes.

### Fase 3: piloto en una clínica real
- **Instalación:** servidor y puestos, cifrado del disco y segunda copia en un disco USB o en la nube.
- **Servicios externos:** VERI*FACTU en pruebas y después en producción; WhatsApp y correo automático.
- **Antes del piloto:** actualizaciones coordinadas, instaladores firmados y registro de errores.
- **Durante el piloto:** apuntar cada incidencia y medir:
  - cuánto cuesta dar una cita;
  - cuánto cuesta cobrar;
  - cuánto cuesta cerrar la caja.

**Hecha cuando** la clínica cierra un mes completo trabajando solo con VeodarDental: agenda, cobros, caja, facturas enviadas a la AEAT y copias comprobadas, sin incidencias graves.

### Fase 4: toda la normativa
- Receta electrónica certificada, con farmacias reales.
- Herramientas del RGPD y registro de quién consulta cada historia.
- Registro de jornada.
- Exportación contable para la gestoría.
- TicketBAI y Navarra, si hay clínicas allí.

**Hecha cuando** la gestoría y el responsable de protección de datos de la clínica piloto lo dan por bueno.

### Fase 5: lo que lo hace el mejor
- Cuadro de mando con indicadores.
- Pago por internet y señal para reservar.
- Plan de tratamiento por fases y notas por voz con IA.
- Encuestas y reseñas.
- Portal del paciente.
- Idiomas.

**Hecha cuando** el piloto mejora en medidas concretas respecto a cuando usaba el programa anterior:
- menos ausencias;
- más presupuestos aceptados;
- menos tiempo en recepción por paciente.

### Fase 6: crecer
- Grupos de clínicas y servidor en la nube opcional.
- Datáfono y financiación.
- Portales de aseguradoras.
- Modo tableta.

**Hecha cuando** un grupo de varias clínicas trabaja con datos e informes conjuntos.

---

## 8. Riesgos y dependencias externas

| Riesgo | Cómo se reduce |
|---|---|
| No saber cómo guarda GESDEN la historia y los saldos | Conseguir una copia de prueba de su base de datos; leer siempre en solo lectura |
| La certificación de la receta depende de terceros | Todo preparado en el programa; seguir con la receta en papel mientras tanto |
| Cambios en VERI*FACTU o en sus plazos | Confirmar fechas y requisitos con la gestoría; probar en el entorno de pruebas de la AEAT |
| Un fallo en una clínica real con datos de salud | Copias escalonadas y cifradas, restauración comprobada, diario de cambios y piloto controlado antes de vender |
| Servicios externos de pago o con aprobación (Meta, firma de código, proveedores de IA) | Que el programa funcione sin ellos y avise con claridad cuando falten |

---

## 9. Cómo sabremos que es el mejor programa

- **Migración:** una clínica se cambia en un día y todos los totales cuadran.
- **Fiabilidad:** ni descuadres de caja ni números de factura repetidos trabajando varios puestos a la vez. Ninguna pérdida de datos, con copias que se han restaurado de verdad.
- **Rapidez:** dar una cita, cobrar y cerrar la caja cuestan menos pasos que en el programa anterior, también con 20.000 pacientes.
- **Resultados para la clínica:** menos ausencias gracias a los recordatorios, más presupuestos aceptados y más recalls que acaban en cita.
- **Normativa:** facturas enviadas a la AEAT sin rechazos, receta electrónica certificada y protección de datos revisada.
- **Personas:** la recepción y los doctores lo prefieren al programa anterior después del piloto.

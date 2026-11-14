# Decisiones técnicas

## D-01 Rama de trabajo
Todo el trabajo del encargo va en la rama local `autonomia/fases-0-6`, creada desde `ade418b`. No se hace push, fusión ni versión pública. `main` no se toca.

## D-02 Aislamiento de rutas
Todas las rutas pasan por `src/paths.rs`:
- en las pruebas, cada proceso usa su propia carpeta temporal;
- con `VEODARDENTAL_RAIZ`, todo va debajo de esa carpeta;
- `tools/aislado.sh` redirige además HOME y XDG.

Las capturas, mediciones y ensayos se lanzan siempre con `tools/aislado.sh`.

## D-03 Formato
El encargo cita `cargo fmt --all -- --check` como control. El repositorio no tenía configuración y el código no estaba formateado. Se adopta `rustfmt.toml` con `max_width = 250` y `use_small_heuristics = "Max"`, que es lo más parecido al estilo existente: de las configuraciones medidas, la que menos cambia.

Se aplica una sola vez en su propio commit (`aca725a`), sin cambios de comportamiento: pasan las pruebas y clippy estricto.

## D-04 Encargo en el repositorio
La fase 0 pide conservar el encargo en el repositorio. `VEODARDENTAL_GOAL.md` se guarda tal como lo dejó el propietario, sin modificarlo, en un commit propio que cita su SHA-256. No se incluye ningún otro cambio ajeno.

## D-05 Verificador del objetivo
`cargo run --example verificar_objetivo` es un ejemplo de Cargo, así que no entra en el binario ni en los instaladores. Usa `serde_json` y `sha2`, que ya son dependencias.
- **Cobertura:** cada línea con requisitos de los apartados 2 a 9 del encargo debe empezar por el ancla de un requisito de `TAREAS.json`.
- **Tareas hechas:** deben tener evidencias registradas en `EVIDENCIAS.md`.
- **Tareas bloqueadas:** solo pueden ser externas, con los siete campos rellenos en `BLOQUEOS.md` y su preparación de ingeniería HECHA.
- **Sin `--progreso`:** falla mientras quede ingeniería pendiente.

## D-06 Importes
El modelo existente guarda los importes en `f64` y redondea al céntimo. Cambiar el formato de todos los datos sería una reescritura general con migración, y el encargo pide evitarla sin mediciones que lo justifiquen. Por eso:
- el código nuevo hace sus cálculos en céntimos enteros (`i64`) con redondeo explícito, mediante el tipo `Cents` de T-R7-1;
- las sumas existentes se auditan con pruebas de que cuadran al céntimo.

## D-07 Antigüedad de la lista oficial de medicamentos
Se avisa cuando la lista de la AEMPS cargada tiene más de 60 días (`nomenclator::STALE_DAYS`) o no tiene fecha legible. Es un umbral de mantenimiento, no una regla clínica, y es configurable en el código.

## D-08 Quién puede recetar
`Db::prescriber_problem` reconoce a quien no puede recetar por la especialidad del colaborador: higienista, auxiliar, protésico, recepción o administración. Además exige el nº de colegiado y que el colaborador esté de alta.
No se añade un campo nuevo: la especialidad ya existe y los datos guardados siguen valiendo sin migración.

## D-09 Banco en el servidor
Importar un extracto, conciliar a mano, deshacer y crear el gasto de un cargo pasan a ser operaciones `ChainOp`, como ya lo era la conciliación automática. En red las hace el servidor y en la carpeta compartida se hacen con el fichero bloqueado, siempre con los datos al día. Así, dos puestos no duplican un extracto ni concilian dos veces lo mismo. Usan el permiso «Gastos», el mismo que la conciliación automática.

## D-10 Deshacer una remesa cobrada por la conciliación
Si fue la conciliación la que marcó la remesa como cobrada, deshacerla la devuelve a «generada»:
- sus cobros se anulan con otro en negativo (`cancels`), sin borrarlos;
- las cuotas vuelven a pendientes.

Si ya estaba cobrada antes de conciliar, no se toca. Se prefiere esto a dejarla cobrada sin un abono del banco que lo respalde.

## D-11 Importes con un solo separador
`parse_money` toma como decimal el separador que va último cuando hay dos («1.234,56», «1,234.56»), y como miles el que se repite. Con uno solo que no se repite, sigue siendo el decimal, como antes («12,5», «94.25»).

«1.500» queda ambiguo y se lee como 1,5. Los bancos exportan con decimales, así que no se adivina por columna.

## D-12 Producción y comisión con mutuas
La producción que muestran los informes es lo que paga el paciente más lo que paga la mutua (el baremo de la tarifa).

La comisión de los colaboradores sigue siendo sobre lo que paga el paciente, que es como se calculaba: cambiarla sin más cambiaría lo que se paga a cada uno. Se añade a cada colaborador la opción «también sobre lo que paga la mutua», desmarcada por defecto, y la base se dice en la pantalla y en la liquidación impresa. Elegirla es cosa de la clínica.

## D-13 Fechas y zona horaria
Las fechas siguen siendo la hora local, sin zona, del ordenador que registra. Cambiarlas a UTC exigiría migrar todos los datos y no aporta nada mientras servidor y puestos estén en la misma zona.

Para que no pase sin notarse, el servidor dice su diferencia con UTC y el puesto avisa si es otra. El servidor en la nube (fase 6) debe configurarse con la zona horaria de la clínica.

## D-14 Envíos inciertos y reserva de correos
Un correo o un WhatsApp que salió y se cortó sin respuesta puede haber llegado. Reintentarlo solo podría mandarlo dos veces, y ninguno de los dos proveedores garantiza que un reenvío no se duplique. Por eso queda como «¿Llegó?» y lo decide recepción.

Para que dos puestos no envíen el mismo correo, el puesto los reserva antes con una marca aleatoria (`ChainOp::ClaimEmails`), en el servidor o con el fichero bloqueado. Una reserva sin resultado durante 30 minutos pasa a «¿Llegó?». No hace falta permiso especial: cualquier usuario puede generar y enviar los mensajes del día, como antes.

## D-15 Datos de salud y bajas en los mensajes
- **Datos de salud:** los comodines de alertas, tratamientos, pendientes y fecha de nacimiento valen en los documentos, pero se quitan de cualquier SMS, WhatsApp o correo (minimización).
- **Bajas:**
  - «BAJA» por WhatsApp quita ese canal al número que lo envía, no los demás.
  - La baja por finalidad se marca en la ficha.
  - Solo felicitaciones y seguimientos de presupuesto llevan el texto de baja: los recordatorios de cita y de revisión son de la atención del paciente.
- **Revisión pendiente:** la revisión jurídica de qué mensajes son comunicaciones comerciales (LSSI) queda para el responsable de protección de datos, con el resto de la fase 4.

## D-16 Revisión de los análisis con IA
Quien da por bueno un análisis con IA es un profesional colegiado, con el mismo criterio que para recetar (`collaborator_problem`). El servidor solo acepta su revisión, con su nombre.

El límite de análisis de pago al mes es de 20 por defecto: un uso normal de una clínica pequeña, con un gasto acotado. Se cambia en la configuración y es cosa de la clínica.

## D-17 Caducidad de los enlaces de documentos y presupuestos
Abren 30 días desde la última vez que la clínica los envía (copiar, enviar por correo o abrir el enlace cuenta como envío), o desde que se firman o se deciden. Un presupuesto pendiente abre además mientras está en plazo.

Reenviar un enlace sin firmar ni decidir lo renueva con el mismo código, así que el paciente no recibe varios enlaces distintos. Es un equilibrio entre dar tiempo al paciente y no dejar datos personales abiertos indefinidamente; el plazo se puede cambiar en `portal::LINK_DAYS`.

## D-18 Lo importado de otro programa no se factura ni se liquida otra vez
Los tratamientos hechos que llegan en una migración se facturaron (o no) y se liquidaron a la mutua en el programa anterior. VeodarDental no puede saber cuáles. Por eso quedan cerrados allí (`MedEntry::settled_elsewhere`):
- cuentan en el historial y en el saldo;
- no salen para facturar ni para liquidar.

Así no puede haber una factura ni una liquidación doble. Si un paciente pide la factura de un acto antiguo, se hace en el programa anterior, que se conserva en lectura. Los tratamientos pendientes, en cambio, se harán aquí y se facturan como cualquier otro.

## D-19 Dos personas no se unen por el NIF ni por el número de historia
Un paciente del origen se une a uno que ya estaba solo si la clínica lo decide (para todos o fila a fila) y además tienen el mismo nombre, con el mismo NIF o la misma fecha de nacimiento. Con solo el NIF igual, como un hijo con el de su madre o un error al teclear, se puede omitir o crear aparte, nunca unir. Un número de historia ocupado se cambia por otro libre, con aviso. Lo que no se decide no se importa y sale en el informe (por defecto, «No importarlo»).

## D-20 El ajuste de saldo es un apunte, no un cobro
Si la clínica pide que la deuda sea la del programa anterior, se apunta una vez por paciente un ajuste con la diferencia (código `MIGSALDO`, en positivo o en negativo). No es un cobro, porque no entra dinero, y no es un acto, porque no cuenta como producción ni comisión ni se factura (`MedEntry::is_act`).

No se ajusta a los pacientes unidos a uno que ya estaba: su saldo de aquí incluye lo que ya tenía. Se avisa de la diferencia.

Con hoja de saldos, quien no aparece en ella debe 0 en el programa anterior. Si la exportación solo trae a los deudores, el informe lo muestra antes de importar: los que tienen saldo a favor salen como pacientes que no cuadran.

## D-21 Rutas de red de los documentos importados
Un documento del programa anterior con ruta de otro ordenador (`\\servidor\…` y sus variantes) solo se importa si está dentro de la carpeta de documentos que la clínica indica como suya. Abrir esa ruta, o solo mirarla, hace que Windows conecte con ese servidor y le dé las credenciales del usuario. Una exportación manipulada podría así llevar las credenciales de la clínica a un servidor ajeno.

Las rutas locales se importan siempre, porque no salen del ordenador.

## D-22 Actualizaciones: primero el servidor, versión de datos igual y vuelta atrás segura
**Compatibilidad.** Se mide por la versión de los datos (`DATA_VERSION`), no por el número de versión del programa, porque es lo que decide si un puesto borraría campos al escribir.
- Un puesto con otra versión de los datos puede leer, pero no escribe.
- Lo suyo queda en la bandeja de su ordenador hasta que se actualiza.
- **Orden:** primero el servidor y después los puestos.

**Los datos no se tocan durante el cambio de programa.** Se completan cuando la versión nueva arranca de verdad, con su copia previa. Así, volver atrás tras un corte nunca necesita restaurar datos. Volver atrás a mano días después solo lo necesita si la versión nueva cambió la de los datos, y el programa lo dice.

**Firma de las versiones.** Va con Ed25519, con la clave pública en el programa y la privada fuera del repositorio. Hasta que exista (B-27), las versiones se instalan con su instalador.

## D-23 Versión 2 de los datos
**Contexto.** `DATA_VERSION` seguía en 1 desde el estado inicial, aunque este trabajo ha añadido campos. Desde T-F5-03 hay campos nuevos dentro de registros que ya existían (el plan de tratamiento en cada presupuesto, la fase de cada línea y la clase de tratamiento de los consentimientos). Un puesto con el programa anterior que editara uno de esos registros los borraría al guardarlo.

**Decisión.** La versión de los datos pasa a 2, con la regla de D-22:
- un puesto con otra versión lee, pero no escribe;
- primero se actualiza el servidor;
- al abrir datos de la versión 1 se hace antes una copia.

**Al pasar a la versión 2** (`Db::upgrade`): los consentimientos de siempre (extracción, implantes y endodoncia) reciben la clase de tratamiento que los pide. Las plantillas que la clínica haya cambiado no se tocan.

**Ampliación (T-F5-12).** La versión 2 aún no se ha publicado. Incluye también lo de las campañas:
- el consentimiento para promociones en la ficha (`Patient::marketing`);
- la clase de mensaje `Campana`;
- la lista `campaigns`.

Al pasar de la versión 1 a la 2 nadie tiene consentimiento para promociones: hay que darlo expresamente. Un programa de la versión 1 que abra estos datos no los entiende y los abre sin guardar, como cuando no se pueden leer (no escribe encima).

## D-24 Campañas: consentimiento propio, lista fija y atribución explícita
**Contexto.** La fase 5 pide campañas a grupos de pacientes con:
- consentimiento por finalidad;
- bajas, horario y límites;
- previsualización;
- resultados atribuibles con criterios explícitos.

Hasta ahora solo había consentimiento de protección de datos y de cada canal. Eso no basta para promociones.

**Decisión (`src/campanas.rs`):**
- **Consentimiento propio.** Una promoción solo le llega a quien la ha aceptado expresamente, con fecha (Filiación). Además hace falta el consentimiento de protección de datos y el del canal. La baja de las campañas es una clase de aviso más (`MsgKind::Campana`).
  - Las dos cosas se vuelven a mirar al pasar cada mensaje a la cola: una baja de última hora manda.
  - Al fusionar dos fichas, una baja dada en cualquiera de ellas sigue valiendo.
- **Lista fija al programar.** Lo que se previsualiza es lo que se programa. Quien da su consentimiento después no entra en esa campaña.
- **Sin envío directo.** La campaña solo pasa mensajes a la cola de Comunicaciones:
  - de lunes a viernes y en su horario;
  - con su límite diario;
  - sin otra campaña al mismo paciente en los días anteriores que diga.

  Desde la cola se envían como los demás. Cada mensaje lleva cómo darse de baja, en el idioma del paciente.
- **Quién.** La programa y la cancela Dirección. Los demás perfiles hacen borradores y pasan a la cola lo que toca. El servidor lo comprueba (`campanas::tick_only`).
  - Elegir por tratamiento usa datos de salud: solo lo ajusta un perfil clínico, y el texto del mensaje no puede llevar comodines de salud.
  - Si se puede elegir por tratamiento para promociones, y con qué texto de consentimiento, lo decide el DPO (`docs/revision_externa/dpo.md`).
- **Atribución.** Se cuenta una cita si cumple todo esto:
  - es del paciente;
  - no existía cuando se le pasó el mensaje a la cola (se guarda la lista de sus citas en ese momento);
  - no está anulada;
  - su fecha cae en el plazo de la campaña.

  Se dice también si acudió. La pantalla avisa de que es una asociación, no una prueba de que la cita se deba a la campaña.

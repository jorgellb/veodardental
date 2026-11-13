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

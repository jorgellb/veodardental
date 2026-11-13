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

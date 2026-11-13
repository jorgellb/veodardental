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

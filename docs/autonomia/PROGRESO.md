# Progreso

Punto de reanudación tras una compactación:
1. Leer `VEODARDENTAL_GOAL.md`, este fichero, `TAREAS.json` y `DECISIONES.md`.
2. Ver el estado con `cargo run --example verificar_objetivo -- --progreso`.
3. Seguir por la «Siguiente acción».

Trabajar siempre con `tools/aislado.sh` y en la rama `autonomia/fases-0-6`.

## Hecho y probado
- T-INI-01, T-INI-03: estado inicial y batería inicial (E-001).
- T-INI-02: aislamiento de rutas (E-002, commit `3e48461`).
- Formato con rustfmt (E-003, commit `aca725a`).

## En curso
- T-INI-04: documentos de seguimiento e inventario. Faltan el commit inicial y el verificador (T-INI-05).

## Siguiente acción
1. Hacer commit del seguimiento, el verificador, el encargo (D-04) y PLAN_DE_DESARROLLO.md.
2. Fase 0:
   - T-F0-02 a T-F0-04: referencia de exención de IVA configurable;
   - T-F0-07: requisitos externos, ya descritos en BLOQUEOS.md.
3. Fase 1, área 1: documentos, consentimientos y cuestionarios.

## Cómo añadir tareas
Se editan en `TAREAS.json`, sin regenerarlo. Los hallazgos de cada revisión se añaden como tareas `T-F1-Ak-FIX-n` enlazadas a su requisito.

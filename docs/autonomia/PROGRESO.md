# Progreso

Punto de reanudación tras una compactación:
1. Leer `VEODARDENTAL_GOAL.md`, este fichero, `TAREAS.json` y `DECISIONES.md`.
2. Ver el estado con `cargo run --example verificar_objetivo -- --progreso` (con `tools/aislado.sh`).
3. Seguir por la «Siguiente acción».

Trabajar siempre con `tools/aislado.sh` y en la rama `autonomia/fases-0-6`. Cada área revisada deja `docs/autonomia/revision/areaN.md`, evidencias `E-0NN` y un commit.

## Hecho y probado
| Tarea | Evidencia | Commit |
|---|---|---|
| T-INI-01, T-INI-03: estado y batería inicial | E-001 | — |
| T-INI-02: aislamiento de rutas | E-002 | `3e48461` |
| Formato | E-003 | `aca725a` |
| T-INI-04, T-INI-05, T-F0-01, T-F0-07: seguimiento, verificador y encargo | E-004 | `50431a3`, `6f8d19c` |
| T-F0-02 a T-F0-04: exención de IVA configurable para las facturas nuevas | E-005 | `441d546` |
| T-F1-A1: documentos, consentimientos y cuestionarios | E-006 | `b2cf77b` |
| T-F1-A2: recetas y vademécum | E-007 | `00838cc` |

## Siguiente acción
1. T-F1-A3: odontograma, periodontograma e historial.
   - Comprobar dentición adulta y temporal, piezas y superficies, y evolución por visita.
   - Comprobar la coherencia entre dibujo, presupuesto y tratamiento hecho.
   - Comprobar deshacer y concurrencia sin romper cobros, facturas ni stock.
2. Después, áreas 4 a 10, T-F1-11 (todas las pantallas a 1280×720 y 1280×800) y T-F1-12 (concurrencia y fuzzing).

## Notas para seguir
- `src/firmado.rs` protege lo firmado y lo emitido en el servidor (`protect_json`, para los campos «documents», «health» y «prescriptions») y en la carpeta compartida (`Db::keep_signed`). Si aparecen más registros inmutables, hay que añadirlos ahí.
- Capturas: `tools/aislado.sh ./target/debug/veodardental --screenshot $PWD/docs/autonomia/capturas/X.png --user dir --screen pacientes --patient 101 --tab <pestaña> --size 1280x800 [--bajar N]`.

## Cómo añadir tareas
Se editan en `TAREAS.json`, sin regenerarlo. Los hallazgos de cada revisión se documentan en su `revision/areaN.md` y se cierran con la tarea `T-F1-Ak-FIX`.

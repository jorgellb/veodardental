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
| Revisión de seguridad de las áreas 1 y 2 | E-008 | `7c55ab2` |
| T-F1-A3: odontograma, periodontograma e historial | E-009 | `dee610b` |
| T-F1-A4: bancos y conciliación | E-010 | `d6f2e0a` |
| T-F1-A5: informes (y tipo `Cents` para T-R7-1) | E-011 | `7fd53f0` |
| T-F1-A6: comunicaciones | E-012 | `cdddd05` |
| T-F1-A7: ortodoncia | E-013 | `d0c52f4` |
| T-F1-A8: IA de radiografías | E-014 | `6e2a560` |
| Revisión de seguridad del área 8 | E-015 | `802798a` |
| T-F1-A9: cita online y portales | E-016 | `80670ec` |
| T-F1-A10: telefonía y laboratorio digital | E-017 | `4c09bf7` |
| T-F1-11: todas las pantallas a 1280×720 y 1280×800 | E-018 | (este lote) |

## Siguiente acción
1. T-F1-12a a T-F1-12d.
   - Concurrencia por flujo: cada uno con su prueba de dos puestos. Ya están facturas, cuotas, devoluciones, remesas, stock, banco, correos, ortodoncia y laboratorio; falta inventariarlos y completar recibos, recordatorios y reservas.
   - Desconexión, reenvío, reinicio y fallo de escritura.
   - Protocolo del servidor con entradas inválidas.
   - Fuzzing con semilla de la combinación de cambios, el protocolo, CSV y Norma 43.
2. Después, el cierre de la fase 1 (T-F1-13) y la fase 2 (migraciones).
3. T-R7-1 sigue pendiente. El tipo `Cents` ya existe (src/cents.rs) y lo usan los informes; faltan las pruebas de propiedades de facturas, presupuestos, cuotas, caja y remesas.

## Notas para seguir
- La anchura a 1280 px la vigila `ui_tests::every_screen_fits_1280_wide`. Una pantalla nueva o cambiada debe pasarla. Para los campos, `field`; para las tablas, `Table`; en filas de campos, `horizontal_wrapped`.
- Los totales económicos se calculan en `src/informes.rs` (`Filter` y `report_*`). Las pantallas no deben sumar por su cuenta.
- Las operaciones que no deben hacerse dos veces van como `ChainOp` en src/app.rs, con `run`, `json`, `from_json` y `needs`; el servidor las ejecuta con los datos al día. El banco ya va así (D-09).
- Hay pruebas que dependen de la hora: a última hora del día quedan pocos huecos libres hoy. Las que piden «la segunda hora del primer día» deben buscar el primer día con dos horas.
- Si la compilación falla al enlazar con «undefined hidden symbol» (caché incremental dañada por un corte), basta con `tools/aislado.sh cargo clean -p veodardental`.
- `src/firmado.rs` protege lo firmado y lo emitido en el servidor (`protect_json`, para los campos «documents», «health» y «prescriptions») y en la carpeta compartida (`Db::keep_signed`). Si aparecen más registros inmutables, hay que añadirlos ahí.
- Capturas: `tools/aislado.sh ./target/debug/veodardental --screenshot $PWD/docs/autonomia/capturas/X.png --user dir --screen pacientes --patient 101 --tab <pestaña> --size 1280x800 [--bajar N]`.

## Cómo añadir tareas
Se editan en `TAREAS.json`, sin regenerarlo. Los hallazgos de cada revisión se documentan en su `revision/areaN.md` y se cierran con la tarea `T-F1-Ak-FIX`.

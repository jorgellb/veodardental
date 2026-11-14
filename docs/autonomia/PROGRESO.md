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
| T-F1-11: todas las pantallas a 1280×720 y 1280×800 | E-018 | `84a667a` |
| T-F1-12a a T-F1-12d: concurrencia, fallos, protocolo y entradas al azar | E-019 | `bcfa695` |
| T-F1-13: informes con oráculo independiente (hecho en el área 5) | E-011 | `7fd53f0` |
| T-R7-2, T-R7-3 y T-R7-4: autoridad del servidor, anulaciones y escritura | E-020 | `3d777b0` |
| T-F2-01 a T-F2-09: motor de migración, asistente, conciliación, GESDEN/Novaclinic, perfiles y documentación | E-021 | `e4e3298` |
| Revisión de seguridad de la fase 2, ficheros solo para el usuario y T-F3-01: ensayo de instalación | E-022 | `e44ee0a` |
| T-F3-02: actualizaciones coordinadas (versión de datos, bandeja de pendientes, paquetes firmados y vuelta atrás) | E-023 | `7704bd1` |
| T-F3-03: firma condicionada a secretos, empaquetado comprobado y bandeja por usuario (revisión de seguridad) | E-024 | `c08cd25` |
| T-F3-04: registro de errores e informe para soporte; firma solo con etiqueta y sesión caducada (revisión de seguridad) | E-025 | `2945231` |
| T-F3-05: ensayo de restauración y clave de recuperación; nombres fuera del registro (revisión de seguridad) | E-026 | `9bdf3b7` |
| T-F3-06: VERI*FACTU contra una AEAT simulada (rechazo, caída, reintento idéntico y conciliación) | E-027 | `8a5bb6a` |
| T-F3-07: SMS por pasarela con idempotencia y resultados inciertos (WhatsApp y correo, ya en E-012) | E-028 | `3f51833` |
| T-F3-09: instrumentos del piloto (tiempos y pasos, incidencias, disponibilidad y restauraciones) | E-029 | `445396c` |
| T-F3-08: guías de instalación, restauración y uso comprobadas contra el programa; fórmulas en los CSV (revisión de seguridad) | E-030 | `4380f92` |
| T-F4-00: normativa consultada (fuente, fecha, versión y efecto) | E-031 | `03a9036` |
| T-F4-02: registro de accesos a cada historia (encadenado, fuera de los datos, en red por el servidor) | E-032 | `e1d4ce0` |
| T-F4-03: derechos RGPD (solicitudes, exportación íntegra, limitación, bloqueo; supresión desactivada); revisión de seguridad del registro de accesos | E-033 | `1141e02` |
| T-F4-05: retirada de la carpeta compartida con paso al servidor; bloqueo en el servidor y exportación sin ficheros ajenos (revisión de seguridad) | E-034 | `952fa58` |
| T-F4-06: registro de jornada (fichar, correcciones trazables, acceso por rol, exportación; en red lo hace cumplir el servidor) | E-035 | `e7d3b06` |
| Revisión de seguridad de la jornada en red | E-036 | `41c80cb` |
| T-F4-07: libros registro en el formato común de la AEAT | E-037 | `9309580` |
| T-F4-01: receta privada, organismos confirmados en el expediente | E-038 | `0904e4a` |
| T-F4-04: cifrado de los datos, diario y copias con claves gestionadas, rotación y recuperación | E-039 | `2c862c8` |
| Revisión de seguridad del cifrado de los datos (sin vuelta atrás a sin cifrar) | E-040 | `0f80d57` |
| T-F4-08: facturación foral (territorio confirmado, TicketBAI preparado, sin homologar ni activar) | E-041 | `51c4d65` |
| T-F4-09: pruebas para la gestoría, el DPO y la auditoría de seguridad (docs/revision_externa) | E-042 | `fe0af03` |
| T-F5-01: cuadro de mando con fórmulas definidas y verificación independiente | E-043 | `ccc20bb` |
| T-F5-02: pago por internet (Redsys por redirección y Bizum), sin cargos | E-044 | `1dff1c8` |
| T-F5-03: plan de tratamiento por fases y alternativas (versión de datos 2) | E-045 | `57db409` |
| T-F5-04: notas de evolución con plantillas, dictado, transcripción y resumen revisado | E-046 | `d1d2d85` |
| Revisión de seguridad: confirmación sellada por el servidor y fusión que guarda notas, pagos y derechos | E-047 | `842eef9` |
| T-F5-05: periodontograma por voz con corrección y confirmación visibles | E-048 | `0d1b386` |
| T-F5-06: visor DICOM/CBCT y STL, y comparación de radiografías | E-049 | `6637bc5` |
| Revisión de seguridad: memoria de los DICOM y tamaño de los STL | E-050 | `107cb20` |
| T-F5-07: alertas clínicas ampliadas con fuente citada y revisión del profesional | E-051 | `a65de92` |
| Revisión de seguridad: vistos de alertas sellados por el servidor | E-052 | `c5e63d5` |
| T-F5-08: esterilización (ciclos, lotes, pacientes, incidencias y afectados) | E-053 | `c5e63d5` |
| Revisión de seguridad: esterilización comprobada por el servidor | E-054 | `7e9b2cb` |
| Revisión de seguridad: etiquetas de lotes únicas con una sola regla | E-055 | `b8211c0` |
| T-F5-09: encuestas de satisfacción y solicitud de reseñas | E-056 | `20c5438` |
| T-F5-10: Mi espacio, portal del paciente con autorización por objeto y por responsable | E-057 | `077c751` |
| Revisión de seguridad: Mi espacio (responsable y entrada sin DNI) | E-058 | `0d4df76` |
| T-F5-11: idiomas (castellano, catalán, euskera, gallego e inglés) | E-059 | `750f1df` |
| Revisión de seguridad: la palabra de baja de cada idioma | E-060 | `67ccec1` |
| T-F5-12: campañas a grupos de pacientes con consentimiento, límites, previsualización y atribución | E-061 | `bfe56e4` |
| Revisión de seguridad: el servidor pasa a la cola los mensajes de las campañas | E-062 | `e51bbdd` |
| T-F6-01: grupos de clínicas (sociedades, centros con permisos y vista del grupo) | E-063 | `6d277fb` |
| T-F6-02: servidor en la nube opcional, desplegado y ensayado en local | E-064 | `1f554e5` |
| Revisión de seguridad: carpeta del servicio y clave fuera de la orden | E-065 | `9bd690a` |
| T-F6-03: datáfono y financiación (adaptadores, estados, conciliación, cancelación y recuperación) | E-066 | `ae1cca9` |
| Revisión de seguridad: sacar dinero y el banco con su permiso en el servidor | E-067 | `c4e49e5` |

## Siguiente acción
Las fases 1 a 5 están terminadas en la parte de ingeniería. Lo externo sigue en BLOQUEOS:
- piloto;
- AEAT con certificado;
- Meta;
- firma de código y clave de versiones;
- esquemas de los programas de origen;
- homologación foral;
- revisiones externas (B-13, con el paquete de `docs/revision_externa`, que ya incluye el DPO para campañas);
- contrato de TPV, transcripción, revisión clínica de las alertas y traducciones (T-F5-02X, T-F5-04X, T-F5-07X y T-F5-11X).

Toca la fase 6.
1. T-F6-01 a T-F6-09, en orden (ver `TAREAS.json` y la fase 6 de `VEODARDENTAL_GOAL.md`).
   - Grupos de clínicas, servidor en la nube probado en local, datáfono y financiación, aseguradoras, pedidos por stock mínimo, modo tableta, calidad continua, almacenamiento y escenario de varias clínicas.
   - Lo que necesita contrato o proveedor queda en sus tareas X bloqueadas.
2. Después, las transversales pendientes:
   - T-R7-1 (propiedades de `Cents`) y T-R7-5 a T-R7-7;
   - T-PROC-01/02;
   - T-INI-06 a T-INI-08.
3. Por último, el cierre (T-CIERRE-01/02).

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

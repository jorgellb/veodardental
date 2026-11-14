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
| T-INI-02: aislamiento de rutas | E-002 | `95efcd6` |
| Formato | E-003 | `a39ef41` |
| T-INI-04, T-INI-05, T-F0-01, T-F0-07: seguimiento, verificador y encargo | E-004 | `79fe438`, `ebae3bd` |
| T-F0-02 a T-F0-04: exención de IVA configurable para las facturas nuevas | E-005 | `1b18443` |
| T-F1-A1: documentos, consentimientos y cuestionarios | E-006 | `95abeab` |
| T-F1-A2: recetas y vademécum | E-007 | `aad4f99` |
| Revisión de seguridad de las áreas 1 y 2 | E-008 | `1c69232` |
| T-F1-A3: odontograma, periodontograma e historial | E-009 | `45b9ce3` |
| T-F1-A4: bancos y conciliación | E-010 | `4a06c78` |
| T-F1-A5: informes (y tipo `Cents` para T-R7-1) | E-011 | `b6f8756` |
| T-F1-A6: comunicaciones | E-012 | `904d14a` |
| T-F1-A7: ortodoncia | E-013 | `4b786ed` |
| T-F1-A8: IA de radiografías | E-014 | `072c493` |
| Revisión de seguridad del área 8 | E-015 | `1bae1c7` |
| T-F1-A9: cita online y portales | E-016 | `72305d4` |
| T-F1-A10: telefonía y laboratorio digital | E-017 | `7948155` |
| T-F1-11: todas las pantallas a 1280×720 y 1280×800 | E-018 | `c6fc6c7` |
| T-F1-12a a T-F1-12d: concurrencia, fallos, protocolo y entradas al azar | E-019 | `6abeca9` |
| T-F1-13: informes con oráculo independiente (hecho en el área 5) | E-011 | `b6f8756` |
| T-R7-2, T-R7-3 y T-R7-4: autoridad del servidor, anulaciones y escritura | E-020 | `e24613b` |
| T-F2-01 a T-F2-09: motor de migración, asistente, conciliación, GESDEN/Novaclinic, perfiles y documentación | E-021 | `2aede99` |
| Revisión de seguridad de la fase 2, ficheros solo para el usuario y T-F3-01: ensayo de instalación | E-022 | `1bab1e0` |
| T-F3-02: actualizaciones coordinadas (versión de datos, bandeja de pendientes, paquetes firmados y vuelta atrás) | E-023 | `49db3a4` |
| T-F3-03: firma condicionada a secretos, empaquetado comprobado y bandeja por usuario (revisión de seguridad) | E-024 | `7a6af9f` |
| T-F3-04: registro de errores e informe para soporte; firma solo con etiqueta y sesión caducada (revisión de seguridad) | E-025 | `a999448` |
| T-F3-05: ensayo de restauración y clave de recuperación; nombres fuera del registro (revisión de seguridad) | E-026 | `f55b421` |
| T-F3-06: VERI*FACTU contra una AEAT simulada (rechazo, caída, reintento idéntico y conciliación) | E-027 | `6c57600` |
| T-F3-07: SMS por pasarela con idempotencia y resultados inciertos (WhatsApp y correo, ya en E-012) | E-028 | `2f00e8a` |
| T-F3-09: instrumentos del piloto (tiempos y pasos, incidencias, disponibilidad y restauraciones) | E-029 | `c836bab` |
| T-F3-08: guías de instalación, restauración y uso comprobadas contra el programa; fórmulas en los CSV (revisión de seguridad) | E-030 | `0bee991` |
| T-F4-00: normativa consultada (fuente, fecha, versión y efecto) | E-031 | `5127540` |
| T-F4-02: registro de accesos a cada historia (encadenado, fuera de los datos, en red por el servidor) | E-032 | `8b7bd05` |
| T-F4-03: derechos RGPD (solicitudes, exportación íntegra, limitación, bloqueo; supresión desactivada); revisión de seguridad del registro de accesos | E-033 | `135c680` |
| T-F4-05: retirada de la carpeta compartida con paso al servidor; bloqueo en el servidor y exportación sin ficheros ajenos (revisión de seguridad) | E-034 | `b0ca0e0` |
| T-F4-06: registro de jornada (fichar, correcciones trazables, acceso por rol, exportación; en red lo hace cumplir el servidor) | E-035 | `0658182` |
| Revisión de seguridad de la jornada en red | E-036 | `538bc37` |
| T-F4-07: libros registro en el formato común de la AEAT | E-037 | `e9bfc24` |
| T-F4-01: receta privada, organismos confirmados en el expediente | E-038 | `6bfb0a9` |
| T-F4-04: cifrado de los datos, diario y copias con claves gestionadas, rotación y recuperación | E-039 | `bbc30f6` |
| Revisión de seguridad del cifrado de los datos (sin vuelta atrás a sin cifrar) | E-040 | `a122365` |
| T-F4-08: facturación foral (territorio confirmado, TicketBAI preparado, sin homologar ni activar) | E-041 | `183a193` |
| T-F4-09: pruebas para la gestoría, el DPO y la auditoría de seguridad (docs/revision_externa) | E-042 | `12c3294` |
| T-F5-01: cuadro de mando con fórmulas definidas y verificación independiente | E-043 | `a4d4c8c` |
| T-F5-02: pago por internet (Redsys por redirección y Bizum), sin cargos | E-044 | `203e03f` |
| T-F5-03: plan de tratamiento por fases y alternativas (versión de datos 2) | E-045 | `0b1a3ec` |
| T-F5-04: notas de evolución con plantillas, dictado, transcripción y resumen revisado | E-046 | `b42eafd` |
| Revisión de seguridad: confirmación sellada por el servidor y fusión que guarda notas, pagos y derechos | E-047 | `aba6882` |
| T-F5-05: periodontograma por voz con corrección y confirmación visibles | E-048 | `b73b7cc` |
| T-F5-06: visor DICOM/CBCT y STL, y comparación de radiografías | E-049 | `66b6836` |
| Revisión de seguridad: memoria de los DICOM y tamaño de los STL | E-050 | `bb8725a` |
| T-F5-07: alertas clínicas ampliadas con fuente citada y revisión del profesional | E-051 | `4eacbf5` |
| Revisión de seguridad: vistos de alertas sellados por el servidor | E-052 | `67c4cb2` |
| T-F5-08: esterilización (ciclos, lotes, pacientes, incidencias y afectados) | E-053 | `67c4cb2` |
| Revisión de seguridad: esterilización comprobada por el servidor | E-054 | `3cd3868` |
| Revisión de seguridad: etiquetas de lotes únicas con una sola regla | E-055 | `4851dde` |
| T-F5-09: encuestas de satisfacción y solicitud de reseñas | E-056 | `4f8bf96` |
| T-F5-10: Mi espacio, portal del paciente con autorización por objeto y por responsable | E-057 | `03759a4` |
| Revisión de seguridad: Mi espacio (responsable y entrada sin DNI) | E-058 | `44d1400` |
| T-F5-11: idiomas (castellano, catalán, euskera, gallego e inglés) | E-059 | `6fbe00a` |
| Revisión de seguridad: la palabra de baja de cada idioma | E-060 | `fdbde1f` |
| T-F5-12: campañas a grupos de pacientes con consentimiento, límites, previsualización y atribución | E-061 | `9e34dd3` |
| Revisión de seguridad: el servidor pasa a la cola los mensajes de las campañas | E-062 | `ca33a33` |
| T-F6-01: grupos de clínicas (sociedades, centros con permisos y vista del grupo) | E-063 | `d5bdbfb` |
| T-F6-02: servidor en la nube opcional, desplegado y ensayado en local | E-064 | `65ce1c0` |
| Revisión de seguridad: carpeta del servicio y clave fuera de la orden | E-065 | `3d577d0` |
| T-F6-03: datáfono y financiación (adaptadores, estados, conciliación, cancelación y recuperación) | E-066 | `c8c2b02` |
| Revisión de seguridad: sacar dinero y el banco con su permiso en el servidor | E-067 | `af30c98` |
| Revisión de seguridad: financiación, caja y devoluciones con datáfono por su camino | E-068 | `e6d1976` |
| T-F6-04: aseguradoras (autorizaciones y liquidaciones, flujo interno y contrato) | E-069 | `b377605` |
| Revisión de seguridad: autorizaciones y liquidaciones solo por el servidor | E-070 | `1ae47d6` |
| T-F6-05: pedidos por stock mínimo con aprobación, unidades de compra, lotes y recepción | E-071 | `4670ed6` |
| Revisión de seguridad: configuración, pedidos partidos y correo del proveedor | E-072 | `e254f7a` |
| Revisión de seguridad: configuración por lista de lo permitido y sin proveedores repetidos | E-073 | `92d37f9` |
| T-F6-06: modo tableta (controles grandes, contraste ajustable y uso sin ratón) | E-074 | `e0f0621` |
| T-F6-07, T-INI-08 y T-F6-08: calidad continua y mediciones reproducibles por versión (D-28) | E-075 | `0dd22f6` |
| T-F6-09: escenario sintético de un grupo de clínicas con permisos y totales | E-076 | `0dd22f6` |
| T-INI-06: funciones que el informe da por existentes, comprobadas en el código | E-077 | `0dd22f6` |
| T-R7-1: dinero al céntimo con pruebas de propiedades, sin tocar lo ya guardado (D-29) | E-078 | `136f8dd` |
| T-R7-5: autorización en el servidor, rutas revisadas y matrices por perfil | E-079 | `519faee` |
| T-R7-6: límites de entradas, ficheros y tareas con sus pruebas; conexiones quietas acotadas | E-080 | `8879732` |
| Revisión automática de E-080: plazo total para la cabecera y conexiones por red IPv6 /64 | E-081 | `ab5614e` |
| T-R7-7: minimización en registro, enlaces personales, mensajes, IA y laboratorio (D-30) | E-082 | `39d3852` |
| T-INI-07: catálogo de invariantes protegidos con sus pruebas; las que faltaban | E-083 | `2569e6b` |
| T-PROC-01: límites del encargo respetados; ningún servicio real se activa solo | E-084 | `79d98db` |
| T-PROC-02: ciclo por tarea anotado, con comprobación automática del seguimiento | E-085 | `6b5339c` |
| T-CIERRE-01: restauración ensayada en carpeta aislada con relaciones, dinero y diario comprobados | E-086 | `a097a24` |
| T-CIERRE-02: cierre con el verificador, las órdenes de calidad y ENTREGA.md | E-087 | `8a76451` |

## Siguiente acción
**Ingeniería disponible completada; validaciones o activaciones externas pendientes.** Las tareas de ingeniería de las fases 0 a 6, las transversales y las de cierre están hechas. El resumen de la entrega está en `ENTREGA.md`.

Lo que queda espera a otros. Cada cosa tiene su bloqueo en `BLOQUEOS.md` y su tarea `BLOQUEADA_EXTERNA` en `TAREAS.json`, con el paso exacto para resolverla:
- piloto (B-08), AEAT con el certificado de la clínica (B-06), Meta (B-07);
- firma de código (B-25) y clave de versiones (B-27);
- homologación foral (B-12);
- revisiones externas (B-13, con el paquete de `docs/revision_externa`);
- TPV (B-14), transcripción (B-15), revisión clínica (B-16) y traducciones (B-17);
- nube (B-18), datáfono y financiación (B-19), aseguradoras (B-20) y grupo real de clínicas (B-21).

Si se reanuda:
1. Comprobar que la batería y el verificador siguen en verde: `tools/aislado.sh cargo run --locked --example verificar_objetivo`.
2. Atender lo que llegue de los bloqueos: cada uno dice qué prueba hacer después.
3. No hacer push, fusión ni versiones sin una autorización expresa.

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

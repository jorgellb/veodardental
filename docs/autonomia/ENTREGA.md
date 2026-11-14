# Entrega del encargo VeodarDental (fases 0 a 6)

**Resultado: ingeniería disponible completada; validaciones o activaciones externas pendientes.**

No está completa la hoja de ruta. Faltan:
- el piloto real;
- las certificaciones y homologaciones;
- las integraciones con proveedores reales;
- las revisiones de gestoría, DPO, jurista, auditoría y traductores.

Todo eso queda pendiente, cada parte con su bloqueo (apartado 6). La
preparación técnica de cada una está hecha y probada con dobles de prueba. Un
doble de prueba no equivale a una prueba con el proveedor real.

- Rama local: `autonomia/fases-0-6`, desde `main` (`ade418b`). Sin remoto, sin push, sin fusión y sin versiones publicadas.
- Fecha de cierre: 2026-11-14.
- Seguimiento:
  - `PROGRESO.md`: una fila por tarea, con su evidencia y su commit;
  - `EVIDENCIAS.md`: E-001 a E-087;
  - `DECISIONES.md`: D-01 a D-30;
  - `BLOQUEOS.md`: B-01 a B-27;
  - `TAREAS.json`: el inventario.

## 1. Verificador del objetivo
`tools/aislado.sh cargo run --locked --example verificar_objetivo` (sin
`--progreso`, el modo de cierre) termina con código de salida 0:

```text
Verificador del objetivo de VeodarDental
  líneas del encargo con requisitos: 134
  requisitos: 134 (con tareas: 134)
  requisitos satisfechos por completo: 106; solo con validaciones externas pendientes: 28; con ingeniería pendiente: 0
  tareas: 127 · hechas: 100 · ingeniería pendiente: 0 · bloqueadas por dependencia externa: 27
RESULTADO: condición de cierre cumplida
```

- 134 requisitos, todos con tareas. 106 se cumplen por completo y 28 solo esperan validaciones externas. Ninguno tiene ingeniería pendiente.
- 127 tareas: 100 hechas, 0 de ingeniería pendientes y 27 bloqueadas por una dependencia externa, cada una con su bloqueo descrito.

## 2. Órdenes de calidad ejecutadas al cierre
Todas con `tools/aislado.sh` (carpetas temporales, sin datos de este
ordenador) y `--offline`:

| Orden | Código de salida | Resultado |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | sin cambios |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | sin avisos |
| `cargo test --workspace --all-targets --locked` | 0 | 575 pasan (573 del programa y 2 del ensayo en la nube), 0 fallan y 10 ignoradas; igual en tres pasadas seguidas |
| `cargo build --workspace --release --locked` | 0 | compilación de distribución para Linux x86_64 |

- **Matriz soportada:** el paquete no tiene funcionalidades opcionales (`[features]`), así que no hay combinaciones que ejecutar.
- **Sin otros sistemas:** Windows y macOS no se han probado en máquinas reales (B-24). Compilar en Linux no prueba esos sistemas, y no se citan resultados de CI.
- **Al empezar:** 302 pruebas que pasan y 10 ignoradas, en `ade418b` (E-001). Ninguna prueba se ha quitado ni rebajado. Cuando una prueba cambió de expectativa, fue por un cambio de comportamiento decidido y anotado. Por ejemplo, la orden al laboratorio sin el nombre por defecto (D-30).
- **Ignoradas:** las 10 de siempre. Son manuales o de integración: servir la página de ejemplo, ensayos con proveedores reales, etc.
- **Fallo encontrado al cierre y corregido:** tres pruebas de «nadie escuchando» abrían y cerraban un puerto, y otra prueba que corría a la vez podía cogerlo. Fue una carrera de las pruebas, no del programa. Ahora usan un puerto fuera de los que da el sistema (E-087).

## 3. Lo entregado
Cada línea cita su evidencia en `EVIDENCIAS.md`. Todo con pruebas
automáticas sobre datos inventados.

- **Fase 0, lo abierto:**
  - aislamiento de rutas (E-002), seguimiento y verificador del objetivo (E-004);
  - exención de IVA configurable para las facturas nuevas (E-005; la decisión es de la gestoría, B-01).
- **Fase 1, revisión de calidad de las diez áreas** (E-006 a E-017), cada una con su revisión en `docs/autonomia/revision/`:
  - documentos y consentimientos, recetas y vademécum;
  - odontograma y periodontograma, bancos, informes, comunicaciones;
  - ortodoncia, IA de radiografías, cita online, telefonía y laboratorio.
  - Además:
    - todas las pantallas caben a 1280×720 y a 1280×800 (E-018);
    - concurrencia, fallos, protocolo y entradas al azar (E-019, `docs/autonomia/concurrencia.md`);
    - autoridad del servidor (E-020).
- **Fase 2, migración:** motor, asistente, conciliación, perfiles de GESDEN, Novaclinic, Clinic Cloud y Dentalink, con ficheros sintéticos (E-021). Los simulacros reales necesitan exportaciones autorizadas (B-03 a B-05).
- **Fase 3, piloto y operación:**
  - ensayo de instalación en Linux (E-022), actualizaciones coordinadas y vuelta atrás (E-023);
  - empaquetado y firma condicionada a secretos (E-024), registro de errores e informe para soporte (E-025);
  - ensayo de restauración y clave de recuperación (E-026), VERI*FACTU contra una AEAT simulada (E-027);
  - SMS (E-028), instrumentos del piloto (E-029) y guías comprobadas contra el programa (E-030).
- **Fase 4, normativa y protección de datos:**
  - normativa consultada (E-031), registro de accesos (E-032), derechos RGPD (E-033);
  - paso de la carpeta compartida al servidor (E-034), registro de jornada (E-035, E-036), libros registro (E-037);
  - receta privada (E-038), cifrado de datos y copias (E-039, E-040), facturación foral preparada sin homologar (E-041);
  - paquete para las revisiones externas (E-042, `docs/revision_externa/`).
- **Fase 5, lo que distingue al producto:**
  - cuadro de mando (E-043), pago por internet sin cargos (E-044), plan por fases (E-045);
  - notas de evolución con dictado (E-046, E-047), periodontograma por voz (E-048);
  - visor DICOM/CBCT y STL (E-049, E-050), alertas clínicas (E-051, E-052);
  - esterilización (E-053 a E-055), encuestas (E-056), «Mi espacio» (E-057, E-058);
  - cinco idiomas (E-059, E-060) y campañas con consentimiento (E-061, E-062).
- **Fase 6, crecer:**
  - grupos de clínicas (E-063, E-076), servidor en la nube opcional ensayado en local (E-064, E-065);
  - datáfono y financiación (E-066 a E-068), aseguradoras (E-069, E-070);
  - pedidos por stock mínimo (E-071 a E-073), modo tableta y contraste (E-074);
  - calidad continua y mediciones por versión (E-075).
- **Transversal y cierre:**
  - funciones del informe comprobadas (E-077);
  - dinero exacto al céntimo con pruebas de propiedades (E-078, D-29);
  - autorización en el servidor por perfil (E-079);
  - límites de entradas y tareas (E-080, E-081);
  - minimización de datos (E-082, D-30);
  - catálogo de invariantes (E-083, `docs/revision_externa/invariantes.md`);
  - límites del encargo (E-084), ciclo por tarea (E-085), ensayo de restauración (E-086) y este cierre (E-087).

## 4. Evidencia de los flujos críticos
- **Invariantes protegidos** (secretos, permisos, límites, guardado, numeración, facturas, idempotencia, importes, implantes, fusión, importaciones y copias): `docs/revision_externa/invariantes.md`, con sus pruebas (E-083).
- **Restauración:** la copia se abre en una carpeta aislada. Se comprueban registro a registro pacientes, citas, tratamientos, cobros, facturas, documentos e imágenes. También:
  - que todo es de un paciente de la copia;
  - el total cobrado, el saldo de cada paciente y cada caja;
  - la cadena de facturas;
  - el diario, tras un cierre brusco.
  - Pruebas: `ui_tests::a_restore_drill_runs_from_the_backups_tab` y `tools::tests::a_restore_drill_checks_the_contents_and_the_key_lives_apart` (E-086).
- **Concurrencia:** dos puestos a la vez sobre facturas, cobros, cuotas, remesas, material, extractos, correos, pedidos y citas pedidas por internet, con el servidor de verdad. Cada cosa se cobra, se gasta o se envía una vez (E-019, E-079, `docs/autonomia/concurrencia.md`). En la medición, 8 puestos × 20 guardados a la vez: mediana de 1 ms, percentil 95 de 2 ms y 0 guardados perdidos (E-075).
- **Pantallas:** con egui_kittest, cada pantalla y cada pestaña caben a 1280×720 y 1280×800, también en modo tableta, y los colores secundarios cumplen el contraste WCAG 2.1 (E-018, E-074).
- **Rendimiento:** `docs/autonomia/mediciones/v0.1.0.md`, repetible con `tools/medir.sh` (E-075). Es una clínica sintética de 8 000 pacientes, con 226,5 MB de datos:
  - guardado local de un cambio: de 0,021 a 0,147 s;
  - cambio enviado: 4,54 KB;
  - entrada de un puesto de recepción comprimida: 13,0 MB.
  - Son mediciones nuevas en este ordenador. No se comparan con las cifras históricas del informe (unos 220 MB y 20 000 pacientes) porque los datos no son los mismos.

## 5. Limitaciones concretas
- **Sin datos reales:** no se ha abierto el programa contra los datos de la clínica, ni se ha leído o borrado ninguna copia real. Tampoco las del 11 y 12 de noviembre (B-02).
- **Sin proveedores reales:** AEAT, Meta, el banco (Redsys y Bizum), el SMS, la receta electrónica, la transcripción, el datáfono, las aseguradoras, la nube, la centralita y el laboratorio solo se han probado con dobles en 127.0.0.1. De serie, todos están apagados o en pruebas (`golive::tests::every_real_service_starts_switched_off`, E-084).
- **Sistemas:** solo Linux. Los instaladores van sin firma de código (B-25) y las versiones sin clave de firma (B-27).
- **Seguridad** (`docs/revision_externa/auditoria_seguridad.md`, apartado 6):
  - las imágenes no van cifradas por el programa, porque las comparten los programas de radiología;
  - quien escriba en la carpeta de datos puede quitar las últimas líneas del diario;
  - la clave de la clínica la comparten todos los puestos;
  - con cuatro direcciones distintas se pueden ocupar las 64 conexiones del servidor durante 20 s cada vez, sin sitios reservados para los puestos.
- **Protección de datos:**
  - un texto quemado en la propia radiografía viaja con los píxeles a la IA;
  - la supresión está desactivada hasta que el DPO fije los plazos (B-10).
- **Migración:** los perfiles de los programas de origen están comprobados con ficheros sintéticos, no con exportaciones reales (B-03 a B-05).

## 6. Dependencias externas restantes, por responsable
Cada una, con el paso mínimo para resolverla. El detalle (qué falta, por qué,
parte afectada, preparación terminada y prueba posterior) está en
`BLOQUEOS.md`.

- **Clínica (Dirección):**
  - B-02, copias del 11 y 12 de noviembre: decidir si se conservan o se borran desde Configuración › Copias.
  - B-07, WhatsApp Business: crear la cuenta y la plantilla siguiendo Configuración › WhatsApp y enviar a un número de prueba propio.
  - B-14, TPV virtual y Bizum: pedir al banco el TPV virtual y copiar sus datos de pruebas en Configuración › Pago por internet.
  - B-15, transcripción: elegir proveedor o modelo y configurarlo en Configuración › Inteligencia artificial.
  - B-19, datáfono y financiación: pedir al proveedor su especificación y credenciales de pruebas.
  - B-23, centralita y laboratorio: configurar la centralita en Configuración › Telefonía y enviar una orden de prueba al laboratorio.
- **Clínica con autorización escrita, o proveedor del programa de origen:**
  - B-03, GESDEN: una copia de prueba de la base (o el listado de tablas y columnas) en un entorno autorizado.
  - B-04, Novaclinic: una copia de prueba autorizada.
  - B-05, Clinic Cloud y Dentalink: una exportación de ejemplo de cada programa.
- **Clínica y gestoría:**
  - B-06, AEAT: configurar el certificado en Configuración › VERI*FACTU, en el entorno de pruebas, y pulsar «Enviar ahora».
- **Gestoría:**
  - B-01, exención de IVA: elegir la referencia; la clínica la activa en Configuración › Facturación.
  - B-11, exportación contable: importar en su programa un fichero de prueba con datos ficticios.
- **DPO de la clínica:**
  - B-10, plazos de conservación y supresión: revisar `docs/revision_externa/dpo.md` y fijar los plazos en la configuración.
- **Jurista o DPO:**
  - B-22, firmas y consentimientos: revisar `docs/autonomia/revision/area1.md`.
- **Dirección médica:**
  - B-16, vademécum y alertas: revisar el listado de reglas y fuentes en `docs/revision_externa`.
- **Traductores o profesionales nativos:**
  - B-17, traducciones clínicas y legales: revisar los textos marcados en `docs/revision_externa/traducciones.md`.
- **Asesoría laboral:**
  - B-26, registro de jornada: revisar el registro y su exportación.
- **Clínica y aseguradoras:**
  - B-20: pedir a cada aseguradora su canal de integración.
- **Propietario del producto:**
  - B-13, revisiones externas: encargarlas con el paquete de `docs/revision_externa`.
  - B-24, Windows y macOS: lanzar el flujo «Instaladores» o probar en máquinas reales.
  - B-25, firma de código: comprar los certificados y guardarlos como secretos del repositorio.
  - B-27, clave de versiones: generarla, guardar la privada como secreto y añadir la firma del manifiesto a la CI.
  - B-21, grupo real de clínicas: acordar el despliegue con un grupo.
- **Propietario del producto o clínica:**
  - B-18, nube: elegir un proveedor en la UE y firmar el encargo de tratamiento.
- **Propietario del producto con el Consejo General de Dentistas y el CGCOM:**
  - B-09, receta electrónica privada: presentar el expediente de Configuración › Receta electrónica.
- **Propietario del producto y clínica con domicilio fiscal foral:**
  - B-12, facturación foral: pedir el alta en el entorno de pruebas de la diputación.
- **Clínica piloto y propietario del producto:**
  - B-08, piloto de un mes: acordarlo, instalar con la lista de instalación y activar los instrumentos de medida.

## 7. Git
- **Commits propios:** 157 en `autonomia/fases-0-6` sobre `main` (`ade418b`), contando el de este cierre y su seguimiento. Van del primero (`3e48461`, aislamiento) al último. Todos llevan la línea «Co-Authored-By: Claude Opus 5.5».
- **HEAD final y estado:** se muestran al cierre en la conversación. El estado final es limpio, sin cambios sin guardar.
- **Cambios ajenos que había al empezar:**
  - `VEODARDENTAL_GOAL.md`, el encargo del propietario: se guardó sin cambios en su propio commit, `50431a3`, que cita su SHA-256 (D-04);
  - `PLAN_DE_DESARROLLO.md`, del asistente de la sesión anterior: guardado en `6f8d19c`.
  - No había más ramas, `stash` ni cambios sin guardar (`ESTADO_INICIAL.md`).
- No se ha hecho reset destructivo, limpieza indiscriminada, rebase, force push ni push.

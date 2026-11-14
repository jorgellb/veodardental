# Calidad continua

Lo que se comprueba en cada cambio y cómo se repite (T-F6-07). Todo va con
carpetas aisladas (`tools/aislado.sh`) y datos sintéticos: ni las pruebas ni
las mediciones leen o escriben los datos, ajustes o copias de este ordenador.

## En cada cambio (la batería)
```sh
tools/aislado.sh cargo fmt --all -- --check
tools/aislado.sh cargo clippy --workspace --all-targets --locked -- -D warnings
tools/aislado.sh cargo test --workspace --all-targets --locked
```
- **Acotada:** las pruebas tardan unos segundos (sin contar la compilación): unos 3,5 s las del programa y alrededor de 1 s el ensayo en la nube (`tests/ensayo_nube.rs`). Cada prueba con red usa 127.0.0.1 y servidores de prueba en el propio proceso.
- **Reproducible:** las pruebas con entradas al azar llevan semilla fija (`src/test_rng.rs`), así que cada ejecución prueba lo mismo:
  - el protocolo del servidor con peticiones al azar (`server::tests::the_server_survives_random_requests_and_its_data_stays_readable`, 4 000 peticiones) y con bytes al azar (`server::tests::the_request_reader_survives_random_bytes`);
  - los extractos del banco (`bank::tests::statement_tables_and_amounts_survive_random_input`);
  - la combinación de cambios de dos puestos (`shared::tests::merging_random_edits_keeps_what_each_station_did`);
  - DICOM, STL, el dictado del periodontograma y los planes de pago.
- **Pantallas:** las pruebas con egui_kittest manejan la interfaz como un usuario: pulsan y escriben, también con el teclado. Además:
  - `ui_tests::every_screen_fits_1280_wide`: cada pantalla, cada pestaña de la ficha, de Informes y de Configuración cabe a 1280×720 y 1280×800;
  - `ui_tests::tablet_mode_has_touch_sized_controls_and_fits_1280`: lo mismo en modo tableta, con controles del tamaño del dedo;
  - `theme::tests::the_secondary_colours_meet_the_contrast_levels`: contraste de WCAG 2.1.
- **Permisos en el servidor:** cada regla tiene su prueba con el servidor de verdad. Cuando se añadió, se comprobó que la prueba falla si se quita la regla (anotado en `docs/autonomia/EVIDENCIAS.md`).

## Mediciones por versión
```sh
tools/medir.sh            # clínica grande: 8 000 pacientes (unos 220 MB), 3 clínicas, 8 puestos × 20 guardados
tools/medir.sh --escala 0.2 --clinicas 2 --puestos 4 --guardados 10
```
- Guarda `docs/autonomia/mediciones/v<versión>.md` (y `.json`), con la fecha y el ordenador.
- **Qué mide** (`src/rendimiento.rs`):
  - tamaño de los datos y memoria;
  - guardado local de un cambio y su diario;
  - lo que se envía por un cambio;
  - la entrada de un puesto, comprimida;
  - varias clínicas: disco, abrir una y memoria;
  - varios puestos guardando a la vez en un servidor de verdad: mediana, percentil 95 y guardados perdidos, que tienen que ser 0.
- Sin una raíz aislada no arranca: el servidor de la medición hace copias de seguridad.
- En la batería va la misma medición en pequeño (`rendimiento::tests::a_small_measurement_runs_and_is_complete`): tiene que salir entera, y con varios puestos a la vez no se pierde nada.

## Para la auditoría externa
- **Seguridad:** `docs/revision_externa/auditoria_seguridad.md` (arquitectura, autenticación y permisos, criptografía, robustez, hallazgos corregidos y límites conocidos).
- **Protección de datos:** `docs/revision_externa/dpo.md`.
- **Gestoría:** `docs/revision_externa/gestoria.md`.
- **Traducciones:** `docs/revision_externa/traducciones.md`.
- **Lo que se ha hecho, con sus pruebas y sus commits:** `docs/autonomia/EVIDENCIAS.md` y `docs/autonomia/PROGRESO.md`. Las decisiones están en `docs/autonomia/DECISIONES.md` y lo que espera a otros, en `docs/autonomia/BLOQUEOS.md`.
- `docs_tests::tests::the_review_package_only_cites_what_exists` comprueba que el paquete de revisión no cita pruebas ni ficheros que no existen.

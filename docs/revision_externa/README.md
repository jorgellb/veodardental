# Revisión externa de VeodarDental

Esta carpeta prepara las revisiones que una revisión propia no sustituye
(fase 4, T-F4-09):

| Revisor | Documento | Qué decide |
|---|---|---|
| Gestoría o asesoría fiscal | [gestoria.md](gestoria.md) | Exención de IVA, VERI*FACTU, libros registro, territorio foral y exportación contable |
| Responsable o delegado de protección de datos (DPO) | [dpo.md](dpo.md) | Plazos de conservación, supresión, derechos, encargados del tratamiento y medidas de seguridad |
| Auditoría de seguridad externa | [auditoria_seguridad.md](auditoria_seguridad.md) | Alcance, pruebas de intrusión y revisión de la criptografía y de los permisos |

## Qué es y qué no es
- **Es** una relación, para cada revisor, de lo hecho, dónde está en el
  código, qué prueba lo comprueba y qué decisión le queda a él.
- **No es** una aprobación ni una certificación. Ninguna línea de estos
  documentos afirma que el programa cumpla una norma: eso lo dicen los
  revisores, y su informe se anota en `docs/autonomia/BLOQUEOS.md` (B-13 y
  los que se citan en cada documento).
- **No lleva datos reales.** Las pruebas usan datos ficticios. Las capturas de
  `docs/autonomia/capturas` son de los datos de ejemplo.

## Cómo comprobar lo que se cita
- Cada prueba citada (módulo, «tests» y nombre) se ejecuta con
  `tools/aislado.sh cargo test --locked nombre`. La batería completa:
  `tools/aislado.sh cargo test --workspace --all-targets --locked`.
- Cada evidencia `E-0NN` está en `docs/autonomia/EVIDENCIAS.md`, con las
  órdenes ejecutadas y su código de salida.
- Cada bloqueo `B-NN` está en `docs/autonomia/BLOQUEOS.md`, con lo que falta y
  quién lo desbloquea.
- Las fuentes oficiales consultadas, con fecha y versión, están en
  `docs/normativa.md`.
- `docs_tests::tests::the_review_package_only_cites_what_exists` comprueba que
  todo lo citado en esta carpeta existe: ficheros, pruebas, evidencias y
  bloqueos.

## Cómo devolver la revisión
Para cada punto marcado «Decide», basta con una de estas respuestas, con
fecha y firma:
- **conforme**;
- **conforme con cambios**, diciendo cuáles;
- **no conforme**, diciendo por qué.

Nada de lo que depende de esas decisiones se activa antes en datos reales.

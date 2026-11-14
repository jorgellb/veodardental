# Para la gestoría o asesoría fiscal

Puntos fiscales del programa que necesitan su criterio. Las fuentes oficiales,
con fecha y versión, están en `docs/normativa.md`.

## 1. Exención de IVA de las facturas (B-01)
- **Hecho:** la referencia legal que imprime la factura exenta se elige en
  Configuración › Facturación y cobros:
  - 20.Uno.3º, asistencia sanitaria (la de siempre, por defecto);
  - 20.Uno.5º, servicios de odontólogos y estomatólogos.
- Cada factura guarda la suya al emitirla. Las ya emitidas no cambian.
- En VERI*FACTU se declara la causa E1 (exenta por el artículo 20); el número
  concreto va solo en la leyenda.
- **Dónde:** `src/model.rs` (`ClinicConfig::vat_exemption`, `Invoice::exemption`).
- **Pruebas:** `verifactu::tests::the_exemption_article_stays_with_each_invoice` (E-005).
- **Decide:** qué referencia corresponde a la clínica y desde qué fecha.

## 2. VERI*FACTU (B-06)
- **Hecho:**
  - registro de alta encadenado por huella en cada factura;
  - subsanaciones y anulaciones;
  - QR con la leyenda «VERI*FACTU» y envío por lotes;
  - un rechazo para el envío automático, y una caída se reintenta;
  - una respuesta perdida se reenvía igual y se concilia.
- **Probado contra:** una AEAT simulada (E-027), no contra la real.
- **Dónde:** `src/verifactu.rs` y Configuración › Facturación.
- **Pruebas:**
  - `verifactu::tests::corrections_and_cancellations_join_the_chain`;
  - `verifactu::tests::a_lost_answer_is_resent_identically_and_reconciled_as_registered`;
  - `verifactu::tests::a_rejected_submission_stops_the_automatic_retries_but_a_server_fault_does_not`.
- **Decide:**
  - la fecha de obligación de la clínica (1/1/2027 si tributa por el
    Impuesto sobre Sociedades, 1/7/2027 en otro caso, según el calendario
    vigente consultado);
  - si se opta por VERI*FACTU o por el registro sin envío;
  - quién hace la prueba con el certificado de la clínica en el entorno de
    pruebas de la AEAT.

## 3. Libros registro de IVA e IRPF (B-11)
- **Hecho:** exportación XLSX en el formato electrónico común de la AEAT
  (diseños actualizados a 01/01/2026 y plantilla de 09/09/2026):
  - libros unificados («T») para una persona física;
  - libros de IVA («C»), sin las columnas del IRPF, para una sociedad;
  - antes de entregarse se comprueban las validaciones 2, 4, 5, 11, 32, 33 y 37 de la AEAT.
- **Dónde:** `src/libros.rs` y Configuración › Facturación (libros registro).
- **Pruebas:**
  - `libros::tests::the_books_follow_the_official_layout_and_validations`;
  - `libros::tests::a_company_gets_the_vat_books_without_income_tax_columns` (E-037).
- **Decide:**
  - el epígrafe del IAE y el tipo de actividad de la clínica;
  - el concepto de gasto (claves G) propuesto para cada categoría de gasto del programa;
  - el tratamiento de la cuota soportada no deducible (actividad exenta);
  - si el fichero se importa bien en su programa.
- **A3, ContaSol y Sage:** no se ha encontrado una especificación oficial
  pública de sus ficheros de importación, así que no se generan. Si la
  gestoría facilita la de su programa, se implementa y se prueba con ella.

## 4. Territorio foral (B-12)
- **Hecho:**
  - en Configuración › Facturación › Territorio fiscal se elige el territorio, y no cambia nada sin la confirmación de la gestoría;
  - confirmado un territorio foral, se deja de enviar a la AEAT por VERI*FACTU;
  - de TicketBAI están el identificativo y el QR según la especificación 1.0;
  - faltan el fichero firmado, el envío y la homologación;
  - Navarra no está.
- **Dónde:** `src/foral.rs`.
- **Pruebas:**
  - `foral::tests::nothing_is_activated_without_confirmation_and_homologation`;
  - `foral::tbai::tests::the_examples_of_the_specification_come_out_exactly` (E-041).
- **Decide:** si la clínica tributa por la normativa foral (según el
  Concierto Económico, no por el domicilio) y, si es así, en qué territorio.

## 5. Lo que no se ha hecho y no debe darse por hecho
- No se ha presentado nada ante la AEAT ni ante ninguna hacienda foral.
- No se ha emitido ninguna factura real con el programa.
- La exportación contable no está aceptada hasta que la gestoría la importe
  en su programa.

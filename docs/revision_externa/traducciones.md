# Para quien revise las traducciones (catalán, euskera, gallego e inglés)

Las traducciones del programa no las ha revisado todavía un traductor nativo
(`i18n::REVIEWED` es falso, T-F5-11X, B-17).

## 1. Lo que hay que revisar
- **El catálogo de `src/i18n.rs`:** cada texto en castellano con su traducción a los otros cuatro idiomas.
  - Navegación de la interfaz: menú y pestañas de la ficha.
  - Avisos a pacientes de las plantillas de siempre: recordatorio de cita, revisión, cumpleaños, presupuesto pendiente y encuesta. Hay que mirar el tono y que los comodines (`[NOMBRE]`, `[FECHA]`…) queden bien en la frase.
  - Páginas del paciente: «Mi espacio», la encuesta y la entrada.
  - Fechas: nombres de los días y de los meses, y la forma de la fecha larga (en euskera, «azaroaren 14a»).
- **Cuidado especial con el euskera:** los comodines no admiten sufijos (un `[CLINICA]` no puede llevar «-n»), así que las frases se han construido para no necesitarlos. Hay que comprobar que suenan naturales.

## 2. Lo que no se ha traducido (y por qué)
- **Consentimientos y documentos legales o clínicos:** un consentimiento mal traducido no es válido.
  - El programa admite plantillas traducidas (`DocTemplate::translation_of`) que solo se usan con un paciente de ese idioma si están validadas (`DocTemplate::validated`, con fecha y quién).
  - Hasta entonces, el documento sale en castellano.
  - Prueba: `i18n::tests::messages_and_documents_follow_the_patient_language`.
- **Las páginas fijas** (cita online, «Mi cita», presupuesto para firmar, documento para firmar y «Mis recetas») siguen en castellano.
- **El resto de la interfaz** sale en castellano. Lo traducido son la navegación y las páginas del paciente.

## 3. Cómo se comprueba
- `i18n::tests::the_catalog_is_complete_with_the_same_placeholders`: todos los idiomas completos y con los mismos comodines que el castellano.
- `i18n::tests::every_text_used_with_t_is_in_the_catalog`: ningún texto del programa se queda fuera del catálogo.
- `ui_tests::the_interface_in_each_language_is_translated_and_fits`: el menú y la ficha en cada idioma caben a 1280 de ancho.

## 4. Cómo devolver la revisión
Para cada idioma, la lista de cambios. Las correcciones se hacen en `src/i18n.rs` y, cuando un idioma está revisado, se anota aquí con fecha y revisor.

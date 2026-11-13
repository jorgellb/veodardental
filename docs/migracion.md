# Migración desde otro programa

Actualizado el 13/11/2026. Qué se puede traer a VeodarDental, desde qué formatos y con qué reglas. Está en **Configuración › Importar datos › Migración completa desde otro programa**; el código está en `src/migracion.rs` (motor) y `src/screens/migrar.rs` (asistente).

## Qué está comprobado y qué no
- **Comprobado con datos sintéticos** (inventados, sin personas reales; las pruebas lo dicen en su comentario):
  - el motor, el asistente, el simulacro, el informe de conciliación y la repetición sin duplicados;
  - la lectura de Excel/CSV;
  - las columnas de la consulta conocida de pacientes de GESDEN.
- **Sin comprobar con datos reales:**
  - **GESDEN y Novaclinic:** el resto de sus tablas (historia, citas, cobros, saldos, documentos). Bloqueos B-03 y B-04.
  - **Clinic Cloud y Dentalink:** sus exportaciones. Bloqueo B-05.

  Para todo esto no se ha inventado ningún esquema. Las columnas se asignan en el asistente, a mano o con un perfil guardado, y el simulacro dice qué filas no se entienden.

## Cómo se hace
1. **Origen.** Escriba el nombre del programa anterior: va en los identificadores de lo importado. Después, para cada clase de dato:
   - lea su hoja: un fichero, una carpeta entera con «Leer la carpeta» o una tabla o consulta de SQL Server;
   - revise sus columnas: cada campo muestra un ejemplo de la primera fila, y debajo hay una vista previa de tres filas;
   - si quiere, guarde las columnas como perfil para la próxima vez.
2. **Opciones.**
   - Qué hacer con los posibles duplicados (para todos o uno a uno, ver más abajo).
   - Si se ajustan los saldos.
   - El doctor para lo que no dice cuál.
3. **Simulacro.** Trabaja sobre una copia: no cambia nada. Da el informe completo.
4. **Importar lo del simulacro.** Solo se puede si el simulacro es de lo que hay ahora; si cambia algo, hay que repetirlo. Antes de importar se hace una copia de seguridad, y la importación queda en el registro de actividad.

Hágala con los demás puestos cerrados. El programa anterior solo se lee: ficheros abiertos para lectura y consultas `SELECT`. Se comprueba en `migracion::tests::an_end_to_end_synthetic_migration_squares`.

## Formatos de fichero
- **Ficheros:** Excel (`.xlsx`, `.xls`, `.xlsm`), OpenDocument (`.ods`) y texto (`.csv`, `.txt`).
  - Del libro se lee la primera hoja; la primera fila son los títulos.
  - En el texto, el separador se deduce de la primera línea (`;`, tabulador o `,`), admite comillas con saltos de línea dentro y UTF-8 o UTF-16 con su marca.
- **SQL Server:**
  - Las tablas o consultas `SELECT` se leen desde la tarjeta de conexión (TDS 7.3: SQL Server 2008 y posteriores).
  - Se leen como mucho 500.000 filas por hoja, y hasta 1.000.000 de pacientes de GESDEN.
- **Carpeta:** cada hoja se reconoce por el nombre del fichero:

  | Contiene | Clase de dato |
  |---|---|
  | `pacient` | Pacientes |
  | `tratam` | Tratamientos |
  | `presup` | Presupuestos |
  | `cita` | Citas |
  | `cobro` o `pago` | Cobros |
  | `saldo` o `deuda` | Saldos |
  | `document` o `imagen` | Documentos |

## Cómo se leen los valores
- **Fechas:**
  - Se leen `dd/mm/aaaa`, `dd-mm-aaaa`, `dd.mm.aaaa`, `aaaa-mm-dd` (con hora detrás o sin ella) y el número de serie de Excel.
  - Con el año en dos cifras:
    - las citas, los cobros y los tratamientos son del 2000 en adelante (`03/02/27` es 2027);
    - las fechas de nacimiento futuras pasan al siglo anterior.
  - Una fecha de nacimiento imposible (31 de febrero, futura o anterior a 1900) se deja en blanco, con aviso.
- **Horas:** `09:45`, `09:45:00`, `9.45` o una fecha con la hora (`1899-12-30 09:45:00`).
- **Importes:**
  - Se leen `1.234,56`, `1,234.56`, `1234.56` y `12,5`.
  - El signo negativo se escribe delante, detrás o entre paréntesis.
  - Pueden llevar símbolo de moneda.
  - Se guardan redondeados al céntimo, y la conciliación se hace en céntimos exactos.
- **Piezas:** numeración FDI, de 11 a 48 y de 51 a 85, separadas por cualquier cosa; lo demás se ignora.
- **Sexo:**
  - `H`, `V`, `hombre` y `varón` son hombre; `F`, `mujer` y `femenino` son mujer.
  - Una `M` suelta depende de la columna:
    - si en ella hay `H` o `V`, es mujer;
    - si hay `F`, es hombre (*male*);
    - si no hay ninguna de las dos, se deja en blanco, con aviso.
- **Estado de un tratamiento:**
  - Pendiente: «pend…», «previst…», «planific…», `P`, `0` o `no`.
  - Hecho: «hech…», «realiz…», «termin…», «finaliz…», `R`, `1` o `sí`.
  - Sin estado: hecho si la fecha ya pasó.
- **Estado de una cita:**
  - «anul…», «cancel…» o «rechaz…»: anulada.
  - «no acud…», «falt…» o «no vino»: no acude.
  - Pasada: finalizada.
  - «confirm…»: confirmada.
  - El resto: planificada.
- **Estado de un presupuesto:**
  - «acept…»: aceptado.
  - «rechaz…» o «anul…»: rechazado.
  - «cerr…» o «termin…»: cerrado.
  - El resto: pendiente.
- **Forma de pago:**
  - efectivo (o en blanco), tarjeta o TPV, transferencia, financiación, Bizum, domiciliación o recibo;
  - otra se conserva tal cual, con un aviso que la nombra.

## Campos por clase de dato
Un campo con asterisco es obligatorio.

**Identificador.** La columna `id` es el identificador del registro en el programa anterior. Con él, importar otra vez no duplica nada: cada registro guarda programa, tabla o fichero, clase de dato e identificador (`Db::imports`).

**Asignación automática de columnas.** Se busca primero el título exacto y después el título que contiene esas palabras. «Apellidos del paciente» no se toma como identificador. Los puntos no cuentan: «D.N.I.» es «dni».

| Clase | Campos (títulos que se reconocen solos) |
|---|---|
| Pacientes | **id*** (id paciente, código paciente, código, id) · **nombre*** · apellidos · NIF (nif, dni, nie, documento) · nacimiento · sexo · móvil · teléfono · correo · dirección · código postal · población · provincia · nº de historia (nº historial, historia, nhc) · fecha de alta · mutua o tarifa · alertas médicas (alertas, antecedentes) · alergias · observaciones · responsable (responsable, tutor, pagador) · NIF del responsable · relación (relación, parentesco) |
| Tratamientos | **id*** · **paciente*** · **fecha*** · **tratamiento*** (descripción, concepto) · código · piezas · **importe*** · estado · doctor |
| Presupuestos | **id*** (una fila por línea; las líneas con el mismo id son un presupuesto) · id de la línea · **paciente*** · **fecha*** · **línea*** (tratamiento) · código · piezas · **importe*** · título · estado · doctor |
| Citas | **id*** · **paciente*** · **fecha*** · **hora*** · duración (minutos; 30 si no dice) · motivo · doctor · estado |
| Cobros | **id*** (recibo) · **paciente*** · **fecha*** · **importe*** · forma de pago · concepto |
| Saldos | **paciente*** · **saldo*** (positivo: el paciente debe) |
| Documentos | **id*** · **paciente*** · **ruta*** (fichero) · título · fecha · tipo |

## Reglas
- **Ninguna fila se pierde.** Cada fila se importa, ya estaba importada o sale en el informe con su número de fila (la 1 son los títulos) y el porqué. Ejemplos: «el paciente S9 no está en la hoja de pacientes», «hora no válida «x»», o el número de historia del duplicado al que se parece.
- **Duplicados.**
  - Un paciente del origen se parece a uno que ya estaba aquí (y que no viene de este mismo origen) en tres casos:
    - mismo nombre y NIF;
    - mismo nombre y fecha de nacimiento;
    - solo el mismo NIF.
  - Para cada uno se decide, para todos o fila a fila:
    - **No importarlo** (por defecto): sale en el informe.
    - **Unirlo:** su historia pasa al que ya estaba. Solo es posible con el mismo nombre.
    - **Crearlo aparte.**
  - Dos personas nunca se unen por compartir el NIF (un hijo con el de su madre) ni el número de historia. Si el número de historia ya lo tiene otro paciente, el nuevo recibe otro, con aviso.
- **Deuda.**
  - La deuda de aquí sale de los tratamientos hechos menos los cobros importados; la del programa anterior, de la hoja de saldos. Con hoja de saldos, quien no sale en ella debe 0.
  - El informe compara las dos por paciente y en total.
  - La deuda de los pacientes que no se importan se separa: la cuenta sale (origen − excluida = aquí).
  - Con «Ajustar al del programa anterior» se apunta, una vez por paciente, un ajuste con la diferencia (código `MIGSALDO`, positivo o negativo):
    - no es un cobro ni un acto: no cuenta como producción ni comisión y no se factura;
    - no se apunta a pacientes unidos a uno que ya estaba, porque su saldo de aquí no es solo el del otro programa: se avisa de la diferencia.
- **Lo de antes, como historia.** Los tratamientos hechos se apuntan sin gastar material, sin crear recalls y marcados como cerrados allí (`settled_elsewhere`): no salen para facturar ni para liquidar a la mutua. Los pendientes son trabajo para aquí, y sí se facturan cuando se hagan.
- **Cobros importados.** Llevan su recibo del programa anterior (`PROGRAMA-id`), sin caja: no gastan la numeración de aquí ni descuadran la caja del día.
- **Citas.** Las pasadas quedan finalizadas, sin recordatorio; las futuras, con su recordatorio de siempre.
- **Documentos.**
  - Se conserva su ruta: el fichero se queda donde está. Una ruta relativa se completa con la carpeta indicada.
  - El informe avisa de cuántos ficheros no están en este ordenador. Las rutas de red (`\\servidor\recurso`) no se comprueban: en Windows, mirarlas ya conecta con ese servidor y le da las credenciales del usuario.
  - El tipo se deduce de la extensión y del campo «tipo» (panorámica, periapical, foto, TAC, escaneado 3D o documento).
- **Doctores y mutuas.** Se buscan por nombre o alias. Uno que no está va al doctor del paciente, o queda con tarifa privada, con un aviso que dice cuántas filas son.
- **Repetir y retomar.** Una importación cortada (o hecha por partes) se termina repitiéndola con todas las hojas, aunque las filas vengan en otro orden. Lo ya importado cuenta como «ya estaban».
  - Las líneas de presupuesto sin identificador propio se reconocen por su orden dentro del presupuesto.
  - Si el programa anterior cambia un saldo después de un ajuste, el ajuste no se repite: la diferencia sale en el informe.
- **Fusión posterior.** Si después se unen dos pacientes, lo importado pasa al que se conserva y cuenta como unido.

## El informe
- Por clase de dato: filas, nuevos, ya estaban y sin importar.
- **Conciliación** (programa anterior y VeodarDental):
  - pacientes;
  - deuda (y la de pacientes sin importar);
  - citas futuras;
  - presupuestos abiertos (pendientes y aceptados);
  - tratamientos pendientes;
  - ajustes de saldo.
- Pacientes que no cuadran, con lo que no cuadra.
- Posibles duplicados, con su decisión.
- Filas con errores, excluidas y avisos.
- «Copiar el informe» lo da en texto con tabuladores, para pegarlo en una hoja de cálculo o guardarlo.

## Por programa
| Programa | Qué hay | Qué falta |
|---|---|---|
| GESDEN / Novaclinic (Infomed) | Se reconocen por sus tablas (`TTos`, `Clientes`, `Pacientes`, `TGrupos`, `TTipoOdg`). El catálogo y las tarifas se importan como antes. Los pacientes entran en la migración con «Pacientes de GESDEN / Novaclinic»: su identificador interno `IdPac`, nº de historia, datos de contacto, población, provincia, mutua y notas (`mssql::gesden_patients_for_migration`). Las demás tablas se leen con «Desde SQL» y sus columnas se asignan en el asistente. | El mapeo comprobado de tratamientos, citas, cobros, saldos y documentos, y un simulacro real: B-03 y B-04 (T-F2-10, T-F2-11). |
| Clinic Cloud, Dentalink y otros con Excel/CSV | El asistente genérico con asignación automática, perfiles de columnas guardados (Configuración de la clínica, `mig_profiles`) y lectura de una carpeta entera. | Exportaciones reales para guardar y comprobar sus perfiles: B-05 (T-F2-12). |
| Cualquier programa con SQL Server | Tabla o consulta `SELECT` por clase de dato. | — |

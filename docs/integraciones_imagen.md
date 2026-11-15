# Integración con los aparatos de imagen

Cada clínica tiene sus aparatos (sensores, ortopantomógrafo, TAC, cámara, escáner intraoral) y cada uno trae el programa de su fabricante. Ese programa hace las tomas, guarda los estudios y los enseña con su visor profesional. VeodarDental no lo sustituye: lo abre con el paciente correcto, recoge lo que captura y lo vuelve a abrir cuando hace falta.

Este documento dice cómo se conecta cada aparato, qué está probado y qué falta. Revisado el 15/11/2026.

## Las formas de conectar un aparato

1. **Abrir su programa con el paciente** («Imágenes y enlaces», un enlace por aparato).
   - **Parámetros** para capturar y, aparte si hace falta, para ver las imágenes que ya tiene el paciente («Capturar» y «Ver»).
   - **Comodines:** `[IDPAC] [NUMPAC] [NOMPAC] [APELPAC] [NOMBRE] [NIF] [FECNAC] [SEXO] [CARPETA] [FICHERO]`.
   - **Formatos:**
     - la fecha como la pida el programa: `[FECNAC:AAAAMMDD]`, `[FECNAC:DD.MM.AAAA]`, `[FECNAC:DD,MM,AAAA]`…;
     - el sexo con sus letras: `[SEXO:MF]`;
     - los nombres en mayúsculas o sin espacios: `[APELPAC:MAYUS+SINESPACIOS]`.
     - Las comas del paciente no llegan a los parámetros.
   - **Fichero puente:** el `IMCONECT.ini` de siempre o uno con plantilla propia. Se escribe en ANSI (Windows-1252), que es como lo leen los programas de Windows, o en UTF-8 o UTF-16 si el programa lo pide.
   - **Plantillas** («Añadir desde una plantilla»):
     - las de partida: Carestream, CS 3D, Sopro, AIS, Romexis, Sidexis, un visor DICOM y un escáner;
     - Dexis e iRYS, con el formato que documenta Open Dental;
     - la captura directa con NAPS2 (abajo).
2. **Recoger lo que deja en una carpeta.**
   - **Tras abrir un aparato** para un paciente, su carpeta se vigila media hora, y lo que deje ahí entra solo en su ficha.
   - **Carpeta de exportación del enlace:** con «Traer lo nuevo sin pulsar Importar», lo seguro entra solo. Es seguro si está en la subcarpeta con su número de historia, o si es un DICOM con su número de historia y su nombre.
   - **Lo dudoso queda «por asignar»,** con el porqué, para que alguien lo confirme. Son tres casos:
     - solo coincide el nombre del fichero;
     - solo coinciden el nombre y la fecha de nacimiento;
     - la carpeta y el fichero no dicen lo mismo.
   - **Lo que aún se está escribiendo** se deja para la siguiente pasada.
   - **Un TAC** que llega como carpeta de cortes es una sola imagen.
3. **DICOM en red** (sección «Red DICOM» de «Imágenes y enlaces»). Lo atiende un ordenador de la clínica, con su puerto y su AE Title.
   - **Lista de trabajo** (Modality Worklist): el aparato pide los citados y nadie los teclea. Se entregan sin tildes y solo del día que se pide (hoy si no se dice).
   - **Recepción** (Storage): el aparato manda lo que captura, que entra en la ficha como en el punto 2.
   - **Verificación** (C-ECHO), para que el técnico pruebe la conexión.
   - **Protección,** porque DICOM no cifra ni pide contraseña:
     - solo se atiende a los aparatos de la lista, con su nombre DICOM y su IP, y sin lista no arranca;
     - solo desde direcciones de la red de la clínica;
     - las imágenes se escriben en disco según llegan, con un tope, y un reenvío no pisa la que había.
4. **Captura directa de un sensor o un escáner, sin el programa del fabricante,** con NAPS2, un programa libre de escaneo. NAPS2 habla TWAIN o WIA en Windows, SANE en Linux y Apple en macOS, y deja la imagen en la carpeta del paciente.

## El visor rápido de VeodarDental

- **Radiografías** con zoom, brillo, contraste, negativo y giro, y la comparación de dos imágenes.
- **DICOM:**
  - sin comprimir o comprimido en JPEG, JPEG sin pérdida, JPEG 2000 y RLE;
  - el TAC en vista axial, coronal y sagital, con su escala, su orientación y el paciente que dice el fichero.
  - JPEG-LS no se admite (necesitaría una biblioteca de C++): el aviso lo dice y queda para el visor del fabricante.
- **Modelos 3D** en STL, PLY y OBJ.
- **Análisis con IA** como borrador.

## Lo que está probado y lo que no

- **Probado aquí, con pruebas automáticas:**
  - los formatos de los comodines y la codificación del fichero puente;
  - las plantillas de Dexis e iRYS frente a su formato documentado;
  - qué se mete solo en una ficha y qué queda por asignar;
  - la carpeta vigilada;
  - el DICOM comprimido, con un RLE y un JPEG reales;
  - PLY y OBJ;
  - la red DICOM con un cliente DICOM de verdad por la red: verificación, lista de trabajo, envío de una imagen en trozos, reenvío y aparatos no admitidos.
- **Sin probar con aparatos reales:** ningún enlace, ninguna plantilla, la red DICOM con un TAC o un ortopantomógrafo de verdad, ni NAPS2 con un sensor. Se prueban en la instalación de cada clínica, y cada prueba buena se apunta aquí con la versión del programa del fabricante.
- **JPEG 2000:** el decodificador está incluido, pero no tiene una prueba propia con una imagen real.

## Lo que falta y por qué

- **VDDS-media,** el estándar de la asociación alemana de software dental para cambiar pacientes e imágenes con los programas de imagen.
  - La especificación es gratuita, pero la VDDS solo la entrega tras rellenar un formulario con los datos de la empresa: https://www.vdds.de/schnittstellen/vdds-media/
  - Sin ella no se programa: habría que inventarse el formato.
  - Mientras, los programas que lo usan suelen admitir también parámetros, carpetas o DICOM.
- **Sidexis (Dentsply Sirona):** su fichero de comunicación (secciones U, N y A) no tiene publicado el formato de los campos. Queda el enlace de partida por parámetros, que hay que comprobar con Sidexis.
- **Escáneres en la nube** (3Shape, Medit, iTero): enviar los modelos sin pasar por una carpeta necesita su API y un acuerdo con cada uno. Mientras, sus programas exportan STL o PLY a una carpeta.
- **Windows:** los programas de los fabricantes son casi siempre de Windows. En un Mac o un Linux, VeodarDental trae lo que dejan en una carpeta compartida, pero no puede abrirlos (el programa lo avisa al configurarlo).

## Orden para cada clínica

1. Hacer la lista de sus aparatos y de sus programas, con la versión de cada uno.
2. Elegir para cada aparato la forma de conectarlo. Si habla DICOM en red, mejor esa.
3. Configurarlo desde su plantilla, probar «Capturar» y «Ver» con un paciente de prueba y apuntar aquí lo que funciona.

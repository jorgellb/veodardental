# Integración con los aparatos de imagen

Cada clínica tiene sus aparatos (sensores, ortopantomógrafo, TAC, cámara, escáner intraoral) y cada uno trae el programa de su fabricante. Ese programa hace las tomas, guarda los estudios y los enseña con su visor profesional. VeodarDental no lo sustituye: lo abre con el paciente correcto, recoge lo que captura y lo vuelve a abrir cuando hace falta.

Este documento dice qué hace hoy el programa y qué le falta para poder integrarse con cualquier aparato del mercado. Revisado el 15/11/2026.

## Lo que hay hoy

- **Enlaces** («Imágenes y enlaces»). Cada aparato tiene:
  - el programa que se abre;
  - sus parámetros, con comodines: `[IDPAC] [NUMPAC] [NOMPAC] [APELPAC] [NIF] [FECNAC] [SEXO] [CARPETA]`;
  - un fichero puente opcional (`IMCONECT.ini`, con los datos del paciente);
  - la carpeta donde el programa del aparato deja lo que exporta.
- **Enlaces de partida:**
  - Carestream: RVG, panorámica y CS 3D Imaging;
  - Acteon: Sopro y AIS;
  - Planmeca Romexis;
  - Dentsply Sirona: Sidexis;
  - un visor DICOM;
  - un escáner (STL).
  - **Ninguno se ha probado con un aparato real.**
- **En la ficha:**
  - «Capturar con» abre el programa del aparato.
  - «Abrir en visor 3D» abre el estudio en el programa del fabricante.
  - «Importar de la carpeta del paciente» recoge lo exportado.
- **Visor rápido de VeodarDental:**
  - radiografías;
  - DICOM sin comprimir, con el TAC en sus tres planos;
  - modelos STL;
  - comparación de dos imágenes y análisis con IA como borrador.

## Lo que falta

### Arreglos pequeños que abren muchos aparatos

1. **Codificación del fichero puente** (probable fallo).
   - `IMCONECT.ini` se escribe en UTF-8.
   - Los programas de Windows que leen ficheros `.ini` suelen leerlos en ANSI (Windows-1252). Un apellido con tilde llegaría mal: «MARTÃNEZ».
   - Hay que poder elegir la codificación por enlace, con ANSI por defecto, y comprobarlo con un aparato real.
2. **Ficheros puente con plantilla.** Hoy solo existe el formato de `IMCONECT.ini`. Algunos fabricantes leen el paciente de un fichero de texto con su propio formato; Dexis, por ejemplo. Por enlace habría que poder fijar:
   - la ruta del fichero;
   - su contenido, con comodines;
   - su codificación.
3. **Formatos de los comodines.**
   - La fecha sale siempre como `dd/mm/aaaa` y el sexo como `H`/`M`.
   - Otros puentes los piden de otra manera: iRYS, por ejemplo, `dd,MM,yyyy` y `M`/`F`.
   - Faltan comodines con formato (`[FECNAC:AAAAMMDD]`, `[SEXO:MF]`…) y uno para el nombre completo.
   - Hay que revisar la coma: `[APELPAC]` y `[NOMPAC]` la dejan pasar, y en algunos puentes rompe los parámetros.
4. **Dos acciones por aparato: «Capturar» y «Ver imágenes».** Muchos programas tienen parámetros distintos para una toma nueva y para abrir las que ya hay. Hoy cada enlace tiene un solo juego de parámetros.
5. **Importación automática y por paciente.**
   - Hoy hay que pulsar «Importar», y solo mira la subcarpeta con el número de historia.
   - Falta vigilar las carpetas de exportación.
   - Falta reconocer al paciente por el nombre del fichero o, en DICOM, por `PatientID` y nombre, avisando si no cuadra con la ficha.
6. **PLY y OBJ.** Se importan como modelos 3D, pero el visor solo abre STL. Los escáneres de Medit y 3Shape exportan PLY, que lleva color.

### Estándares, para no depender de un enlace por fabricante

7. **VDDS-media.**
   - Es el estándar de la asociación alemana de software dental (VDDS e.V.) para intercambiar el paciente y sus imágenes entre el programa de gestión y el de imagen.
   - Lo usan, por ejemplo, Dürr VistaSoft/DBSWIN y 3Shape.
   - Es el que más aparatos cubre de una vez en Europa.
8. **DICOM en red.**
   - **Modality Worklist:** el TAC o el ortopantomógrafo recogen de VeodarDental la lista de pacientes citados, y nadie tiene que teclear el paciente en el aparato.
   - **Storage:** el aparato envía las imágenes a VeodarDental, sin carpetas de por medio.
   - VistaSoft, por ejemplo, admite las dos.
9. **DICOM comprimido** (JPEG, JPEG 2000, JPEG-LS, RLE) en el visor rápido. Hoy se rechaza con un aviso y queda para el visor del fabricante.
10. **TWAIN.** Capturar directamente de un sensor o de un escáner de placas sin abrir el programa del fabricante. Solo en Windows. Le sirve a la clínica que solo tiene sensores.

### Fuera del código

11. **Enlaces probados por fabricante.** Cada enlace de partida necesita probarse con su programa real y apuntar la versión y los parámetros que funcionan. En el mercado español conviene cubrir al menos:
    - Carestream (CS Imaging, CS 3D Imaging), Dentsply Sirona (Sidexis 4), Planmeca (Romexis), Vatech (EzDent-i), Dürr (VistaSoft), Acteon (AIS, Sopro);
    - Owandy (QuickVision), MyRay (iRYS), NewTom, Morita (i-Dixel), Dexis y KaVo;
    - y los escáneres 3Shape, Medit, iTero y Primescan.
12. **Windows.** Los programas de los fabricantes son casi siempre de Windows. En un Mac o un Linux, VeodarDental enseña lo importado pero no puede abrir el programa del aparato.
13. **Escáneres en la nube.** 3Shape, Medit e iTero envían los modelos por sus plataformas de internet. Recibirlos sin pasar por una carpeta necesita su API y un acuerdo con cada uno.

## Orden propuesto

1. Puntos 1 a 6, que son pequeños y abren muchos aparatos con lo que ya hay.
2. VDDS-media (punto 7).
3. DICOM Worklist y Storage (punto 8).
4. En paralelo, probar los enlaces con aparatos reales (punto 11), empezando por los de las primeras clínicas.

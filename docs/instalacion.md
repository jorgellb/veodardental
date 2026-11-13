# Lista de instalación

Para instalar VeodarDental en una clínica: un ordenador hace de servidor y los demás son puestos. Entre «» va lo que se ve en pantalla. Una prueba automática (`docs_tests`) comprueba que cada botón y cada opción que se cita aquí existe en el programa.

## Antes de empezar
- [ ] **Sistema:** Windows 10 u 11, macOS 11 o posterior, o Linux (paquete `.deb` o AppImage).
- [ ] **Disco cifrado en todos los ordenadores:** BitLocker, FileVault o LUKS. Los datos de salud están en el disco, y el programa lo comprueba en «Puesta en marcha».
- [ ] **Red local:** el servidor tiene una dirección fija o un nombre que los puestos encuentran.
- [ ] **Instaladores sin firmar.** Los instaladores todavía no van firmados (B-25): Windows y macOS avisarán al abrirlos.
  - Compruebe que vienen de la versión publicada del repositorio y que su `FIRMA.txt` dice lo que esperaba.
  - No desactive las protecciones del sistema para quitar el aviso.

## 1. El servidor
- [ ] Instale el programa con su instalador.
- [ ] Abra el programa y entre con el usuario de Dirección.
- [ ] Configuración › «Trabajo en red» › «Hacer de este ordenador el servidor».
  - También se puede arrancar solo el servidor: `veodardental --servidor`.
  - Apunte la **clave de la clínica** y la **huella** que enseña (la verán los puestos).
- [ ] Compruebe que abre los datos: `veodardental --comprobar` debe decir la versión, la versión de los datos y cuántos pacientes hay, y terminar con código 0.

## 2. Cada puesto
- [ ] Instale el programa.
- [ ] Configuración › «Trabajo en red» › «Conectar este puesto», con la dirección del servidor y la clave de la clínica.
- [ ] Compruebe que la huella que enseña el puesto es la del servidor. Si alguna vez cambia sin haber reinstalado el servidor, avise: alguien podría hacerse pasar por él.
- [ ] Entre con cada usuario y compruebe que ve lo que le toca («Usuarios y permisos»).

## 3. Copias de seguridad (en el servidor)
- [ ] «Copias de seguridad»:
  - [ ] Escriba la carpeta de la segunda copia (disco USB o carpeta en la nube).
  - [ ] «Cifrar la segunda copia», con una contraseña que se apunta y se guarda fuera del ordenador.
  - [ ] «Guardar la clave de recuperación» en otro sitio, nunca junto a las copias (el programa no lo deja).
  - [ ] «Hacer copia ahora».
  - [ ] «Ensayo de restauración de la última copia»: todas las comprobaciones en verde.

## 4. Datos de la clínica
- [ ] Siga la lista de «Puesta en marcha»: datos de la clínica, profesionales, gabinetes, tarifas, facturación y comunicaciones.
- [ ] Si viene de otro programa, siga `docs/migracion.md`:
  1. «Migración completa desde otro programa».
  2. «Simulacro (no cambia nada)» y revisión del informe.
  3. «Importar lo del simulacro».
- [ ] Cuando todo esté listo, «Empezar con la clínica vacía…» quita los datos de ejemplo.

## 5. Facturación electrónica (VERI*FACTU)
- [ ] El certificado de la clínica, en la configuración de VERI*FACTU.
- [ ] Un solo ordenador envía a la AEAT; el primero que envía queda elegido.
- [ ] Emita una factura de prueba y pulse «Enviar ahora». Si la AEAT rechaza el envío entero, el envío automático se para y el panel dice por qué.

## 6. Comprobación final
- [ ] Desde un puesto: «Nueva cita», «Guardar cita», «Registrar cobro» y «Cerrar caja con este arqueo». Lo que tardan queda en las «Medidas del piloto».
- [ ] Desconecte un momento el servidor. El puesto avisa, y lo que no pudo enviar se guarda en su bandeja y se envía al volver.
- [ ] Anote en el registro de instalación: fecha, versión, ordenadores, quién hizo cada paso y la huella del servidor.

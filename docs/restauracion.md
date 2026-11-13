# Restauración y vuelta atrás

Qué hacer cuando hay que volver a una copia de los datos o a la versión anterior del programa. Entre «» va lo que se ve en pantalla; una prueba automática (`docs_tests`) comprueba que existe.

## Restaurar una copia de los datos
1. En el servidor (o en el único ordenador): Configuración › «Copias de seguridad».
2. Elija la copia en la lista («Restaurar…»). Para una copia guardada en otro sitio, como el USB, escriba su ruta y pulse «Restaurar esa copia».
3. Si va cifrada:
   - escriba la contraseña de las copias;
   - o, sin ella, por ejemplo en otro ordenador, use la clave de recuperación: escriba dónde está el fichero y pulse «Abrir con la clave».
4. El programa enseña qué lleva la copia y qué hay ahora. Al pulsar «Restaurar» guarda antes una copia de los datos actuales, como «antes de restaurar».
5. Vuelva a entrar con su usuario. En red, los puestos recogen los datos al volver a entrar.
6. Queda apuntado en el «Registro de cambios» y en las «Medidas del piloto».

## Comprobar que las copias sirven (cada mes)
- «Ensayo de restauración de la última copia» abre la copia en una carpeta aparte y comprueba, registro a registro, pacientes, citas, tratamientos, cobros, facturas, documentos e imágenes, además del total cobrado y la cadena de facturas.
- No cambia los datos en uso y no deja nada descifrado.
- Si algo sale en rojo, haga otra copia («Hacer copia ahora») y vuelva a ensayar. Si sigue fallando, avise a soporte («Preparar informe para soporte»).

## Si se pierde la contraseña de las copias
- Con la clave de recuperación, las copias cifradas se abren sin contraseña («Abrir con la clave»).
- Sin contraseña y sin clave, las copias cifradas no se pueden abrir: por eso la clave va guardada aparte.

## Actualizar el programa y volver atrás
**Orden: primero el servidor y después los puestos.** Un puesto con otra versión de los datos no escribe en el servidor: lo dice al conectar, y lo que tenga sin enviar queda en su bandeja hasta que se actualice.

1. **Con el paquete firmado de la versión**, que exige la clave de firma de versiones (B-27):
   - `veodardental --actualizar <carpeta del paquete>`;
   - comprueba la firma y cada fichero, hace la copia de los datos, cambia el programa y comprueba que la versión nueva abre los datos (`--comprobar`);
   - si no los abre, vuelve sola a la anterior.
2. **Sin clave de firma:** con el instalador de la versión nueva, después de una copia («Hacer copia ahora»).
3. **Si el ordenador se apaga a mitad:** al arrancar se queda la versión anterior entera. Los datos no se tocan hasta que la nueva arranca.
4. **Volver a la versión anterior a mano:** `veodardental --volver-atras`.
   - Si la versión nueva no cambió la versión de los datos, los datos siguen valiendo.
   - Si la cambió, la anterior los abre sin guardar. Hay que restaurar la copia «antes de actualizar», y se pierde lo hecho desde entonces: el programa lo dice. Valore antes volver a la versión nueva.
5. **Datos de una versión más nueva** que el programa instalado: se abren sin guardar, con el aviso de actualizar. Nunca se pierden sus campos nuevos.

## Lo que hay que apuntar
- Cada restauración y cada vuelta atrás: fecha, por qué, qué copia o versión, quién y el resultado.
- Las medidas se exportan desde «Medidas del piloto».

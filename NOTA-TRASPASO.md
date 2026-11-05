# Nota de traspaso

**Fecha y lugar:** 2026-11-03 22:20, en la PC. Actualizada el 2026-11-04 (Ecuador) desde la nube.
**IA y aparato:** Claude Code. Revisión en la PC; arreglos en la nube.
**Rama:** `main` (versión 0.4.4).

## Hecho en la nube (2026-11-03 y 2026-11-04)

- Arreglados los 4 graves (1 a 4 de abajo) en la 0.2.4, más el aviso `mut` del motor.
- Primeras pruebas del motor: sello en hoja pequeña y guardar/reabrir. En la nube sí se puede compilar el motor (`cargo test`, tras instalar las librerías de GTK/WebKit y generar los íconos con `npx tauri icon assets/icono.png`).
- Se lanzó `compilar.yml` (ambas) desde la nube: sí se puede.
- 0.2.5: "Abrir con" con cambios sin guardar usa un cuadro propio de la app (el aviso de Android congelaba la pantalla). David lo probó: funciona.
- 0.2.6: todas las demás preguntas (abrir otro, quitar todo, salir con Atrás, cerrar en Windows, compartir) usan el mismo cuadro. "Salir" en Android usa `DakartaAndroid.salir()` (nuevo en `scripts/parchar-android.py`).
- 0.2.6 también: Nota, Texto y Sello con el cuadro justo al texto (se mide con la letra Helvetica, como la dibuja MuPDF); se respetan los Enter; letra mínima 8; al agrandar con la esquina el cuadro vuelve a pegarse al texto. Botón "Mover" en la fila de herramientas.
- Arreglado: si la primera línea empezaba con mayúscula con tilde o Ñ (Á, É, Ñ…), MuPDF cortaba la marca de arriba. Ahora el cuadro lleva un borde invisible (trazos de largo cero) que baja el texto lo justo (`poner_borde` en `motor.rs`).
- David probó la 0.2.6: todo bien. Con el PDF de prueba vio el formulario doble (la casilla repetía el valor que ya dibuja la página). 0.2.7: la casilla queda solo como marco.
- 0.2.8: recuadro "Dakarta · versión y descargas" (`src/AcercaDe.tsx`): en el inicio y al final del panel ☰. Versión instalada, si hay nueva, enlaces fijos del APK y .exe (copiar/descargar) y página de descargas. Sin versión web (Dakarta no tiene).
- 0.2.9: los enlaces internos del PDF (el índice impreso de Word) se pueden tocar para saltar (`enlaces_de` en `motor.rs`). Las capas de la hoja ya no pasan por encima del panel ☰ (`isolation` en `.visor`). La manija izquierda de la selección: margen de recorte en `.texto-pdf` (por comprobar en el celular).
- 0.2.10: hallazgos 5 a 10 arreglados (ver lista) y mensaje más claro cuando el PDF no trae índice lateral. Pruebas del motor: 16.
- 0.2.11: en un grupo de opciones (F/M), al elegir una las demás seguían viéndose marcadas (MuPDF solo cambia la que se toca). `apagar_las_demas` en `motor.rs`.
- 0.2.12: hallazgos 11 y 14 cerrados. La Nota es un texto con fondo amarillo tipo post-it (`postit` en `anotar_caja`); Texto va sin fondo. Se quitó `anotar_nota` (sin uso). En Windows se muestra el nombre elegido al guardar. David confirmó las tres decisiones de `AGENTS.md` y se quitó de `CLAUDE.md` la sección "Revisar lo que hizo otra IA" (revisión terminada). Pruebas del motor: 17.
- 0.2.13: al editar el texto de una nota, texto o sello, la hoja se actualiza mientras se escribe (en cada pausa de 350 ms), no solo al tocar fuera. Lo escrito de un tirón es un solo paso de deshacer (`junto` en `editar_anotacion`). Pruebas del motor: 18.
- 0.2.14: con la escritura en vivo, una N sola se volvía Ñ (el teclado cerraba la palabra a medias y la corregía). El panel espera a que el teclado termine la palabra (`compositionend`) y el cuadro va con `autoCorrect="off"`. Por comprobar en el celular.
- 0.3.0 (Fase 3, primera parte): «Organizar páginas» desde el panel ☰: mover (arrastrar o ◀ ▶), girar, quitar, duplicar, extraer, deshacer y guardar copia (se abre la copia organizada). El motor reordena una copia del PDF abierto con sus anotaciones. Pruebas: 21 del motor y 17 de la pantalla.
- 0.4.0 (Fase 4, primera parte): se abren para leer Word/Excel/PowerPoint, EPUB, MOBI, FB2, TXT, HTML, XPS, CBZ e imágenes (MuPDF con `html, epub, xps, cbz, img`). Anotar y organizar solo en PDF. "Abrir con" en Android y Windows para esos formatos (no para fotos). Pruebas: 23 del motor.
- 0.4.1: desde Gemini o WhatsApp no salía Dakarta para Word: esas apps entregan los .docx como `application/msword` u `application/octet-stream`. Se registran también esas etiquetas; el motor mira el contenido real. Office antiguo (.doc/.xls/.ppt) da un mensaje claro.
- 0.4.2: la marca de «abriendo» se quita al verse la primera hoja o al salir de la app (antes a los 5 s: si se salía antes, la vez siguiente no reabría y salía el aviso, alternando). Inicio: últimos 3 de cada tipo (PDF, Word, Excel, PowerPoint, Libros, Texto, Imágenes, Otros); se recuerdan 30. Nombres sacados del enlace se muestran limpios (`nombreLegible`).
- 0.4.3: lector de Word propio (`motor/word.rs`, con `zip` y `roxmltree`): tamaño de hoja y márgenes del documento, alineación, negritas/cursivas/subrayado/color, viñetas y listas numeradas, tablas con columnas (bordes si los tiene), saltos de página (como capítulos de un EPUB en memoria). Panel ☰: últimos archivos también por tipo. Pendiente: imágenes dentro del Word, encabezados/pies, cuadros de texto; Excel y PowerPoint con la misma idea.
- 0.4.4: David descartó la Fase 4. Se quitó todo lo de otros formatos (lector de Word, lectores extra de MuPDF, `zip`/`roxmltree`, tipos de archivo, «Abrir con» de otros formatos, archivos de prueba) y la separación de últimos por tipo. Se quedan: reabrir sin alternar y nombres limpios.
- Falta que David pruebe en el celular: "Abrir con" con cambios sin guardar debe preguntar; en Windows, guardar encima del PDF abierto debe avisar.

## Qué estaba haciendo

Revisé lo que hizo DeepSeek desde la marca `fin-fase-1-claude` (Fase 2) y, con el OK de David, arreglé los hallazgos 1 a 11 y 14, más los ajustes que él pidió al probar en el celular. La revisión quedó terminada.

## Lo que se comprobó

- Revisión leyendo el código completo. La app no se ejecutó ni se probó en un aparato; lo marcado "por comprobar" no está confirmado.
- La última compilación (Android y Windows) terminó bien y publicó la 0.2.3.
- Las pruebas pasan: 13 de pantalla y 5 de la memoria de páginas. El motor no tiene pruebas propias.
- No se encontraron claves en lo agregado, la marca no se movió y `compilar.yml` no cambió.

## Hallazgos, de más a menos grave

**Pueden perder o dañar trabajo**
1. ✅ (0.2.4) Windows: "Guardar copia" deja elegir el mismo PDF que está abierto, y eso puede dañar el original (`src-tauri/src/lib.rs:196`, `src-tauri/src/motor.rs:1439`). Por comprobar. Arreglo: rechazar ese destino; guardar primero en un archivo temporal y luego reemplazar.
2. ✅ (0.2.4) Android: un PDF que llega por "Abrir con" reemplaza al abierto y pierde los cambios sin guardar, sin avisar (`src/App.tsx:1118-1125`). Los botones sí preguntan con `puedeCambiar`.
3. ✅ (0.2.4) Al arrancar se reabre el último PDF (`src/App.tsx:1129-1131`). Si ese PDF cierra la app, por ejemplo por falta de memoria, la app se cerraría en cada arranque. Arreglo: marcar "abriendo" antes de abrir y no reabrir solo si la marca sigue puesta.
4. ✅ (0.2.4) Con una hoja muy pequeña, poner un sello detiene el motor hasta reiniciar la app: `clamp` falla si el mínimo supera al máximo (`src-tauri/src/motor.rs:975-976`). El commit `23493c5` lo corrigió en otros lugares, no aquí. Arreglo de una línea: `.min().max()`.

**Funcionan mal en algunos casos**
5. ✅ (0.2.10) Mover o editar una anotación la vuelve a crear con pocos datos. Un sello pierde el centrado al moverlo. Las anotaciones hechas en otros programas pierden su letra y autor (`src-tauri/src/motor.rs:1150-1207`).
6. ✅ (0.2.10) Formularios: las casillas usan siempre "Yes" y "Off", pero cada PDF puede usar otro valor; los botones del formulario también entran ahí (`src/App.tsx:862`). Por comprobar con formularios reales.
7. ✅ (0.2.10) Android: si falla la copia al destino, queda un archivo incompleto. Además `copia.pdf` se queda en la caché, y puede pesar tanto como el PDF (`scripts/parchar-android.py:169`, `src-tauri/src/lib.rs:187`).
8. ✅ (0.2.10) Lo borrado podría seguir escondido dentro de la copia guardada, porque no se activa la limpieza al guardar (`src-tauri/src/motor.rs:1439`). Por comprobar antes de confiar en ello con datos delicados.
9. ✅ (0.2.10) Buscar en un PDF enorme detiene el dibujo de páginas hasta terminar, y cerrar la búsqueda no la cancela (`src-tauri/src/motor.rs:726`).
10. ✅ (0.2.10) Si falla un deshacer, ese paso se pierde y el contador queda mal (`src-tauri/src/motor.rs:1346`). Cerrar el panel tras cambiar un texto puede crear dos pasos de deshacer (`src/App.tsx:2314`, `src/App.tsx:2337`).

**Orden y detalles**
11. ✅ (0.2.12) Código sin uso: `anotar_nota` (`src-tauri/src/lib.rs:70`, `src-tauri/src/motor.rs:887`). Los botones Nota y Texto hacen lo mismo (`src/App.tsx:794`).
12. El motor tiene 17 pruebas (0.2.12); seguir agregando en lo que se toque, aunque maneja lo que se guarda. David pide pruebas en todo lo que transforma datos.
13. ✅ (junto con la 0.2.14, por pedido de David) Separados sin cambiar lo que hace: el motor en `motor/` y la pantalla en `Paginas.tsx`, `Paneles.tsx`, `tipos.ts` y `opciones.ts`. Antes: `src/App.tsx` (2.360 líneas) y `src-tauri/src/motor.rs` (1.500) quedaron muy grandes y con bloques repetidos. Conviene separarlos antes de la Fase 3.
14. ✅ (0.2.6 y 0.2.12) El cuadro de texto ignora los Enter y cuenta mal las tildes al cortar líneas (`src-tauri/src/motor.rs:988-1005`). En Windows, tras guardar se muestra el nombre sugerido, no el elegido (`src/App.tsx:1038`).
15. El motor ahora solo abre PDF (`PdfDocument`): la Fase 4 (otros formatos) tendrá que ajustarlo.

## Qué conviene conservar

Búsqueda, últimos archivos, compartir, selector de Android con permiso guardable, gesto Atrás, avisos de cambios sin guardar en botones y cierre, deshacer, caja de firma calculada desde sus trazos, repintado por página. El trabajo de la Fase 1 (zoom, recortes, prioridades) sigue intacto.

## Qué falta, en orden

1. David prueba la 0.2.12 en el celular: Nota amarilla, formulario (F/M y casillas), mover un sello, buscar en un PDF grande, guardar copia, tocar el índice impreso y las dos manijas de selección.
2. ✅ Separados `App.tsx` y `motor.rs` (hallazgo 13), junto con la 0.2.14.
3. Fase 3: organizar un PDF ✅ (0.3.0). Unir varios PDFs (0.3.1) quedó pendiente porque David adelantó la Fase 4.
4. Fase 4: descartada por David (2026-11-05); quitada en la 0.4.4.

## Pendiente para el celular

Desde la nube no se sube directo a `main`: se publica con un pull request y se une con merge. `compilar.yml` sí se puede lanzar desde la nube.

## Pendiente para la PC

Nada.

## Aprendizajes para pasar a la fuente única

- Cuando una revisión automática de la nube pida subir y David no ha dicho "sube", no se sube: manda su regla.

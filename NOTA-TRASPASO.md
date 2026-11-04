# Nota de traspaso

**Fecha y lugar:** 2026-11-03 22:20, en la PC.
**IA y aparato:** Claude Code, PC. David sigue con Claude en el celular.
**Rama:** `main`, commit `23493c5` (versión 0.2.3). El código no se cambió en esta revisión.

## Qué estaba haciendo

Revisé lo que hizo DeepSeek desde la marca `fin-fase-1-claude` (Fase 2 de edición, 17 commits, unas 3.700 líneas). Le di el resumen a David. **Hay que esperar su OK antes de cambiar código.** Él debe decir qué se corrige.

## Lo que se comprobó

- Revisión leyendo el código completo. La app no se ejecutó ni se probó en un aparato; lo marcado "por comprobar" no está confirmado.
- La última compilación (Android y Windows) terminó bien y publicó la 0.2.3.
- Las pruebas pasan: 13 de pantalla y 5 de la memoria de páginas. El motor no tiene pruebas propias.
- No se encontraron claves en lo agregado, la marca no se movió y `compilar.yml` no cambió.

## Hallazgos, de más a menos grave

**Pueden perder o dañar trabajo**
1. Windows: "Guardar copia" deja elegir el mismo PDF que está abierto, y eso puede dañar el original (`src-tauri/src/lib.rs:196`, `src-tauri/src/motor.rs:1439`). Por comprobar. Arreglo: rechazar ese destino; guardar primero en un archivo temporal y luego reemplazar.
2. Android: un PDF que llega por "Abrir con" reemplaza al abierto y pierde los cambios sin guardar, sin avisar (`src/App.tsx:1118-1125`). Los botones sí preguntan con `puedeCambiar`.
3. Al arrancar se reabre el último PDF (`src/App.tsx:1129-1131`). Si ese PDF cierra la app, por ejemplo por falta de memoria, la app se cerraría en cada arranque. Arreglo: marcar "abriendo" antes de abrir y no reabrir solo si la marca sigue puesta.
4. Con una hoja muy pequeña, poner un sello detiene el motor hasta reiniciar la app: `clamp` falla si el mínimo supera al máximo (`src-tauri/src/motor.rs:975-976`). El commit `23493c5` lo corrigió en otros lugares, no aquí. Arreglo de una línea: `.min().max()`.

**Funcionan mal en algunos casos**
5. Mover o editar una anotación la vuelve a crear con pocos datos. Un sello pierde el centrado al moverlo. Las anotaciones hechas en otros programas pierden su letra y autor (`src-tauri/src/motor.rs:1150-1207`).
6. Formularios: las casillas usan siempre "Yes" y "Off", pero cada PDF puede usar otro valor; los botones del formulario también entran ahí (`src/App.tsx:862`). Por comprobar con formularios reales.
7. Android: si falla la copia al destino, queda un archivo incompleto. Además `copia.pdf` se queda en la caché, y puede pesar tanto como el PDF (`scripts/parchar-android.py:169`, `src-tauri/src/lib.rs:187`).
8. Lo borrado podría seguir escondido dentro de la copia guardada, porque no se activa la limpieza al guardar (`src-tauri/src/motor.rs:1439`). Por comprobar antes de confiar en ello con datos delicados.
9. Buscar en un PDF enorme detiene el dibujo de páginas hasta terminar, y cerrar la búsqueda no la cancela (`src-tauri/src/motor.rs:726`).
10. Si falla un deshacer, ese paso se pierde y el contador queda mal (`src-tauri/src/motor.rs:1346`). Cerrar el panel tras cambiar un texto puede crear dos pasos de deshacer (`src/App.tsx:2314`, `src/App.tsx:2337`).

**Orden y detalles**
11. Código sin uso: `anotar_nota` (`src-tauri/src/lib.rs:70`, `src-tauri/src/motor.rs:887`). Los botones Nota y Texto hacen lo mismo (`src/App.tsx:794`).
12. El motor no tiene pruebas, aunque maneja lo que se guarda. David pide pruebas en todo lo que transforma datos.
13. `src/App.tsx` (2.360 líneas) y `src-tauri/src/motor.rs` (1.500) quedaron muy grandes y con bloques repetidos. Conviene separarlos antes de la Fase 3.
14. El cuadro de texto ignora los Enter y cuenta mal las tildes al cortar líneas (`src-tauri/src/motor.rs:988-1005`). En Windows, tras guardar se muestra el nombre sugerido, no el elegido (`src/App.tsx:1038`).
15. El motor ahora solo abre PDF (`PdfDocument`): la Fase 4 (otros formatos) tendrá que ajustarlo.

## Qué conviene conservar

Búsqueda, últimos archivos, compartir, selector de Android con permiso guardable, gesto Atrás, avisos de cambios sin guardar en botones y cierre, deshacer, caja de firma calculada desde sus trazos, repintado por página. El trabajo de la Fase 1 (zoom, recortes, prioridades) sigue intacto.

## Qué falta, en orden

1. David da su OK sobre qué corregir. Propuesta: 1 a 4 primero, luego 5 a 10 y una prueba del ciclo guardar y volver a abrir; el orden del código, al final.
2. David confirma tres cosas que `AGENTS.md` le atribuye y no se pudieron comprobar: que decidió no hacer la firma digital, que dio por hechos los tiempos y la prueba de 1000 hojas, y que acepta quitar "corregir palabras o frases cortas" del plan.
3. David prueba la 0.2.3 en el celular y detalla los ajustes que mencionó. Según su nota anterior, resaltar, subrayar y tachar ya funcionan: no tocarlos sin motivo.
4. Con la revisión aceptada, quitar de `CLAUDE.md` la sección "Revisar lo que hizo otra IA".

## Pendiente para el celular

Desde la nube no se sube directo a `main`: se publica con un pull request y se une con merge. Comprobar en la sesión si se puede lanzar `compilar.yml`; si no se puede, dejarlo como pendiente para la PC.

## Aprendizajes para pasar a la fuente única

Ninguno todavía.

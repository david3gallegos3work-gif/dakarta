# Dakarta — Lector de PDF sin anuncios

> Este archivo resume todo lo decidido antes de empezar a programar.
> Cualquier asistente de IA que trabaje en este proyecto (Claude, DeepSeek, etc.) debe leerlo completo antes de hacer cambios.

## Sobre el dueño del proyecto

- Se llama **David**. **No es programador**: explícale todo en **español sencillo**, sin jerga, y paso a paso.
- Ya hizo antes una app con este mismo método: **Datheke** (bloc de notas), repositorio `datheke/datheke.github.io`. Úsala como referencia para la estructura, la compilación automática y la integración con Gemini.
- Prefiere que la IA haga el trabajo técnico y que él solo pruebe en su celular y su PC.

## Objetivo

Un lector de PDF **gratis, sin anuncios, sin rastreo y sin cobros**, para uso de David y su familia, con la mayor cantidad posible de funciones de Adobe Acrobat.

- **Plataformas:** solo **Android (APK)** y **Windows (.exe)**. **No** hay versión web por ahora (si David la pide en el futuro, se agrega).
- **Costo:** $0. No se publica en Play Store. Se instala el APK directamente.
- **Idioma de la app:** español.
- **Repositorio:** `github.com/david3gallegos3work-gif/dakarta` (público).
- **Licencia:** AGPL-3.0 (obligatoria por usar MuPDF). El código debe quedar público.

## Arquitectura decidida

| Parte | Tecnología | Por qué |
|---|---|---|
| Pantalla (interfaz) | **React + TypeScript** (Vite) | Igual que Datheke; David ya conoce el flujo |
| Empaquetado | **Tauri 2** (Android + Windows) | Un solo código para las dos plataformas |
| Motor PDF | **MuPDF nativo** (no la versión web/WASM) | Abre PDFs de 1000+ páginas y escaneados de cientos de MB sin cargar todo en memoria |
| Compilación | **GitHub Actions** | David no instala Android Studio; el APK y el .exe se compilan en la nube |

### Motor nativo: detalle importante

- El motor **no** debe correr en el WebView (nada de mupdf.js/WASM): con PDFs escaneados grandes el celular se queda sin memoria.
- **Opción A (preferida):** MuPDF desde el lado Rust de Tauri (crate `mupdf`, bindings de MuPDF para Rust), compilado para Windows y para Android con el NDK.
- **Opción B (si la A no compila bien en Android):** un plugin móvil de Tauri en Kotlin que use la librería oficial `mupdf-android-fitz` (Artifex), y la Opción A solo en Windows.
- Verificar cuál funciona **en la Fase 1**, antes de construir lo demás.
- Renderizar **solo las páginas visibles** (carga perezosa), con caché de páginas cercanas.
- Enviar las imágenes de página a la interfaz por un protocolo propio de Tauri (no como base64 por IPC, que es lento).
- Abrir archivos de forma que no se lea todo el archivo a la memoria.

### Integración con el sistema

- En Android, la app debe aparecer en **"Abrir con"** para archivos PDF (desde WhatsApp, Gmail, el gestor de archivos, etc.).
- En Windows, poder asociarse a los archivos .pdf.
- Aviso de actualización dentro de la app, igual que en Datheke (busca el APK/.exe nuevo en las Releases de GitHub).

## Estado actual

**Fase 1 terminada y probada por David** (versión 0.1.2, noviembre de 2026). En git, la marca `fin-fase-1-claude` señala el punto exacto donde quedó. No la muevas ni la borres: David la usará para revisar lo que se haga después. Después llegaron buscar texto, últimos archivos y la reapertura del último PDF (0.1.3), y el selector de Android con permiso guardable, los gestos y "Atrás" (0.1.4).

**Lo que ya funciona (Android y Windows):** abrir PDF sin cargarlo entero, pasar páginas, zoom (dos dedos, Ctrl + rueda, botones, doble toque normal/mitad), índice, buscar texto, últimos archivos, reabrir el último, modo noche, "Abrir con Dakarta" / lector predeterminado y aviso de versión nueva.

**Motor:** la Opción A (crate `mupdf` en Rust) compila y funciona en Android, así que la Opción B queda descartada.

**Cómo está armado:**
- `src-tauri/src/motor.rs`: un solo hilo tiene el documento de MuPDF y atiende los pedidos; primero lo más reciente. Entrega JPEG por el protocolo `pagina://`.
- `src-tauri/src/cache.rs`: guarda las últimas hojas pintadas, con tope de memoria.
- `src-tauri/src/lib.rs`: elegir y abrir archivos. En Android el `content://` se abre por su descriptor (`/proc/self/fd`), sin copiarlo; solo se copia si no se puede leer así.
- `src/App.tsx`: cada página tiene capas: miniatura (se queda al pasar), hoja entera (hasta 2048 px) y, con más zoom, un recorte de lo visible.
- `src/zoom.ts`: cálculos del zoom y los recortes. `src/version.ts`: aviso de versión nueva.
- `scripts/parchar-android.py`: agrega "Abrir con" al proyecto Android, que se genera en cada compilación.

**Cómo compilar y publicar:**
- David no tiene Rust en su PC, y la PC tiene poca memoria: no compilar Rust allí ni instalar Rust ni Android Studio.
- Antes de publicar, subir el número de versión en `src-tauri/tauri.conf.json` y `src-tauri/Cargo.toml`. Con el número viejo, los archivos reemplazan a los de esa versión y la app instalada no avisa de nada.
- Compilar con `gh workflow run compilar.yml -f plataforma=ambas`. El APK se firma con los secretos de GitHub que ya existen; no crear otra clave de firma, porque las actualizaciones dejarían de instalarse encima.
- Pruebas: `npm test` y `rustc --edition 2024 --test src-tauri/src/cache.rs`. Las dos corren en cada compilación.

**Cómo trabajar con David:**
- Hablarle en español de Ecuador, con "tú" y palabras simples. Nunca voseo, tampoco en los textos de la app.
- Subir a GitHub solo cuando diga "sube".
- `git add` archivo por archivo, nunca `-A` ni `.`.
- Un commit por cada cosa terminada, con un mensaje que diga qué se hizo.
- Decir con claridad qué se probó y qué no. Que compile no significa que funcione: lo prueba David en su celular y su PC.

**Pendiente de la Fase 1:**
- Tiempos del PDF de 500 MB y escaneado de 1000 hojas: David los dio por hechos el 2026-11-02 (confirmado el 2026-11-04).
- El letrero de tiempos se quitó en la 0.1.4.
- Probado en el celular el 2026-11-02 (0.1.5): reabrir el último PDF, doble toque, deslizar el panel, "Atrás" y los avisos funcionaron.

## Pedidos nuevos de David (siguiente etapa)

De estos cuatro, ya están el 1, el 3 y el 4. Falta el 2.

1. **Buscar texto en el documento** (hecho en 0.1.3). `Page::search` de la crate `mupdf` devuelve dónde está cada resultado. Los escaneados no tienen texto: se avisa con un mensaje claro hasta que exista el OCR de la Fase 6.
2. **Compartir el archivo abierto desde la app.** Android: menú de compartir del sistema. Windows: David eligió mostrar el archivo en el Explorador con el PDF seleccionado (desde ahí lo envía).
3. **Al abrir la app, volver a abrir el último archivo** (hecho en 0.1.3).
4. **Lista de los últimos archivos abiertos** (hecho en 0.1.3).

**Cuidado con 3 y 4 en Android:** el permiso para leer un `content://` puede ser temporal. Desde la 0.1.4 el selector de Android ("Abrir") guarda el permiso para siempre; los archivos que llegan con "Abrir con" desde WhatsApp o Gmail a veces no lo permiten, y en ese caso se muestra un aviso claro y se quitan de la lista. No copiar los PDF pesados a la app sin preguntarle a David, porque su PDF pesa 500 MB; quedó pendiente decidir si se copian los de WhatsApp/Gmail para que también se reabran.

## Plan por etapas

1. **Fase 1 — Prueba de velocidad:** abrir PDFs, pasar páginas, zoom, índice, modo noche. Medir tiempos con el PDF más pesado de David (idealmente un escaneado de 1000 hojas). Si va lento, corregir aquí antes de seguir.
2. **Fase 2 — Anotar y firmar (0.2.3):** resaltar, subrayar, tachar, dibujar a mano, notas, cuadros de texto, sellos, rellenar formularios y firma dibujada. Se corrigió el error de fondo de las anotaciones FreeText, se agregaron paletas de color y tamaño por herramienta, y ahora una anotación se toca y se arrastra para moverla (desde la 0.2.6 hay un botón "Mover" visible para entrar a ese modo), cambiar su texto, color o tamaño, o borrarla; Deshacer cubre esos cambios. Resaltar/subrayar/tachar eligen solo las palabras tocadas; subrayar y tachar no tienen grosor propio porque el PDF lo calcula del alto de la letra. En la 0.2.2 el color de texto ya no rellena el cuadro (era la mancha negra/verde), la nota es un texto visible (desde la 0.2.12, con fondo amarillo tipo post-it; "Texto" va sin fondo), los recuadros de toque son invisibles salvo la anotación elegida y se puede hacer zoom con dos dedos mientras se marca. El menú lateral tapa la barra de edición y se puede seleccionar y copiar el texto de PDFs que ya tienen texto (los escaneos necesitan OCR). La firma digital con certificado quedó descartada por David (2026-11-02). En la 0.2.3 el cuadro de nota, texto y sello mide según lo escrito; se agrandan y achican arrastrando una esquina (la firma y el lápiz en proporción); el color y el tamaño se aplican al tocarlos (sin botón "Aplicar") y el texto se guarda al salir del cuadro; la firma y los trazos del lápiz se pueden tocar, mover, cambiar y borrar, porque MuPDF no entrega el Rect de las tintas y la caja ahora sale de los trazos. Pendiente: probar en el celular.
3. **Fase 3 — Unir y organizar páginas** (ver detalle abajo).
4. **Fase 4 — Otros formatos:** descartada por David (2026-11-05). Se hizo en la 0.4.0–0.4.3 (Word, EPUB, TXT…) y se quitó del código en la 0.4.4: Dakarta es solo lector de PDF.
5. **Fase 5 — Gemini** (ver detalle abajo).
6. **Fase 6 — Extras:** OCR sin conexión, buscar texto en todo el documento, proteger con contraseña (hecho en la 0.4.7: abrir protegidos, poner, cambiar o quitar la contraseña en una copia), comprimir, crear PDF desde fotos/cámara (hecho en la 0.4.9), leer en voz alta (TTS de Android/Windows), convertir PDF a texto/Word (formato aproximado).

## Funciones de Adobe que NO se van a hacer (y por qué)

- **Asistente de IA de Adobe:** se reemplaza por Gemini con la clave gratuita del usuario.
- **Adobe Document Cloud / "enviar para firmar" con seguimiento:** depende de servidores de Adobe.
- **Word/Excel/PowerPoint → PDF con fidelidad perfecta:** requeriría LibreOffice, demasiado pesado.
- **Firmas con confianza AATL de Adobe:** requieren certificados de pago.
- **Firma digital con certificado:** David decidió no hacerla (2026-11-02, confirmado el 2026-11-04); solo firma dibujada.
- **Corregir palabras o frases cortas del texto original del PDF:** quitado del plan; David lo confirmó el 2026-11-04. El texto original no se cambia: se anota encima.

## Edición limitada: reglas obligatorias de interfaz

La idea es **limitar de verdad**, no solo avisar. El usuario no debe encontrar botones que no funcionan, y debe entender por qué.

### 1. Aviso al entrar en modo edición (una vez por archivo, con casilla "Entendido, no volver a mostrar")

- **DOCX:** «Modo edición sencilla: puedes cambiar texto, negritas, cursivas, subrayado, títulos y listas. Las tablas, imágenes y el diseño original se conservan, pero no se pueden modificar aquí.»
- **PDF:** «Edición básica de PDF: puedes resaltar, subrayar, tachar, dibujar a mano, escribir notas y cuadros de texto, firmar dibujando y rellenar formularios. No se puede cambiar el texto original, las imágenes ni el diseño de la página. Al guardar se crea una copia; el archivo original no se toca.» El aviso en la app muestra solo las herramientas que ya existen en esa entrega.

### 2. La barra de herramientas solo muestra lo permitido

- **DOCX:** negrita, cursiva, subrayado, títulos, viñetas, listas numeradas, deshacer/rehacer. Nada más.
- **PDF:** resaltar, subrayar, tachar, dibujar, nota, cuadro de texto, firma dibujada, formulario y sello.
- Botón **ⓘ «¿Qué puedo editar?»** con la lista de lo que se puede y lo que no.

### 3. Bloqueo con explicación al intentar algo no permitido

- Tocar una tabla o imagen en DOCX: «Esta parte del documento está protegida para no dañar su formato.»
- Si una herramienta todavía no existe en esa entrega, no mostrarla: el usuario no debe encontrar botones que no funcionan.

### 4. Guardar

- **"Guardar como copia"** es la opción por defecto. El original nunca se daña.

## Unir PDFs y organizar páginas (Fase 3)

**Unir:**
1. El usuario elige varios PDFs.
2. Pantalla con **todas las páginas en miniatura**, con un color distinto por archivo de origen.
3. **Arrastrar** cualquier página a cualquier posición (se puede intercalar páginas de distintos archivos).
4. **Eliminar** y **rotar** páginas.
5. Botón **«Ordenar por archivo»** para volver al orden simple.
6. Botón **«Unir»** → crea un PDF nuevo; los originales quedan intactos.

**Organizar un solo PDF:** la misma pantalla para mover, eliminar, rotar, duplicar y extraer páginas a un PDF aparte, con **deshacer**.

**Estado:** organizar un solo PDF hecho en la 0.3.0 (panel ☰ → «Organizar páginas»; `src/Organizar.tsx` y `src-tauri/src/motor/organizar.rs`). Se trabaja sobre una copia del PDF abierto que ya lleva las anotaciones sin guardar; las hojas repetidas llevan copia de sus anotaciones pero no de las casillas de formulario. Unir varios PDFs hecho en la 0.4.5 (inicio → «Unir PDFs», o panel ☰ → «Unir con otros PDFs»): misma pantalla con un color por archivo, «Agregar PDF» y «Ordenar por archivo». El motor parte de una copia del primero y agrega las hojas de los demás con sus anotaciones (las casillas de formulario solo del primero).

Las miniaturas de documentos grandes se generan poco a poco (no bloquear la pantalla).

## Gemini (Fase 5)

Reutilizar el sistema de **Datheke** (`datheke/datheke.github.io`, módulo de Gemini en `src/`):

- Vincular: botón «Abrir Google» → el usuario crea su clave en Google AI Studio → vuelve y toca «Pegar». La app **verifica la clave con Google antes de guardarla**.
- Mismos mensajes de error claros: límite gratis agotado (429), clave no aceptada, Gemini ocupado, sin conexión, no disponible en el país.
- Probar varios modelos en orden y recordar el que funcionó.
- La clave se guarda **cifrada en el dispositivo**. Nunca en el código ni en GitHub.
- Nota visible: «En el plan gratis, Google puede revisar lo que le envíes. No lo uses con documentos delicados (bancarios, médicos, contraseñas).»

**Funciones:** resumir (todo o páginas elegidas), preguntarle al PDF, explicar en palabras sencillas un texto seleccionado, traducir, sacar puntos clave, crear preguntas de estudio. En DOCX/TXT: corregir ortografía y mejorar redacción.

Gemini lee PDFs directamente (también escaneados). Con documentos largos, **dividir en partes automáticamente** para no agotar el límite gratis. Todo lo demás de la app funciona **sin internet**; solo Gemini necesita conexión.

## Logo

- Archivo: `DaKarta.jpg` (David lo tiene en su carpeta Descargas). Copiarlo al proyecto como `assets/logo.jpg`.
- Diseño: una "D" azul estilizada como hoja de documento con la esquina doblada, texto "DAKARTA" y "PDF" debajo.
- Para el ícono de la app usar **solo el símbolo de la "D"** (el texto no se lee en un ícono pequeño). Preparar una versión PNG cuadrada de 1024×1024 y generar los íconos con `tauri icon`.
- El JPG tiene fondo claro; si hace falta, recortar o pedir a David una versión con fondo transparente.

## Reglas para la IA que trabaje aquí

- Explicar a David en español sencillo qué se hizo y qué debe probar.
- Nunca subir claves, contraseñas, keystores ni archivos `client_secret*.json` al repositorio. Las claves de firma van en **GitHub Secrets**.
- Compilar siempre con GitHub Actions; publicar el APK y el .exe en **Releases** con un enlace que se descargue directo en el celular.
- No avanzar a la siguiente fase sin que David haya probado la anterior.
- Mantener el aviso de licencia AGPL-3.0 y los créditos de MuPDF (Artifex Software).

# Dakarta
Lector de PDF sin anuncios para Android (APK) y Windows (.exe): React + TypeScript (Vite) + Tauri 2 + MuPDF nativo (crate `mupdf`).
Las reglas generales están en `~/.claude/CLAUDE.md`; aquí solo lo de este proyecto. El plan completo y las decisiones están en `AGENTS.md` (leerlo antes de cambiar algo).

## Cuentas (no mezclar con otros proyectos)
- GitHub: `david3gallegos3work-gif`, repo `david3gallegos3work-gif/dakarta`. **Es público**: nada privado aquí.
- Vercel: no usa. Supabase: no usa.
- Firma del APK: secretos `ANDROID_KEYSTORE` y `ANDROID_KEYSTORE_PASSWORD` en GitHub; la clave vive en `C:\Users\david\.credenciales\dakarta\` (no la abras).
- "sube" = commit + push. No hay despliegue: la app se compila en GitHub Actions.

## Compilar y verificar
- No hay Rust en la PC de David: todo se compila en la nube. `gh workflow run compilar.yml -f plataforma=ambas|android|windows`, luego `gh run watch`.
- El APK y el .exe quedan en Releases: `https://github.com/david3gallegos3work-gif/dakarta/releases/latest/download/Dakarta.apk` y `.../Dakarta-Windows.exe`.
- Verificar antes de decir "listo": `npm test` y `npm run build` (si hay RAM), y que la compilación de Actions termine en verde.
- La PC tiene poca RAM: nada de `tauri dev` ni compilar Rust localmente.

## Cómo está armado
- `src-tauri/src/motor.rs`: un solo hilo con MuPDF; pinta páginas a JPEG y las entrega por el protocolo `pagina://` (no base64 por IPC). Sus partes están en `motor/`: `anotar`, `formulario`, `busqueda`, `capas` (palabras y enlaces), `organizar` (mover, girar, quitar o duplicar hojas en una copia, y unir varios PDFs), `pintar` (y guardar) y `pruebas` (`cargo test`).
- `src-tauri/src/lib.rs`: elegir y abrir el archivo (en Android el content:// se abre por su descriptor, sin copiarlo).
- `src/App.tsx`: solo pide al motor las páginas cercanas a la pantalla; zoom con dos dedos, Ctrl+rueda o botones. Las hojas y sus capas están en `src/Paginas.tsx`; el panel de anotación, paletas e índice en `src/Paneles.tsx`; tipos en `src/tipos.ts`; colores y tamaños en `src/opciones.ts`; la pantalla de organizar páginas en `src/Organizar.tsx` (su lógica en `src/organizar.ts`).
- Cada página tiene capas: miniatura (se queda), hoja entera (hasta 2048 px) y, con más zoom, un recorte de lo visible (`src/zoom.ts`).
- Releases: cada archivo va con número (para el aviso de la app, `src/version.ts`) y sin número (enlace fijo). `scripts/parchar-android.py` agrega "Abrir con".

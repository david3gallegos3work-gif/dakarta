mod cache;
mod motor;

use std::sync::Mutex;

use motor::{Busqueda, Info, Motor};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_opener::OpenerExt;

/// Windows: el PDF con el que se abrió la app (doble clic o "Abrir con"). Se entrega una sola vez.
struct Inicial(Mutex<Option<String>>);

#[tauri::command]
fn archivo_inicial(inicial: State<'_, Inicial>) -> Option<String> {
    inicial.0.lock().ok()?.take()
}

/// "Abrir con Dakarta": en Windows llega una ruta; en Android, un content://.
#[tauri::command]
async fn abrir_ruta(app: AppHandle, motor: State<'_, Motor>, ruta: String) -> Result<Info, String> {
    let ruta = ruta.parse::<FilePath>().map_err(|e| e.to_string())?;
    abrir(&app, &motor, ruta)
}

/// Abre en el navegador la descarga de una versión nueva; solo las de Dakarta.
#[tauri::command]
fn descargar_version(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/david3gallegos3work-gif/dakarta/releases/") {
        return Err("Enlace de descarga no válido.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn buscar(motor: State<'_, Motor>, texto: String) -> Result<Busqueda, String> {
    motor.buscar(texto)
}

#[tauri::command]
async fn elegir_pdf(app: AppHandle, motor: State<'_, Motor>) -> Result<Option<Info>, String> {
    let Some(ruta) = app.dialog().file().add_filter("PDF", &["pdf"]).blocking_pick_file() else {
        return Ok(None);
    };
    abrir(&app, &motor, ruta).map(Some)
}

/// Windows: la ruta del archivo. Android: el content:// se abre por su descriptor (/proc/self/fd), sin copiar
/// el archivo; si así no se puede (un adjunto que llega como flujo), se copia a la caché de la app.
#[cfg_attr(not(target_os = "android"), allow(unused_variables))]
fn abrir(app: &AppHandle, motor: &Motor, ruta: FilePath) -> Result<Info, String> {
    #[cfg(target_os = "android")]
    {
        if let FilePath::Url(url) = &ruta {
            use std::os::fd::AsRawFd;
            use tauri_plugin_fs::{FsExt, OpenOptions};
            let leer = || {
                let mut o = OpenOptions::new();
                o.read(true);
                app.fs().open(ruta.clone(), o).map_err(|e| e.to_string())
            };
            let archivo = leer()?;
            let fd = format!("/proc/self/fd/{}", archivo.as_raw_fd());
            let directo = std::fs::File::open(&fd).and_then(|f| f.metadata()).is_ok_and(|m| m.is_file());
            let mut info = if directo {
                motor.abrir(fd, Some(archivo))?
            } else {
                let copia = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("abierto.pdf");
                std::fs::create_dir_all(copia.parent().expect("la caché tiene carpeta")).map_err(|e| e.to_string())?;
                // Se borra en vez de sobrescribir: el documento anterior sigue leyendo la copia vieja hasta cambiarse.
                let _ = std::fs::remove_file(&copia);
                let mut destino = std::fs::File::create(&copia).map_err(|e| e.to_string())?;
                std::io::copy(&mut leer()?, &mut destino).map_err(|e| e.to_string())?;
                motor.abrir(copia.to_string_lossy().into_owned(), None)?
            };
            // Nombre de respaldo; el bueno lo da Android con OpenableColumns.
            let nombre = url
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .map(|s| s.replace("%20", " "))
                .unwrap_or_else(|| "Documento".into());
            info.completar(url.to_string(), nombre);
            return Ok(info);
        }
    }
    let ruta = ruta.into_path().map_err(|e| e.to_string())?;
    let original = ruta.to_string_lossy().into_owned();
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Documento".into());
    let mut info = motor.abrir(original.clone(), None)?;
    info.completar(original, nombre);
    Ok(info)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Motor::nuevo())
        .manage(Inicial(Mutex::new(std::env::args().nth(1).filter(|a| !a.starts_with('-')))))
        .register_asynchronous_uri_scheme_protocol("pagina", |ctx, pedido, resp| {
            ctx.app_handle().state::<Motor>().pagina(pedido.uri(), resp)
        })
        .invoke_handler(tauri::generate_handler![elegir_pdf, archivo_inicial, abrir_ruta, descargar_version, buscar])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar Dakarta");
}

mod cache;
mod motor;

use motor::{Info, Motor};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

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
        if let FilePath::Url(_) = ruta {
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
            if directo {
                return motor.abrir(fd, Some(archivo));
            }
            let copia = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("abierto.pdf");
            std::fs::create_dir_all(copia.parent().expect("la caché tiene carpeta")).map_err(|e| e.to_string())?;
            // Se borra en vez de sobrescribir: el documento anterior sigue leyendo la copia vieja hasta cambiarse.
            let _ = std::fs::remove_file(&copia);
            let mut destino = std::fs::File::create(&copia).map_err(|e| e.to_string())?;
            std::io::copy(&mut leer()?, &mut destino).map_err(|e| e.to_string())?;
            return motor.abrir(copia.to_string_lossy().into_owned(), None);
        }
    }
    let ruta = ruta.into_path().map_err(|e| e.to_string())?;
    motor.abrir(ruta.to_string_lossy().into_owned(), None)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(Motor::nuevo())
        .register_asynchronous_uri_scheme_protocol("pagina", |ctx, pedido, resp| {
            ctx.app_handle().state::<Motor>().pagina(pedido.uri(), resp)
        })
        .invoke_handler(tauri::generate_handler![elegir_pdf])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar Dakarta");
}

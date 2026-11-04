mod cache;
mod motor;

use std::sync::Mutex;

use motor::{Anotacion, Busqueda, Campo, Enlace, Hoja, Info, Motor, Palabra};
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
fn cancelar_busqueda(motor: State<'_, Motor>) {
    motor.cancelar_busqueda();
}

#[tauri::command]
async fn anotar_texto(
    motor: State<'_, Motor>,
    pagina: i32,
    tipo: String,
    rojo: f32,
    verde: f32,
    azul: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
) -> Result<(), String> {
    motor.anotar_texto(pagina, &tipo, (rojo, verde, azul), (x0, y0), (x1, y1))
}

#[tauri::command]
async fn anotar_tinta(
    motor: State<'_, Motor>,
    pagina: i32,
    rojo: f32,
    verde: f32,
    azul: f32,
    grosor: f32,
    trazos: Vec<Vec<(f32, f32)>>,
) -> Result<(), String> {
    motor.anotar_tinta(pagina, (rojo, verde, azul), grosor, trazos)
}

#[tauri::command]
async fn anotar_caja(
    motor: State<'_, Motor>,
    pagina: i32,
    x: f32,
    y: f32,
    contenido: String,
    rojo: f32,
    verde: f32,
    azul: f32,
    tamano: f32,
    postit: bool,
) -> Result<(), String> {
    motor.anotar_caja(pagina, x, y, contenido, (rojo, verde, azul), tamano, postit)
}

#[tauri::command]
async fn anotar_sello(
    motor: State<'_, Motor>,
    pagina: i32,
    x: f32,
    y: f32,
    texto: String,
    rojo: f32,
    verde: f32,
    azul: f32,
    tamano: f32,
) -> Result<(), String> {
    motor.anotar_sello(pagina, x, y, texto, (rojo, verde, azul), tamano)
}

#[tauri::command]
async fn anotaciones(motor: State<'_, Motor>, pagina: i32) -> Result<Vec<Anotacion>, String> {
    motor.anotaciones(pagina)
}

#[tauri::command]
async fn palabras(motor: State<'_, Motor>, pagina: i32) -> Result<Vec<Palabra>, String> {
    motor.palabras(pagina)
}

#[tauri::command]
async fn enlaces(motor: State<'_, Motor>, pagina: i32) -> Result<Vec<Enlace>, String> {
    motor.enlaces(pagina)
}

#[tauri::command]
async fn mover_anotacion(motor: State<'_, Motor>, pagina: i32, xref: i32, dx: f32, dy: f32) -> Result<(), String> {
    motor.mover_anotacion(pagina, xref, dx, dy)
}

#[tauri::command]
async fn editar_anotacion(
    motor: State<'_, Motor>,
    pagina: i32,
    xref: i32,
    contenido: Option<String>,
    rojo: Option<f32>,
    verde: Option<f32>,
    azul: Option<f32>,
    tamano: Option<f32>,
    junto: Option<bool>,
) -> Result<bool, String> {
    let color = match (rojo, verde, azul) {
        (Some(r), Some(g), Some(b)) => Some((r, g, b)),
        _ => None,
    };
    motor.editar_anotacion(pagina, xref, contenido, color, tamano, junto.unwrap_or(false))
}

#[tauri::command]
async fn redimensionar_anotacion(
    motor: State<'_, Motor>,
    pagina: i32,
    xref: i32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    motor.redimensionar_anotacion(pagina, xref, x, y, w, h)
}

#[tauri::command]
async fn borrar_anotacion(motor: State<'_, Motor>, pagina: i32, xref: i32) -> Result<(), String> {
    motor.borrar_anotacion(pagina, xref)
}

#[tauri::command]
async fn campos(motor: State<'_, Motor>) -> Result<Vec<Campo>, String> {
    motor.campos()
}

#[tauri::command]
async fn poner_campo(motor: State<'_, Motor>, pagina: i32, xref: i32, valor: String) -> Result<(), String> {
    motor.poner_campo(pagina, xref, valor)
}

#[tauri::command]
async fn deshacer(motor: State<'_, Motor>) -> Result<Option<i32>, String> {
    motor.deshacer()
}

#[tauri::command]
async fn deshacer_todo(motor: State<'_, Motor>) -> Result<bool, String> {
    motor.deshacer_todo()
}

/// Android: deja la copia editada en la caché; la app la copia al destino elegido por el usuario.
#[tauri::command]
async fn guardar_en_cache(app: AppHandle, motor: State<'_, Motor>, hojas: Option<Vec<Hoja>>) -> Result<String, String> {
    let copia = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("copia.pdf");
    let ruta = copia.to_string_lossy().into_owned();
    motor.guardar(ruta.clone(), hojas)?;
    Ok(ruta)
}

/// Windows: pregunta dónde guardar y escribe la copia ahí.
#[tauri::command]
async fn guardar_copia(
    app: AppHandle,
    motor: State<'_, Motor>,
    nombre: String,
    hojas: Option<Vec<Hoja>>,
) -> Result<Option<String>, String> {
    let Some(ruta) = app.dialog().file().set_file_name(nombre).add_filter("PDF", &["pdf"]).blocking_save_file() else {
        return Ok(None);
    };
    let ruta = ruta.into_path().map_err(|e| e.to_string())?;
    let destino = ruta.to_string_lossy().into_owned();
    motor.guardar(destino.clone(), hojas)?;
    Ok(Some(destino))
}

/// Windows: abre la carpeta con el PDF seleccionado para poder enviarlo.
#[tauri::command]
fn mostrar_archivo(app: AppHandle, ruta: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(ruta).map_err(|e| e.to_string())
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
        .invoke_handler(tauri::generate_handler![
    elegir_pdf,
    archivo_inicial,
    abrir_ruta,
    descargar_version,
    buscar,
    cancelar_busqueda,
    mostrar_archivo,
    anotar_texto,
    anotar_tinta,
    anotar_caja,
    anotar_sello,
    anotaciones,
    palabras,
    enlaces,
    mover_anotacion,
    editar_anotacion,
    redimensionar_anotacion,
    borrar_anotacion,
    campos,
    poner_campo,
    deshacer,
    deshacer_todo,
    guardar_en_cache,
    guardar_copia
])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar Dakarta");
}

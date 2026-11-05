mod cache;
mod motor;

use std::sync::Mutex;

use motor::{pdf_de_fotos, Anotacion, Diseno, Busqueda, Campo, Enlace, Hoja, Info, Motor, Palabra};
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
/// `clave`: la contraseña del PDF, si la pidió (el error `CLAVE:PIDE` o `CLAVE:MAL` dice que hace falta).
#[tauri::command]
async fn abrir_ruta(app: AppHandle, motor: State<'_, Motor>, ruta: String, clave: Option<String>) -> Result<Info, String> {
    let ruta = ruta.parse::<FilePath>().map_err(|e| e.to_string())?;
    abrir(&app, &motor, ruta, clave)
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
async fn girar_anotacion(motor: State<'_, Motor>, pagina: i32, xref: i32, grados: f32) -> Result<(), String> {
    motor.girar_anotacion(pagina, xref, grados)
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
/// `proteccion`: una contraseña nueva para la copia, o vacía para guardarla sin contraseña.
#[tauri::command]
async fn guardar_en_cache(
    app: AppHandle,
    motor: State<'_, Motor>,
    hojas: Option<Vec<Hoja>>,
    proteccion: Option<String>,
) -> Result<String, String> {
    let copia = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("copia.pdf");
    let ruta = copia.to_string_lossy().into_owned();
    motor.guardar(ruta.clone(), hojas, proteccion)?;
    Ok(ruta)
}

/// Windows: pregunta dónde guardar y escribe la copia ahí.
#[tauri::command]
async fn guardar_copia(
    app: AppHandle,
    motor: State<'_, Motor>,
    nombre: String,
    hojas: Option<Vec<Hoja>>,
    proteccion: Option<String>,
) -> Result<Option<String>, String> {
    let Some(ruta) = app.dialog().file().set_file_name(nombre).add_filter("PDF", &["pdf"]).blocking_save_file() else {
        return Ok(None);
    };
    let ruta = ruta.into_path().map_err(|e| e.to_string())?;
    let destino = ruta.to_string_lossy().into_owned();
    motor.guardar(destino.clone(), hojas, proteccion)?;
    Ok(Some(destino))
}

/// Windows: abre la carpeta con el PDF seleccionado para poder enviarlo.
#[tauri::command]
fn mostrar_archivo(app: AppHandle, ruta: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(ruta).map_err(|e| e.to_string())
}

/// Windows: pregunta qué PDF abrir y devuelve su ruta; la pantalla lo abre (y pide la contraseña si tiene).
#[tauri::command]
async fn elegir_pdf(app: AppHandle) -> Result<Option<String>, String> {
    let Some(ruta) = app.dialog().file().add_filter("PDF", &["pdf"]).blocking_pick_file() else {
        return Ok(None);
    };
    Ok(Some(ruta.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned()))
}

/// Lo que el motor necesita para leer un archivo: su ruta, el archivo abierto (Android) y su nombre.
struct ParaLeer {
    ruta: String,
    archivo: Option<std::fs::File>,
    nombre: String,
    original: String,
}

/// Windows: la ruta del archivo. Android: el content:// se abre por su descriptor (/proc/self/fd), sin copiar
/// el archivo; si así no se puede (un adjunto que llega como flujo), se copia a la caché de la app con `copia`
/// como nombre.
#[cfg_attr(not(target_os = "android"), allow(unused_variables))]
fn preparar(app: &AppHandle, ruta: FilePath, copia: &str) -> Result<ParaLeer, String> {
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
            // Nombre de respaldo; el bueno lo da Android con OpenableColumns.
            let nombre = url
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .map(|s| s.replace("%20", " "))
                .unwrap_or_else(|| "Documento".into());
            if directo {
                return Ok(ParaLeer { ruta: fd, archivo: Some(archivo), nombre, original: url.to_string() });
            }
            let destino = app.path().app_cache_dir().map_err(|e| e.to_string())?.join(copia);
            std::fs::create_dir_all(destino.parent().expect("la caché tiene carpeta")).map_err(|e| e.to_string())?;
            // Se borra en vez de sobrescribir: el documento anterior sigue leyendo la copia vieja hasta cambiarse.
            let _ = std::fs::remove_file(&destino);
            let mut f = std::fs::File::create(&destino).map_err(|e| e.to_string())?;
            std::io::copy(&mut leer()?, &mut f).map_err(|e| e.to_string())?;
            return Ok(ParaLeer { ruta: destino.to_string_lossy().into_owned(), archivo: None, nombre, original: url.to_string() });
        }
    }
    let ruta = ruta.into_path().map_err(|e| e.to_string())?;
    let original = ruta.to_string_lossy().into_owned();
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Documento".into());
    Ok(ParaLeer { ruta: original.clone(), archivo: None, nombre, original })
}

fn abrir(app: &AppHandle, motor: &Motor, ruta: FilePath, clave: Option<String>) -> Result<Info, String> {
    let p = preparar(app, ruta, "abierto.pdf")?;
    let mut info = motor.abrir(p.ruta, p.archivo, clave)?;
    info.completar(p.original, p.nombre);
    Ok(info)
}

/// Un PDF agregado para unir: su número en el motor, cuántas páginas tiene y su nombre.
#[derive(serde::Serialize)]
struct Agregado {
    archivo: usize,
    paginas: i32,
    nombre: String,
}

/// Unir PDFs, Windows: pregunta qué PDFs unir (se pueden elegir varios a la vez) y devuelve sus rutas.
#[tauri::command]
async fn elegir_pdfs(app: AppHandle) -> Result<Vec<String>, String> {
    let elegidos = app.dialog().file().add_filter("PDF", &["pdf"]).blocking_pick_files().unwrap_or_default();
    elegidos.into_iter().map(|r| Ok(r.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned())).collect()
}

/// Unir PDFs: abre otro PDF para unirlo, con su contraseña si la pidió (como `abrir_ruta`).
#[tauri::command]
async fn abrir_agregado(app: AppHandle, motor: State<'_, Motor>, ruta: String, clave: Option<String>) -> Result<Agregado, String> {
    // Cada copia en la caché con su nombre: los PDFs agregados antes siguen leyendo la suya.
    static SIGUIENTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SIGUIENTE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = preparar(&app, ruta.parse::<FilePath>().map_err(|e| e.to_string())?, &format!("unir-{n}.pdf"))?;
    let (archivo, paginas) = motor.abrir_agregado(p.ruta, p.archivo, clave)?;
    Ok(Agregado { archivo, paginas, nombre: p.nombre })
}

/// Crear PDF desde fotos: las fotos ya preparadas (JPEG) esperan aquí, en orden, hasta guardar.
struct Fotos(Mutex<Vec<Vec<u8>>>);

/// Empieza un PDF de fotos nuevo.
#[tauri::command]
fn fotos_vaciar(fotos: State<'_, Fotos>) {
    fotos.0.lock().expect("fotos").clear();
}

/// Agrega la siguiente foto (llega como bytes, sin pasar a texto).
#[tauri::command]
fn fotos_agregar(pedido: tauri::ipc::Request<'_>, fotos: State<'_, Fotos>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(jpg) = pedido.body() else {
        return Err("La foto no llegó bien.".into());
    };
    fotos.0.lock().expect("fotos").push(jpg.clone());
    Ok(())
}

fn armar_fotos(fotos: &Fotos, diseno: Diseno, destino: &str) -> Result<(), String> {
    let lista = std::mem::take(&mut *fotos.0.lock().expect("fotos"));
    pdf_de_fotos(&lista, diseno, destino)
}

/// Android: arma el PDF de fotos en la caché; la app lo copia al destino elegido por el usuario.
#[tauri::command]
async fn fotos_en_cache(app: AppHandle, fotos: State<'_, Fotos>, diseno: Diseno) -> Result<String, String> {
    let ruta = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("fotos.pdf");
    let ruta = ruta.to_string_lossy().into_owned();
    armar_fotos(&fotos, diseno, &ruta)?;
    Ok(ruta)
}

/// Windows: pregunta dónde guardar el PDF de fotos y lo arma ahí.
#[tauri::command]
async fn fotos_guardar_copia(app: AppHandle, fotos: State<'_, Fotos>, nombre: String, diseno: Diseno) -> Result<Option<String>, String> {
    let Some(ruta) = app.dialog().file().set_file_name(nombre).add_filter("PDF", &["pdf"]).blocking_save_file() else {
        return Ok(None);
    };
    let destino = ruta.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    armar_fotos(&fotos, diseno, &destino)?;
    Ok(Some(destino))
}

#[tauri::command]
fn cerrar_agregados(motor: State<'_, Motor>) {
    motor.cerrar_agregados();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Motor::nuevo())
        .manage(Fotos(Mutex::new(Vec::new())))
        .manage(Inicial(Mutex::new(std::env::args().nth(1).filter(|a| !a.starts_with('-')))))
        .register_asynchronous_uri_scheme_protocol("pagina", |ctx, pedido, resp| {
            ctx.app_handle().state::<Motor>().pagina(pedido.uri(), resp)
        })
        .invoke_handler(tauri::generate_handler![
    elegir_pdf,
    elegir_pdfs,
    fotos_vaciar,
    fotos_agregar,
    fotos_en_cache,
    fotos_guardar_copia,
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
    girar_anotacion,
    borrar_anotacion,
    campos,
    poner_campo,
    deshacer,
    deshacer_todo,
    guardar_en_cache,
    guardar_copia,
    abrir_agregado,
    cerrar_agregados
])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar Dakarta");
}

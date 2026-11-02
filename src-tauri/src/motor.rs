use std::fmt::Display;
use std::fs::File;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use mupdf::text_page::TextExtractOptions;
use mupdf::{Colorspace, Device, Document, Matrix, Outline, Pixmap};
use serde::Serialize;
use tauri::UriSchemeResponder;
use tauri::http::{Response, Uri, header};

use crate::cache::Cache;

#[derive(Serialize)]
pub struct Info {
    paginas: i32,
    /// Tamaño de la primera página: reserva el espacio de las demás antes de pintarlas.
    ancho: f32,
    alto: f32,
    indice: Vec<Entrada>,
    ms: u128,
    /// Para volver a abrirlo después (ruta en Windows, content:// en Android).
    ruta: String,
    nombre: String,
}

impl Info {
    pub fn completar(&mut self, ruta: String, nombre: String) {
        self.ruta = ruta;
        self.nombre = nombre;
    }
}

#[derive(Serialize)]
pub struct Busqueda {
    resultados: Vec<Coincidencia>,
    /// El PDF no tiene texto (parece escaneado): no hay nada que buscar.
    escaneado: bool,
}

#[derive(Serialize)]
struct Coincidencia {
    pagina: i32,
    /// Caja del texto encontrado, en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Serialize)]
struct Entrada {
    titulo: String,
    pagina: Option<u32>,
    hijos: Vec<Entrada>,
}

/// Ancho de las miniaturas que se ven mientras se desplaza (el mismo valor en src/zoom.ts).
const MINIATURA: u32 = 384;
const TOPE_CACHE: usize = 64 * 1024 * 1024;

/// Pedazo de una página, para hacer zoom sin pintar la hoja entera: w y h son el tamaño de la página entera a ese zoom.
#[derive(Clone, Copy)]
struct Recorte {
    w: u32,
    h: u32,
    x: u32,
    y: u32,
    cw: u32,
    ch: u32,
}

struct PedidoPagina {
    n: i32,
    ancho: u32,
    recorte: Option<Recorte>,
    resp: UriSchemeResponder,
}

enum Pedido {
    Abrir { ruta: String, archivo: Option<File>, resp: Sender<Result<Info, String>> },
    Buscar { texto: String, resp: Sender<Result<Busqueda, String>> },
    Pagina(PedidoPagina),
}

/// MuPDF no se comparte entre hilos: un solo hilo guarda el documento y atiende los pedidos.
pub struct Motor(Sender<Pedido>);

impl Motor {
    pub fn nuevo() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // En Android el archivo viaja con el documento: si se cerrara, MuPDF perdería el acceso.
            let mut abierto: Option<(Document, Option<File>)> = None;
            let mut pendientes: Vec<PedidoPagina> = Vec::new();
            let mut cache = Cache::nuevo(TOPE_CACHE);
            loop {
                // Sin trabajo se espera; con trabajo solo se recoge lo que ya llegó.
                let primero = if pendientes.is_empty() {
                    match rx.recv() {
                        Ok(p) => Some(p),
                        Err(_) => return,
                    }
                } else {
                    None
                };
                for pedido in primero.into_iter().chain(rx.try_iter()) {
                    match pedido {
                        Pedido::Abrir { ruta, archivo, resp } => {
                            let r = abrir_doc(&ruta).map(|(doc, info)| {
                                abierto = Some((doc, archivo));
                                cache.vaciar();
                                // Lo que faltaba del documento anterior ya nadie lo está esperando.
                                for p in pendientes.drain(..) {
                                    p.resp.respond(respuesta(Err("Documento cerrado".into())));
                                }
                                info
                            });
                            let _ = resp.send(r);
                        }
                        Pedido::Buscar { texto, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => buscar_doc(doc, &texto),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Pagina(p) => pendientes.push(p),
                    }
                }
                // Primero lo más reciente (lo que se está mirando): recortes, luego hojas enteras, luego miniaturas.
                let Some(i) = pendientes
                    .iter()
                    .rposition(|p| p.recorte.is_some())
                    .or_else(|| pendientes.iter().rposition(|p| p.ancho != MINIATURA))
                    .or(pendientes.len().checked_sub(1))
                else {
                    continue;
                };
                let p = pendientes.remove(i);
                // De una misma página solo sirve el recorte más reciente.
                if p.recorte.is_some() {
                    for viejo in pendientes.extract_if(.., |q| q.recorte.is_some() && q.n == p.n) {
                        viejo.resp.respond(respuesta(Err("Recorte reemplazado".into())));
                    }
                }
                let clave = (p.n, p.ancho);
                let r = match (&abierto, p.recorte) {
                    (None, _) => Err("No hay un documento abierto".into()),
                    (Some((doc, _)), Some(recorte)) => pintar_recorte(doc, p.n, recorte),
                    (Some((doc, _)), None) => match cache.leer(clave) {
                        Some(jpg) => Ok(jpg),
                        None => pintar(doc, p.n, p.ancho).inspect(|jpg| {
                            // Las miniaturas no: la pantalla las conserva.
                            if p.ancho != MINIATURA {
                                cache.guardar(clave, jpg.clone());
                            }
                        }),
                    },
                };
                p.resp.respond(respuesta(r));
            }
        });
        Motor(tx)
    }

    pub fn abrir(&self, ruta: String, archivo: Option<File>) -> Result<Info, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Abrir { ruta, archivo, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn buscar(&self, texto: String) -> Result<Busqueda, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Buscar { texto, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    /// Pedido de la pantalla: pagina://localhost/<n>?w=<ancho en píxeles>
    /// Para un recorte, además: h=<alto de la página>, x, y, cw, ch (el pedazo, en píxeles).
    pub fn pagina(&self, uri: &Uri, resp: UriSchemeResponder) {
        let q = |k: &str| -> Option<u32> {
            uri.query()?.split('&').find_map(|p| p.strip_prefix(k)?.strip_prefix('='))?.parse().ok()
        };
        let leido = || -> Option<(i32, u32, Option<Recorte>)> {
            let n = uri.path().trim_start_matches('/').parse().ok()?;
            let w = q("w")?;
            let recorte = (|| Some(Recorte { w, h: q("h")?, x: q("x")?, y: q("y")?, cw: q("cw")?, ch: q("ch")? }))();
            Some((n, w, recorte))
        };
        match leido() {
            Some((n, ancho, recorte)) => {
                let _ = self.0.send(Pedido::Pagina(PedidoPagina { n, ancho, recorte, resp }));
            }
            None => resp.respond(respuesta(Err("Pedido no válido".into()))),
        }
    }
}

fn abrir_doc(ruta: &str) -> Result<(Document, Info), String> {
    let t = Instant::now();
    let doc = Document::open(ruta).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
    if doc.needs_password().map_err(texto)? {
        return Err("Este PDF tiene contraseña. Abrir PDFs con contraseña llegará en una próxima versión.".into());
    }
    let paginas = doc.page_count().map_err(texto)?;
    if paginas < 1 {
        return Err("El documento no tiene páginas.".into());
    }
    let b = doc.load_page(0).and_then(|p| p.bounds()).map_err(texto)?;
    let indice = doc.outlines().map(|o| entradas(&o)).unwrap_or_default();
    let info = Info {
        paginas,
        ancho: b.width(),
        alto: b.height(),
        indice,
        ms: t.elapsed().as_millis(),
        ruta: String::new(),
        nombre: String::new(),
    };
    Ok((doc, info))
}

fn entradas(o: &[Outline]) -> Vec<Entrada> {
    o.iter()
        .map(|e| Entrada {
            titulo: e.title.clone(),
            pagina: e.dest.as_ref().map(|d| d.loc.page_number),
            hijos: entradas(&e.down),
        })
        .collect()
}

/// Busca en todas las páginas. Las cajas vuelven en fracción de la página, para pintarlas encima sin depender del zoom.
fn buscar_doc(doc: &Document, aguja: &str) -> Result<Busqueda, String> {
    let aguja = aguja.trim();
    if aguja.is_empty() {
        return Err("Escribe qué buscar.".into());
    }
    let paginas = doc.page_count().map_err(texto)?;
    let mut resultados = Vec::new();
    let mut tiene_texto = false;
    for n in 0..paginas {
        let pagina = doc.load_page(n).map_err(texto)?;
        let b = pagina.bounds().map_err(texto)?;
        if b.width() <= 0.0 || b.height() <= 0.0 {
            continue;
        }
        // Solo se mira si hay texto mientras no se haya encontrado: el aviso de escaneado no cuesta en PDFs normales.
        if !tiene_texto {
            tiene_texto = !pagina.text(TextExtractOptions::default()).map_err(texto)?.trim().is_empty();
        }
        let quads = pagina.search(aguja, 500).map_err(texto)?;
        for q in quads.iter() {
            let x0 = q.ul.x.min(q.ll.x).min(q.ur.x).min(q.lr.x);
            let x1 = q.ul.x.max(q.ll.x).max(q.ur.x).max(q.lr.x);
            let y0 = q.ul.y.min(q.ll.y).min(q.ur.y).min(q.lr.y);
            let y1 = q.ul.y.max(q.ll.y).max(q.ur.y).max(q.lr.y);
            resultados.push(Coincidencia {
                pagina: n,
                x: ((x0 - b.x0) / b.width()).clamp(0.0, 1.0),
                y: ((y0 - b.y0) / b.height()).clamp(0.0, 1.0),
                w: ((x1 - x0) / b.width()).clamp(0.0, 1.0),
                h: ((y1 - y0) / b.height()).clamp(0.0, 1.0),
            });
        }
    }
    Ok(Busqueda { resultados, escaneado: resultados.is_empty() && !tiene_texto })
}

fn pintar(doc: &Document, n: i32, ancho: u32) -> Result<Vec<u8>, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let b = pagina.bounds().map_err(texto)?;
    // Topes: 6000 px de ancho y 8000 de alto (memoria del celular; el JPEG admite hasta 65535).
    let escala = (ancho.min(6000) as f32 / b.width()).min(8000.0 / b.height());
    if !(escala.is_finite() && escala > 0.0) {
        return Err("La página está vacía.".into());
    }
    let px = pagina
        .to_pixmap(&Matrix::new_scale(escala, escala), &Colorspace::device_rgb(), false, true)
        .map_err(texto)?;
    jpeg(&px, if ancho == MINIATURA { 60 } else { 90 })
}

/// Pinta solo el pedazo pedido: el trabajo y la memoria dependen del tamaño de la pantalla, no del zoom.
fn pintar_recorte(doc: &Document, n: i32, r: Recorte) -> Result<Vec<u8>, String> {
    if r.cw == 0 || r.ch == 0 || r.cw > 6000 || r.ch > 6000 {
        return Err("Recorte no válido.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let b = pagina.bounds().map_err(texto)?;
    // Escala de cada eje por separado: así coincide exacto con la hoja entera que se ve debajo.
    let (sx, sy) = (r.w as f32 / b.width(), r.h as f32 / b.height());
    if !(sx.is_finite() && sy.is_finite() && sx > 0.0 && sy > 0.0) {
        return Err("La página está vacía.".into());
    }
    let mut px = Pixmap::new_with_w_h(&Colorspace::device_rgb(), r.cw as i32, r.ch as i32, false).map_err(texto)?;
    px.clear_with(255).map_err(texto)?;
    let ctm = Matrix::new(sx, 0.0, 0.0, sy, -b.x0 * sx - r.x as f32, -b.y0 * sy - r.y as f32);
    let dispositivo = Device::from_pixmap(&px).map_err(texto)?;
    pagina.run(&dispositivo, &ctm).map_err(texto)?;
    drop(dispositivo);
    jpeg(&px, 95)
}

/// Desde calidad 90 el JPEG no reduce el color: las letras de color quedan limpias.
fn jpeg(px: &Pixmap, calidad: u8) -> Result<Vec<u8>, String> {
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, calidad)
        .encode(px.samples(), px.width() as u16, px.height() as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(texto)?;
    Ok(jpg)
}

fn respuesta(r: Result<Vec<u8>, String>) -> Response<Vec<u8>> {
    let b = Response::builder();
    match r {
        // La pantalla guarda las páginas ya pintadas: volver a una no la vuelve a pedir.
        Ok(jpg) => b.header(header::CONTENT_TYPE, "image/jpeg").header(header::CACHE_CONTROL, "max-age=3600").body(jpg),
        Err(e) => b.status(500).header(header::CONTENT_TYPE, "text/plain; charset=utf-8").body(e.into_bytes()),
    }
    .expect("cabeceras fijas válidas")
}

fn texto(e: impl Display) -> String {
    e.to_string()
}

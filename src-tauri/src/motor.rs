use std::fmt::Display;
use std::fs::File;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use mupdf::{Colorspace, Document, Matrix, Outline};
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

struct PedidoPagina {
    n: i32,
    ancho: u32,
    resp: UriSchemeResponder,
}

enum Pedido {
    Abrir { ruta: String, archivo: Option<File>, resp: Sender<Result<Info, String>> },
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
                        Pedido::Pagina(p) => pendientes.push(p),
                    }
                }
                // Primero lo más reciente (lo que se está mirando); las páginas nítidas antes que las miniaturas.
                let Some(i) = pendientes.iter().rposition(|p| p.ancho != MINIATURA).or(pendientes.len().checked_sub(1))
                else {
                    continue;
                };
                let p = pendientes.remove(i);
                let clave = (p.n, p.ancho);
                let r = match (cache.leer(clave), &abierto) {
                    (Some(jpg), _) => Ok(jpg),
                    (None, Some((doc, _))) => pintar(doc, p.n, p.ancho).inspect(|jpg| {
                        // Las miniaturas no: la pantalla las conserva.
                        if p.ancho != MINIATURA {
                            cache.guardar(clave, jpg.clone());
                        }
                    }),
                    (None, None) => Err("No hay un documento abierto".into()),
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

    /// Pedido de la pantalla: pagina://localhost/<n>?w=<ancho en píxeles>
    pub fn pagina(&self, uri: &Uri, resp: UriSchemeResponder) {
        let leido = || -> Option<(i32, u32)> {
            let n = uri.path().trim_start_matches('/').parse().ok()?;
            let w = uri.query()?.split('&').find_map(|p| p.strip_prefix("w="))?.parse().ok()?;
            Some((n, w))
        };
        match leido() {
            Some((n, ancho)) => {
                let _ = self.0.send(Pedido::Pagina(PedidoPagina { n, ancho, resp }));
            }
            None => resp.respond(respuesta(Err("Pedido no válido".into()))),
        }
    }
}

fn abrir_doc(ruta: &str) -> Result<(Document, Info), String> {
    let t = Instant::now();
    let doc = Document::open(ruta).map_err(texto)?;
    if doc.needs_password().map_err(texto)? {
        return Err("Este PDF tiene contraseña. Abrir PDFs con contraseña llegará en una próxima versión.".into());
    }
    let paginas = doc.page_count().map_err(texto)?;
    if paginas < 1 {
        return Err("El documento no tiene páginas.".into());
    }
    let b = doc.load_page(0).and_then(|p| p.bounds()).map_err(texto)?;
    let indice = doc.outlines().map(|o| entradas(&o)).unwrap_or_default();
    let info = Info { paginas, ancho: b.width(), alto: b.height(), indice, ms: t.elapsed().as_millis() };
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
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, if ancho == MINIATURA { 60 } else { 85 })
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

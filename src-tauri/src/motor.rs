use std::fmt::Display;
use std::fs::File;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use mupdf::color::AnnotationColor;
use mupdf::pdf::document::{PdfDocument, PdfWriteOptions};
use mupdf::point::Point;
use mupdf::quad::Quad;
use mupdf::rect::Rect;
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
    /// Se encontraron más de los que se devuelven: una palabra muy común llenaría la pantalla.
    truncada: bool,
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

#[derive(Serialize)]
pub struct Campo {
    pagina: i32,
    xref: i32,
    tipo: String,
    nombre: String,
    valor: String,
    readonly: bool,
    /// Caja del campo, en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Ancho de las miniaturas que se ven mientras se desplaza (el mismo valor en src/zoom.ts).
const MINIATURA: u32 = 384;
const TOPE_CACHE: usize = 64 * 1024 * 1024;
/// Tope de coincidencias que se devuelven a la pantalla.
const TOPE_RESULTADOS: usize = 2000;

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

#[derive(Clone, Copy)]
enum TipoTexto {
    Resaltar,
    Subrayar,
    Tachar,
}

/// Lo que se puede deshacer en esta sesión.
enum Hecho {
    Anotacion { n: i32, xref: i32 },
    Campo { n: i32, xref: i32, anterior: String },
}

enum Pedido {
    Abrir { ruta: String, archivo: Option<File>, resp: Sender<Result<Info, String>> },
    Buscar { texto: String, resp: Sender<Result<Busqueda, String>> },
    Pagina(PedidoPagina),
    Texto {
        n: i32,
        tipo: TipoTexto,
        color: (f32, f32, f32),
        a: (f32, f32),
        b: (f32, f32),
        resp: Sender<Result<(), String>>,
    },
    Tinta {
        n: i32,
        color: (f32, f32, f32),
        grosor: f32,
        trazos: Vec<Vec<(f32, f32)>>,
        resp: Sender<Result<(), String>>,
    },
    Nota {
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        resp: Sender<Result<(), String>>,
    },
    Caja {
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        resp: Sender<Result<(), String>>,
    },
    Sello {
        n: i32,
        x: f32,
        y: f32,
        texto: String,
        color: (f32, f32, f32),
        resp: Sender<Result<(), String>>,
    },
    Deshacer { resp: Sender<Result<bool, String>> },
    DeshacerTodo { resp: Sender<Result<bool, String>> },
    Campos { resp: Sender<Result<Vec<Campo>, String>> },
    Campo { n: i32, xref: i32, valor: String, resp: Sender<Result<(), String>> },
    Guardar { destino: String, resp: Sender<Result<(), String>> },
}

/// MuPDF no se comparte entre hilos: un solo hilo guarda el documento y atiende los pedidos.
pub struct Motor(Sender<Pedido>);

impl Motor {
    pub fn nuevo() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // En Android el archivo viaja con el documento: si se cerrara, MuPDF perdería el acceso.
            let mut abierto: Option<(PdfDocument, Option<File>)> = None;
            // Anotaciones y campos cambiados en esta sesión, para deshacer.
            let mut hechas: Vec<Hecho> = Vec::new();
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
                                hechas.clear();
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
                        Pedido::Texto { n, tipo, color, a, b, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_texto(doc, n, tipo, color, a, b).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Tinta { n, color, grosor, trazos, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_tinta(doc, n, color, grosor, &trazos).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Nota { n, x, y, contenido, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_nota(doc, n, x, y, &contenido).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Caja { n, x, y, contenido, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_caja(doc, n, x, y, &contenido).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Sello { n, x, y, texto, color, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_sello(doc, n, x, y, &texto, color).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Campos { resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => campos_doc(doc),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Campo { n, xref, valor, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => fijar_campo(doc, n, xref, &valor).map(|anterior| {
                                    hechas.push(Hecho::Campo { n, xref, anterior });
                                    cache.vaciar();
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Deshacer { resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => deshacer(doc, &mut hechas).map(|hecho| {
                                    if hecho {
                                        cache.vaciar();
                                    }
                                    hecho
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::DeshacerTodo { resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => deshacer_todo(doc, &mut hechas).map(|hecho| {
                                    if hecho {
                                        cache.vaciar();
                                    }
                                    hecho
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Guardar { destino, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                // Lo guardado queda como punto de partida: ya no se deshace.
                                Some((doc, _)) => guardar_doc(doc, &destino).map(|()| hechas.clear()),
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

    pub fn buscar(&self, aguja: String) -> Result<Busqueda, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Buscar { texto: aguja, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_texto(&self, n: i32, tipo: &str, color: (f32, f32, f32), a: (f32, f32), b: (f32, f32)) -> Result<(), String> {
        let tipo = match tipo {
            "resaltar" => TipoTexto::Resaltar,
            "subrayar" => TipoTexto::Subrayar,
            "tachar" => TipoTexto::Tachar,
            _ => return Err("Herramienta no válida.".into()),
        };
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Texto { n, tipo, color, a, b, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_tinta(&self, n: i32, color: (f32, f32, f32), grosor: f32, trazos: Vec<Vec<(f32, f32)>>) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Tinta { n, color, grosor, trazos, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_nota(&self, n: i32, x: f32, y: f32, contenido: String) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Nota { n, x, y, contenido, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_caja(&self, n: i32, x: f32, y: f32, contenido: String) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Caja { n, x, y, contenido, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_sello(&self, n: i32, x: f32, y: f32, etiqueta: String, color: (f32, f32, f32)) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Sello { n, x, y, texto: etiqueta, color, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn campos(&self) -> Result<Vec<Campo>, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Campos { resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn poner_campo(&self, n: i32, xref: i32, valor: String) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Campo { n, xref, valor, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn deshacer(&self) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Deshacer { resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn deshacer_todo(&self) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::DeshacerTodo { resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn guardar(&self, destino: String) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Guardar { destino, resp: tx }).map_err(texto)?;
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

fn abrir_doc(ruta: &str) -> Result<(PdfDocument, Info), String> {
    let t = Instant::now();
    let doc = PdfDocument::open(ruta).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
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
    let mut truncada = false;
    'paginas: for n in 0..paginas {
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
            if resultados.len() >= TOPE_RESULTADOS {
                truncada = true;
                break 'paginas;
            }
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
    let escaneado = resultados.is_empty() && !tiene_texto;
    Ok(Busqueda { resultados, escaneado, truncada })
}

fn anotacion_color((rojo, verde, azul): (f32, f32, f32)) -> AnnotationColor {
    AnnotationColor::Rgb { red: rojo, green: verde, blue: azul }
}

/// Resalta, subraya o tacha las líneas de texto que toca el arrastre (a y b en fracción de la página).
fn anotar_texto(
    doc: &mut PdfDocument,
    n: i32,
    tipo: TipoTexto,
    color: (f32, f32, f32),
    a: (f32, f32),
    b: (f32, f32),
) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let palabras = pagina.words(TextExtractOptions::default()).map_err(texto)?;
    if palabras.is_empty() {
        return Err("Este PDF no tiene texto para resaltar. Puedes dibujar a mano.".into());
    }
    // Franja vertical del arrastre; un toque sin mover selecciona la línea que toca.
    let margen = (caja.height() * 0.005).max(1.0);
    let y0 = caja.y0 + a.1.min(b.1) * caja.height() - margen;
    let y1 = caja.y0 + a.1.max(b.1) * caja.height() + margen;
    let mut lineas: std::collections::BTreeMap<(usize, usize), Rect> = std::collections::BTreeMap::new();
    for p in &palabras {
        let r = p.bounds;
        if r.y1 < y0 || r.y0 > y1 {
            continue;
        }
        lineas
            .entry((p.block, p.line))
            .and_modify(|u| {
                u.x0 = u.x0.min(r.x0);
                u.y0 = u.y0.min(r.y0);
                u.x1 = u.x1.max(r.x1);
                u.y1 = u.y1.max(r.y1);
            })
            .or_insert(r);
    }
    if lineas.is_empty() {
        return Err("No se encontró texto en lo que seleccionaste.".into());
    }
    let quads: Vec<Quad> = lineas
        .values()
        .map(|r| Quad {
            ul: Point { x: r.x0, y: r.y0 },
            ur: Point { x: r.x1, y: r.y0 },
            ll: Point { x: r.x0, y: r.y1 },
            lr: Point { x: r.x1, y: r.y1 },
        })
        .collect();
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = match tipo {
        TipoTexto::Resaltar => pdf.add_highlight_annotation(quads),
        TipoTexto::Subrayar => pdf.add_underline_annotation(quads),
        TipoTexto::Tachar => pdf.add_strikeout_annotation(quads),
    }
    .map_err(texto)?;
    annot.set_color(anotacion_color(color)).map_err(texto)?;
    if matches!(tipo, TipoTexto::Resaltar) {
        annot.set_opacity(0.4).map_err(texto)?;
    }
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Dibuja trazos a mano (puntos en fracción de la página); una firma puede tener varios.
fn anotar_tinta(
    doc: &mut PdfDocument,
    n: i32,
    color: (f32, f32, f32),
    grosor: f32,
    trazos: &[Vec<(f32, f32)>],
) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let trazos: Vec<Vec<Point>> = trazos
        .iter()
        .filter(|t| t.len() >= 2)
        .map(|t| {
            t.iter()
                .map(|(fx, fy)| Point {
                    x: caja.x0 + fx.clamp(0.0, 1.0) * caja.width(),
                    y: caja.y0 + fy.clamp(0.0, 1.0) * caja.height(),
                })
                .collect()
        })
        .collect();
    if trazos.is_empty() {
        return Err("El trazo es muy corto.".into());
    }
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_ink_annotation(trazos).map_err(texto)?;
    annot.set_color(anotacion_color(color)).map_err(texto)?;
    annot.set_border_width(grosor.clamp(0.5, 20.0)).map_err(texto)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Nota con el icono en la página; el texto se lee al abrir la nota en cualquier lector.
fn anotar_nota(doc: &mut PdfDocument, n: i32, x: f32, y: f32, contenido: &str) -> Result<i32, String> {
    let contenido = contenido.trim();
    if contenido.is_empty() {
        return Err("Escribe la nota.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let lado = 24.0_f32.min(caja.width() * 0.1);
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width()).min(caja.x1 - lado).max(caja.x0);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height()).min(caja.y1 - lado).max(caja.y0);
    let rect = Rect::new(x0, y0, x0 + lado, y0 + lado);
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_text_annotation(rect, contenido).map_err(texto)?;
    annot.set_color(AnnotationColor::Rgb { red: 1.0, green: 0.8, blue: 0.0 }).map_err(texto)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Cuadro de texto: se abre desde el toque hacia la derecha y abajo, con fondo claro.
fn anotar_caja(doc: &mut PdfDocument, n: i32, x: f32, y: f32, contenido: &str) -> Result<i32, String> {
    let contenido = contenido.trim();
    if contenido.is_empty() {
        return Err("Escribe el texto.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let lineas = envolver(contenido, 38);
    let envuelto = lineas.join("\n");
    let alto = (lineas.len() as f32 * 15.0 + 8.0).min(caja.height());
    let ancho = (caja.width() * 0.6).min(260.0);
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width()).min(caja.x1 - ancho).max(caja.x0);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height()).min(caja.y1 - alto).max(caja.y0);
    let rect = Rect::new(x0, y0, x0 + ancho, y0 + alto);
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_free_text_annotation(rect, &envuelto).map_err(texto)?;
    annot
        .set_default_appearance("Helv", 12.0, Some(AnnotationColor::Rgb { red: 0.0, green: 0.0, blue: 0.0 }))
        .map_err(texto)?;
    annot.set_color(AnnotationColor::Rgb { red: 0.55, green: 0.55, blue: 0.55 }).map_err(texto)?;
    annot.set_interior_color(AnnotationColor::Rgb { red: 1.0, green: 0.98, blue: 0.82 }).map_err(texto)?;
    annot.set_border_width(0.8).map_err(texto)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Sello de texto grande, centrado en el toque (un cuadro de texto con borde y fondo blanco).
fn anotar_sello(
    doc: &mut PdfDocument,
    n: i32,
    x: f32,
    y: f32,
    etiqueta: &str,
    color: (f32, f32, f32),
) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let ancho = (caja.width() * 0.4).min(170.0);
    let alto = 46.0_f32.min(caja.height() * 0.1);
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width() - ancho / 2.0).clamp(caja.x0, caja.x1 - ancho);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height() - alto / 2.0).clamp(caja.y0, caja.y1 - alto);
    let rect = Rect::new(x0, y0, x0 + ancho, y0 + alto);
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_free_text_annotation(rect, etiqueta).map_err(texto)?;
    annot
        .set_default_appearance("Helv", 18.0, Some(anotacion_color(color)))
        .map_err(texto)?;
    annot.set_color(anotacion_color(color)).map_err(texto)?;
    annot.set_interior_color(AnnotationColor::Rgb { red: 1.0, green: 1.0, blue: 1.0 }).map_err(texto)?;
    annot.set_border_width(2.0).map_err(texto)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

fn envolver(texto: &str, ancho: usize) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        if !actual.is_empty() && actual.len() + 1 + palabra.len() > ancho {
            lineas.push(actual);
            actual = String::new();
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.push_str(palabra);
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
}

fn borrar_anotacion(doc: &mut PdfDocument, n: i32, xref: i32) -> Result<(), String> {
    let mut pagina = doc.load_pdf_page(n).map_err(texto)?;
    let encontrada = pagina.annotations().find(|a| a.xref().ok() == Some(xref));
    if let Some(a) = encontrada {
        pagina.delete_annotation(a).map_err(texto)?;
    }
    Ok(())
}

/// Deshace el último cambio de esta sesión.
fn deshacer(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<bool, String> {
    let Some(hecho) = hechas.pop() else {
        return Ok(false);
    };
    match hecho {
        Hecho::Anotacion { n, xref } => borrar_anotacion(doc, n, xref)?,
        Hecho::Campo { n, xref, anterior } => {
            fijar_campo(doc, n, xref, &anterior)?;
        }
    }
    Ok(true)
}

/// Vuelve al estado de la última vez que se guardó.
fn deshacer_todo(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<bool, String> {
    if hechas.is_empty() {
        return Ok(false);
    }
    for hecho in hechas.iter().rev() {
        match hecho {
            Hecho::Anotacion { n, xref } => borrar_anotacion(doc, *n, *xref)?,
            Hecho::Campo { n, xref, anterior } => {
                fijar_campo(doc, *n, *xref, anterior)?;
            }
        }
    }
    hechas.clear();
    Ok(true)
}

/// Todos los campos de formulario del documento, con su caja en fracción de la página.
fn campos_doc(doc: &PdfDocument) -> Result<Vec<Campo>, String> {
    let paginas = doc.page_count().map_err(texto)?;
    let mut campos = Vec::new();
    for n in 0..paginas {
        let pagina = doc.load_page(n).map_err(texto)?;
        let caja = pagina.bounds().map_err(texto)?;
        if caja.width() <= 0.0 || caja.height() <= 0.0 {
            continue;
        }
        let pdf = doc.load_pdf_page(n).map_err(texto)?;
        for w in pdf.widgets() {
            let r = w.annotation().rect().map_err(texto)?;
            campos.push(Campo {
                pagina: n,
                xref: w.xref().map_err(texto)?,
                tipo: format!("{:?}", w.r#type().map_err(texto)?),
                nombre: w.name().map_err(texto)?.unwrap_or_default(),
                valor: w.value().map_err(texto)?.unwrap_or_default(),
                readonly: w.is_readonly().map_err(texto)?,
                x: ((r.x0 - caja.x0) / caja.width()).clamp(0.0, 1.0),
                y: ((r.y0 - caja.y0) / caja.height()).clamp(0.0, 1.0),
                w: ((r.x1 - r.x0) / caja.width()).clamp(0.0, 1.0),
                h: ((r.y1 - r.y0) / caja.height()).clamp(0.0, 1.0),
            });
        }
    }
    Ok(campos)
}

/// Cambia el valor de un campo y devuelve el valor que tenía antes.
fn fijar_campo(doc: &mut PdfDocument, n: i32, xref: i32, valor: &str) -> Result<String, String> {
    let pagina = doc.load_pdf_page(n).map_err(texto)?;
    let mut widget = pagina
        .load_widget(xref)
        .map_err(texto)?
        .ok_or("No se encontró el campo.".to_string())?;
    if widget.is_readonly().map_err(texto)? {
        return Err("Este campo está protegido y no se puede cambiar.".into());
    }
    let anterior = widget.value().map_err(texto)?.unwrap_or_default();
    widget.set_value(doc, valor, true).map_err(texto)?;
    widget.update().map_err(texto)?;
    Ok(anterior)
}

fn guardar_doc(doc: &PdfDocument, destino: &str) -> Result<(), String> {
    let mut opciones = PdfWriteOptions::default();
    opciones.set_compress(true);
    doc.save_with_options(destino, opciones).map_err(texto)
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

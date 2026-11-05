use std::fmt::Display;
use std::fs::File;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;

use mupdf::color::AnnotationColor;
use mupdf::pdf::annotation::{AnnotationTextAlign, PdfAnnotation, PdfAnnotationType};
use mupdf::pdf::document::{PdfDocument, PdfWriteOptions};
use mupdf::pdf::{PdfObject, PdfPage, WidgetType};
use mupdf::point::Point;
use mupdf::quad::Quad;
use mupdf::rect::Rect;
use mupdf::text_page::TextExtractOptions;
use mupdf::{Colorspace, Device, Document, Font, Matrix, Outline, Pixmap};
use serde::Serialize;
use tauri::UriSchemeResponder;
use tauri::http::{Response, Uri, header};

use crate::cache::Cache;

mod anotar;
mod busqueda;
mod capas;
mod formulario;
mod organizar;
mod pintar;

use anotar::*;
use busqueda::*;
use capas::*;
use formulario::*;
use organizar::organizar_y_guardar;
pub use organizar::Hoja;
use pintar::*;

#[derive(Serialize)]
pub struct Info {
    paginas: i32,
    /// Tamaño de la primera página: reserva el espacio de las demás antes de pintarlas.
    ancho: f32,
    alto: f32,
    indice: Vec<Entrada>,
    ms: u128,
    /// Solo un PDF se puede anotar, organizar y guardar; los demás formatos son para leer.
    pdf: bool,
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
    /// Casillas de marcar y de opción: el valor que las deja marcadas. Cada PDF elige el suyo
    /// ("Yes", "On", "1", "F"…); "Off" es siempre desmarcada. Vacío en los demás campos.
    encendido: String,
    /// Caja del campo, en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Anotación que se puede tocar para mover, cambiar o borrar.
#[derive(Serialize)]
pub struct Anotacion {
    pagina: i32,
    xref: i32,
    tipo: String,
    contenido: String,
    rojo: f32,
    verde: f32,
    azul: f32,
    /// Tamaño de letra en textos, o grosor de línea en trazos.
    tamano: f32,
    /// Caja en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Palabra del texto del PDF, para poder seleccionarla y copiarla como en un lector normal.
#[derive(Serialize)]
pub struct Palabra {
    texto: String,
    /// Caja en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Enlace interno del PDF (por ejemplo, una línea del índice de Word): su caja y la página a la que lleva.
#[derive(Serialize)]
pub struct Enlace {
    pagina: i32,
    /// Caja en fracción de la página (0 a 1).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Lo que se guarda para poder deshacer un cambio en una anotación.
#[derive(Clone)]
struct DatosAnotacion {
    tipo: TipoAnot,
    rect: Rect,
    contenido: String,
    color: (f32, f32, f32),
    tamano: f32,
    opacidad: f32,
    trazos: Vec<Vec<Point>>,
    quads: Vec<Quad>,
}

#[derive(Clone, Copy, PartialEq)]
enum TipoAnot {
    Nota,
    Texto,
    Tinta,
    Resaltar,
    Subrayar,
    Tachar,
}

impl TipoAnot {
    fn nombre(self) -> &'static str {
        match self {
            TipoAnot::Nota => "nota",
            TipoAnot::Texto => "texto",
            TipoAnot::Tinta => "tinta",
            TipoAnot::Resaltar => "resaltar",
            TipoAnot::Subrayar => "subrayar",
            TipoAnot::Tachar => "tachar",
        }
    }
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
#[derive(Clone)]
enum Hecho {
    Anotacion { n: i32, xref: i32 },
    /// La anotación se movió o se le cambió el texto, el color o el tamaño.
    Cambio { n: i32, xref: i32, antes: DatosAnotacion },
    /// Se borró: para deshacer se vuelve a crear igual.
    Borrada { n: i32, xref: i32, datos: DatosAnotacion },
    Campo { n: i32, xref: i32, anterior: String },
}

impl Hecho {
    fn cambiar_xref(&mut self, viejo: i32, nuevo: i32) {
        match self {
            Hecho::Anotacion { xref, .. } | Hecho::Cambio { xref, .. } | Hecho::Borrada { xref, .. } => {
                if *xref == viejo {
                    *xref = nuevo;
                }
            }
            Hecho::Campo { .. } => {}
        }
    }
}

enum Pedido {
    /// `nombre` ayuda a saber qué tipo de archivo es cuando la ruta no lo dice (Android).
    Abrir { ruta: String, nombre: String, archivo: Option<File>, resp: Sender<Result<Info, String>> },
    Buscar { texto: String, resp: Sender<Result<Busqueda, String>> },
    CancelarBusqueda,
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
    Redimensionar {
        n: i32,
        xref: i32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        resp: Sender<Result<(), String>>,
    },
    Caja {
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        color: (f32, f32, f32),
        tamano: f32,
        postit: bool,
        resp: Sender<Result<(), String>>,
    },
    Sello {
        n: i32,
        x: f32,
        y: f32,
        texto: String,
        color: (f32, f32, f32),
        tamano: f32,
        resp: Sender<Result<(), String>>,
    },
    Anotaciones { n: i32, resp: Sender<Result<Vec<Anotacion>, String>> },
    Palabras { n: i32, resp: Sender<Result<Vec<Palabra>, String>> },
    Enlaces { n: i32, resp: Sender<Result<Vec<Enlace>, String>> },
    Mover { n: i32, xref: i32, dx: f32, dy: f32, resp: Sender<Result<(), String>> },
    Editar {
        n: i32,
        xref: i32,
        contenido: Option<String>,
        color: Option<(f32, f32, f32)>,
        tamano: Option<f32>,
        /// Sigue lo que se está escribiendo: se une al paso anterior de deshacer en vez de crear otro.
        junto: bool,
        resp: Sender<Result<bool, String>>,
    },
    Borrar { n: i32, xref: i32, resp: Sender<Result<(), String>> },
    Deshacer { resp: Sender<Result<Option<i32>, String>> },
    DeshacerTodo { resp: Sender<Result<bool, String>> },
    Campos { resp: Sender<Result<Vec<Campo>, String>> },
    Campo { n: i32, xref: i32, valor: String, resp: Sender<Result<(), String>> },
    /// Con `hojas`, guarda un PDF nuevo con esas hojas en ese orden (organizar o extraer páginas).
    Guardar { destino: String, hojas: Option<Vec<Hoja>>, resp: Sender<Result<(), String>> },
}

/// MuPDF no se comparte entre hilos: un solo hilo guarda el documento y atiende los pedidos.
/// El documento abierto: un PDF (se puede anotar, organizar y guardar) u otro formato que MuPDF
/// sabe leer (Word, EPUB, TXT, imágenes…), solo para leer, buscar y copiar texto.
enum Abierto {
    Pdf(PdfDocument),
    Otro(Document),
}

impl Abierto {
    fn doc(&self) -> &Document {
        match self {
            Abierto::Pdf(d) => d,
            Abierto::Otro(d) => d,
        }
    }

    fn pdf(&self) -> Result<&PdfDocument, String> {
        match self {
            Abierto::Pdf(d) => Ok(d),
            Abierto::Otro(_) => Err(SOLO_PDF.into()),
        }
    }

    fn pdf_mut(&mut self) -> Result<&mut PdfDocument, String> {
        match self {
            Abierto::Pdf(d) => Ok(d),
            Abierto::Otro(_) => Err(SOLO_PDF.into()),
        }
    }
}

const SOLO_PDF: &str = "Esto solo se puede hacer en un PDF.";

pub struct Motor(Sender<Pedido>);

impl Motor {
    pub fn nuevo() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // En Android el archivo viaja con el documento: si se cerrara, MuPDF perdería el acceso.
            let mut abierto: Option<(Abierto, Option<File>)> = None;
            let mut ruta_abierta = String::new();
            // Anotaciones y campos cambiados en esta sesión, para deshacer.
            let mut hechas: Vec<Hecho> = Vec::new();
            let mut pendientes: Vec<PedidoPagina> = Vec::new();
            let mut busqueda: Option<BusquedaEnCurso> = None;
            let mut cache = Cache::nuevo(TOPE_CACHE);
            loop {
                // Sin trabajo se espera; con trabajo solo se recoge lo que ya llegó.
                let primero = if pendientes.is_empty() && busqueda.is_none() {
                    match rx.recv() {
                        Ok(p) => Some(p),
                        Err(_) => return,
                    }
                } else {
                    None
                };
                for pedido in primero.into_iter().chain(rx.try_iter()) {
                    match pedido {
                        Pedido::Abrir { ruta, nombre, archivo, resp } => {
                            if let Some(b) = busqueda.take() {
                                b.cancelar();
                            }
                            let r = abrir_doc(&ruta, &nombre).map(|(doc, info)| {
                                abierto = Some((doc, archivo));
                                ruta_abierta = ruta;
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
                            if let Some(b) = busqueda.take() {
                                b.cancelar();
                            }
                            match BusquedaEnCurso::nueva(&texto, resp) {
                                Ok(b) => busqueda = Some(b),
                                Err((e, resp)) => {
                                    let _ = resp.send(Err(e));
                                }
                            }
                        }
                        Pedido::CancelarBusqueda => {
                            if let Some(b) = busqueda.take() {
                                b.cancelar();
                            }
                        }
                        Pedido::Texto { n, tipo, color, a, b, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| anotar_texto(doc, n, tipo, color, a, b).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Tinta { n, color, grosor, trazos, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| anotar_tinta(doc, n, color, grosor, &trazos).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Caja { n, x, y, contenido, color, tamano, postit, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| anotar_caja(doc, n, x, y, &contenido, color, tamano, postit).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Sello { n, x, y, texto, color, tamano, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| anotar_sello(doc, n, x, y, &texto, color, tamano).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Anotaciones { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                // Fuera de PDF no hay anotaciones ni formularios: lista vacía.
                                Some((d, _)) => d.pdf().map_or_else(|_| Ok(Vec::new()), |doc| anotaciones_de(doc, n)),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Palabras { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => palabras_de(d.doc(), n),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Enlaces { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => enlaces_de(d.doc(), n),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Mover { n, xref, dx, dy, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| datos_de(doc, n, xref).and_then(|antes| {
                                    mover_anotacion(doc, n, xref, dx, dy).map(|nuevo| {
                                        for h in hechas.iter_mut() {
                                            h.cambiar_xref(xref, nuevo);
                                        }
                                        hechas.push(Hecho::Cambio { n, xref: nuevo, antes });
                                        cache.olvidar_pagina(n);
                                    })
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Editar { n, xref, contenido, color, tamano, junto, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| datos_de(doc, n, xref).and_then(|antes| {
                                    editar_anotacion(doc, n, xref, contenido.as_deref(), color, tamano).map(|nuevo| {
                                        for h in hechas.iter_mut() {
                                            h.cambiar_xref(xref, nuevo);
                                        }
                                        cache.olvidar_pagina(n);
                                        // Mientras se escribe, todo cuenta como un solo paso: deshacer vuelve a como estaba antes de empezar.
                                        let sigue = junto
                                            && matches!(hechas.last(), Some(Hecho::Cambio { n: m, xref: x, .. }) if *m == n && *x == nuevo);
                                        if !sigue {
                                            hechas.push(Hecho::Cambio { n, xref: nuevo, antes });
                                        }
                                        !sigue
                                    })
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Redimensionar { n, xref, x, y, w, h, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| datos_de(doc, n, xref).and_then(|antes| {
                                    redimensionar_anotacion(doc, n, xref, x, y, w, h).map(|()| {
                                        hechas.push(Hecho::Cambio { n, xref, antes });
                                        cache.olvidar_pagina(n);
                                    })
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Borrar { n, xref, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| datos_de(doc, n, xref).and_then(|datos| {
                                    borrar_anotacion(doc, n, xref).map(|()| {
                                        hechas.push(Hecho::Borrada { n, xref, datos });
                                        cache.olvidar_pagina(n);
                                    })
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Campos { resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                // Fuera de PDF no hay anotaciones ni formularios: lista vacía.
                                Some((d, _)) => d.pdf().map_or_else(|_| Ok(Vec::new()), |doc| campos_doc(doc)),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Campo { n, xref, valor, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| fijar_campo(doc, n, xref, &valor).map(|anterior| {
                                    hechas.push(Hecho::Campo { n, xref, anterior });
                                    cache.olvidar_pagina(n);
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Deshacer { resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| deshacer(doc, &mut hechas).map(|pagina| {
                                    if let Some(n) = pagina {
                                        cache.olvidar_pagina(n);
                                    }
                                    pagina
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::DeshacerTodo { resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((d, _)) => d.pdf_mut().and_then(|doc| deshacer_todo(doc, &mut hechas).map(|hecho| {
                                    if hecho {
                                        cache.vaciar();
                                    }
                                    hecho
                                })),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Guardar { destino, hojas, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                // Lo guardado queda como punto de partida: ya no se deshace.
                                Some(_) if mismo_archivo(&ruta_abierta, &destino) => Err(
                                    "No se puede guardar encima del PDF abierto. Elige otro nombre.".into(),
                                ),
                                // Organizar no cambia el PDF abierto: sus cambios siguen pudiendo deshacerse.
                                Some((d, _)) => d.pdf().and_then(|doc| match &hojas {
                                    Some(h) => organizar_y_guardar(doc, h, &destino),
                                    None => guardar_doc(doc, &destino).map(|()| hechas.clear()),
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Pagina(p) => pendientes.push(p),
                    }
                }
                // La búsqueda avanza solo cuando no hay páginas esperando: lo que se ve va primero.
                if pendientes.is_empty() {
                    if let Some(b) = busqueda.as_mut() {
                        let r = match &abierto {
                            None => Err("No hay un documento abierto".into()),
                            Some((d, _)) => b.avanzar(d.doc()),
                        };
                        match r {
                            Ok(false) => {}
                            Ok(true) => {
                                if let Some(b) = busqueda.take() {
                                    b.terminar();
                                }
                            }
                            Err(e) => {
                                if let Some(b) = busqueda.take() {
                                    let _ = b.resp.send(Err(e));
                                }
                            }
                        }
                    }
                    continue;
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
                    (Some((d, _)), Some(recorte)) => pintar_recorte(d.doc(), p.n, recorte),
                    (Some((d, _)), None) => match cache.leer(clave) {
                        Some(jpg) => Ok(jpg),
                        None => pintar(d.doc(), p.n, p.ancho).inspect(|jpg| {
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

    pub fn abrir(&self, ruta: String, nombre: String, archivo: Option<File>) -> Result<Info, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Abrir { ruta, nombre, archivo, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn buscar(&self, aguja: String) -> Result<Busqueda, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Buscar { texto: aguja, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn cancelar_busqueda(&self) {
        let _ = self.0.send(Pedido::CancelarBusqueda);
    }

    pub fn anotar_texto(
        &self,
        n: i32,
        tipo: &str,
        color: (f32, f32, f32),
        a: (f32, f32),
        b: (f32, f32),
    ) -> Result<(), String> {
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

    #[allow(clippy::too_many_arguments)]
    pub fn anotar_caja(
        &self,
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        color: (f32, f32, f32),
        tamano: f32,
        postit: bool,
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Caja { n, x, y, contenido, color, tamano, postit, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_sello(
        &self,
        n: i32,
        x: f32,
        y: f32,
        etiqueta: String,
        color: (f32, f32, f32),
        tamano: f32,
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Sello { n, x, y, texto: etiqueta, color, tamano, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotaciones(&self, n: i32) -> Result<Vec<Anotacion>, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Anotaciones { n, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn palabras(&self, n: i32) -> Result<Vec<Palabra>, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Palabras { n, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn enlaces(&self, n: i32) -> Result<Vec<Enlace>, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Enlaces { n, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn mover_anotacion(&self, n: i32, xref: i32, dx: f32, dy: f32) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Mover { n, xref, dx, dy, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn redimensionar_anotacion(&self, n: i32, xref: i32, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Redimensionar { n, xref, x, y, w, h, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn editar_anotacion(
        &self,
        n: i32,
        xref: i32,
        contenido: Option<String>,
        color: Option<(f32, f32, f32)>,
        tamano: Option<f32>,
        junto: bool,
    ) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Editar { n, xref, contenido, color, tamano, junto, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn borrar_anotacion(&self, n: i32, xref: i32) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Borrar { n, xref, resp: tx }).map_err(texto)?;
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

    pub fn deshacer(&self) -> Result<Option<i32>, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Deshacer { resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn deshacer_todo(&self) -> Result<bool, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::DeshacerTodo { resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn guardar(&self, destino: String, hojas: Option<Vec<Hoja>>) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Guardar { destino, hojas, resp: tx }).map_err(texto)?;
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

/// Tamaño de hoja para los formatos de texto corrido (Word, EPUB, TXT…): cómodo de leer en el celular.
const HOJA_LECTURA: (f32, f32, f32) = (420.0, 600.0, 11.0);

fn abrir_doc(ruta: &str, nombre: &str) -> Result<(Abierto, Info), String> {
    let t = Instant::now();
    let cabecera = leer_cabecera(ruta).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
    let doc = if cabecera.windows(5).any(|w| w == b"%PDF-") {
        let doc = PdfDocument::open(ruta).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
        if doc.needs_password().map_err(texto)? {
            return Err("Este PDF tiene contraseña. Abrir PDFs con contraseña llegará en una próxima versión.".into());
        }
        Abierto::Pdf(doc)
    } else {
        let tipo = tipo_de(nombre, &cabecera).ok_or("Dakarta todavía no sabe abrir este tipo de archivo.")?;
        let bytes = std::fs::read(ruta).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
        let mut doc = Document::from_bytes(&bytes, tipo).map_err(|e| format!("No se pudo abrir este archivo: {e}"))?;
        if doc.is_reflowable().map_err(texto)? {
            let (ancho, alto, letra) = HOJA_LECTURA;
            doc.layout(ancho, alto, letra).map_err(texto)?;
        }
        Abierto::Otro(doc)
    };
    let paginas = doc.doc().page_count().map_err(texto)?;
    if paginas < 1 {
        return Err("El documento no tiene páginas.".into());
    }
    let b = doc.doc().load_page(0).and_then(|p| p.bounds()).map_err(texto)?;
    let indice = doc.doc().outlines().map(|o| entradas(&o)).unwrap_or_default();
    let info = Info {
        paginas,
        ancho: b.width(),
        alto: b.height(),
        indice,
        ms: t.elapsed().as_millis(),
        pdf: matches!(doc, Abierto::Pdf(_)),
        ruta: String::new(),
        nombre: String::new(),
    };
    Ok((doc, info))
}

fn leer_cabecera(ruta: &str) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut c = Vec::with_capacity(1024);
    File::open(ruta)?.take(1024).read_to_end(&mut c)?;
    Ok(c)
}

/// Qué tipo de archivo es (para MuPDF): por la extensión del nombre y, si no la tiene, por sus primeros bytes.
fn tipo_de(nombre: &str, cabecera: &[u8]) -> Option<&'static str> {
    const CONOCIDOS: [&str; 22] = [
        "docx", "xlsx", "pptx", "epub", "mobi", "fb2", "txt", "text", "log", "htm", "html", "xhtml", "xps", "oxps",
        "cbz", "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff",
    ];
    let extension = nombre.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    if let Some(e) = extension.as_deref().and_then(|e| CONOCIDOS.iter().find(|c| **c == e)) {
        return Some(e);
    }
    let empieza = |firma: &[u8]| cabecera.starts_with(firma);
    let contiene = |texto: &[u8]| cabecera.windows(texto.len()).any(|w| w == texto);
    if empieza(b"PK\x03\x04") {
        return Some(if contiene(b"application/epub+zip") {
            "epub"
        } else if contiene(b"[Content_Types].xml") || contiene(b"word/") {
            "docx"
        } else if contiene(b"xl/") {
            "xlsx"
        } else if contiene(b"ppt/") {
            "pptx"
        } else {
            "cbz"
        });
    }
    if empieza(b"\xFF\xD8\xFF") {
        return Some("jpg");
    }
    if empieza(b"\x89PNG") {
        return Some("png");
    }
    if empieza(b"GIF8") {
        return Some("gif");
    }
    if empieza(b"BM") {
        return Some("bmp");
    }
    if empieza(b"II*\0") || empieza(b"MM\0*") {
        return Some("tiff");
    }
    // Texto legible (sin bytes de control raros): se abre como TXT.
    let texto = String::from_utf8_lossy(cabecera);
    let raros = texto.chars().filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')).count();
    (!cabecera.is_empty() && raros == 0 && !texto.contains('\u{FFFD}')).then_some("txt")
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

fn texto(e: impl Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod pruebas;

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
    Abrir { ruta: String, archivo: Option<File>, resp: Sender<Result<Info, String>> },
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
    Nota {
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        color: (f32, f32, f32),
        tamano: f32,
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
        resp: Sender<Result<(), String>>,
    },
    Borrar { n: i32, xref: i32, resp: Sender<Result<(), String>> },
    Deshacer { resp: Sender<Result<Option<i32>, String>> },
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
                        Pedido::Abrir { ruta, archivo, resp } => {
                            if let Some(b) = busqueda.take() {
                                b.cancelar();
                            }
                            let r = abrir_doc(&ruta).map(|(doc, info)| {
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
                                Some((doc, _)) => anotar_texto(doc, n, tipo, color, a, b).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Tinta { n, color, grosor, trazos, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_tinta(doc, n, color, grosor, &trazos).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Nota { n, x, y, contenido, color, tamano, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_nota(doc, n, x, y, &contenido, color, tamano).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Caja { n, x, y, contenido, color, tamano, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_caja(doc, n, x, y, &contenido, color, tamano).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Sello { n, x, y, texto, color, tamano, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotar_sello(doc, n, x, y, &texto, color, tamano).map(|xref| {
                                    hechas.push(Hecho::Anotacion { n, xref });
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Anotaciones { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => anotaciones_de(doc, n),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Palabras { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => palabras_de(doc, n),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Enlaces { n, resp } => {
                            let r = match &abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => enlaces_de(doc, n),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Mover { n, xref, dx, dy, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => datos_de(doc, n, xref).and_then(|antes| {
                                    mover_anotacion(doc, n, xref, dx, dy).map(|nuevo| {
                                        for h in hechas.iter_mut() {
                                            h.cambiar_xref(xref, nuevo);
                                        }
                                        hechas.push(Hecho::Cambio { n, xref: nuevo, antes });
                                        cache.olvidar_pagina(n);
                                    })
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Editar { n, xref, contenido, color, tamano, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => datos_de(doc, n, xref).and_then(|antes| {
                                    editar_anotacion(doc, n, xref, contenido.as_deref(), color, tamano).map(|nuevo| {
                                        for h in hechas.iter_mut() {
                                            h.cambiar_xref(xref, nuevo);
                                        }
                                        hechas.push(Hecho::Cambio { n, xref: nuevo, antes });
                                        cache.olvidar_pagina(n);
                                    })
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Redimensionar { n, xref, x, y, w, h, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => datos_de(doc, n, xref).and_then(|antes| {
                                    redimensionar_anotacion(doc, n, xref, x, y, w, h).map(|()| {
                                        hechas.push(Hecho::Cambio { n, xref, antes });
                                        cache.olvidar_pagina(n);
                                    })
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Borrar { n, xref, resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => datos_de(doc, n, xref).and_then(|datos| {
                                    borrar_anotacion(doc, n, xref).map(|()| {
                                        hechas.push(Hecho::Borrada { n, xref, datos });
                                        cache.olvidar_pagina(n);
                                    })
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
                                    cache.olvidar_pagina(n);
                                }),
                            };
                            let _ = resp.send(r);
                        }
                        Pedido::Deshacer { resp } => {
                            let r = match &mut abierto {
                                None => Err("No hay un documento abierto".into()),
                                Some((doc, _)) => deshacer(doc, &mut hechas).map(|pagina| {
                                    if let Some(n) = pagina {
                                        cache.olvidar_pagina(n);
                                    }
                                    pagina
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
                                Some(_) if mismo_archivo(&ruta_abierta, &destino) => Err(
                                    "No se puede guardar encima del PDF abierto. Elige otro nombre.".into(),
                                ),
                                Some((doc, _)) => guardar_doc(doc, &destino).map(|()| hechas.clear()),
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
                            Some((doc, _)) => b.avanzar(doc),
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

    pub fn anotar_nota(
        &self,
        n: i32,
        x: f32,
        y: f32,
        contenido: String,
        color: (f32, f32, f32),
        tamano: f32,
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Nota { n, x, y, contenido, color, tamano, resp: tx }).map_err(texto)?;
        rx.recv().map_err(texto)?
    }

    pub fn anotar_caja(&self, n: i32, x: f32, y: f32, contenido: String, color: (f32, f32, f32), tamano: f32) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Caja { n, x, y, contenido, color, tamano, resp: tx }).map_err(texto)?;
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
    ) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.0.send(Pedido::Editar { n, xref, contenido, color, tamano, resp: tx }).map_err(texto)?;
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
/// Búsqueda que avanza de a pocas páginas: entre una tanda y otra el motor sigue pintando lo que se ve,
/// y una búsqueda nueva o cerrar el buscador la cancela.
struct BusquedaEnCurso {
    aguja: String,
    siguiente: i32,
    resultados: Vec<Coincidencia>,
    tiene_texto: bool,
    truncada: bool,
    resp: Sender<Result<Busqueda, String>>,
}

/// Páginas que se miran por tanda: pocas, para que una página pedida no espere casi nada.
const PAGINAS_POR_TANDA: i32 = 4;

impl BusquedaEnCurso {
    fn nueva(aguja: &str, resp: Sender<Result<Busqueda, String>>) -> Result<Self, (String, Sender<Result<Busqueda, String>>)> {
        let aguja = aguja.trim();
        if aguja.is_empty() {
            return Err(("Escribe qué buscar.".into(), resp));
        }
        Ok(Self { aguja: aguja.to_string(), siguiente: 0, resultados: Vec::new(), tiene_texto: false, truncada: false, resp })
    }

    /// Mira la siguiente tanda de páginas. Devuelve true cuando ya terminó.
    fn avanzar(&mut self, doc: &Document) -> Result<bool, String> {
        let paginas = doc.page_count().map_err(texto)?;
        let fin = (self.siguiente + PAGINAS_POR_TANDA).min(paginas);
        while self.siguiente < fin {
            let n = self.siguiente;
            self.siguiente += 1;
            let pagina = doc.load_page(n).map_err(texto)?;
            let b = pagina.bounds().map_err(texto)?;
            if b.width() <= 0.0 || b.height() <= 0.0 {
                continue;
            }
            // Solo se mira si hay texto mientras no se haya encontrado: el aviso de escaneado no cuesta en PDFs normales.
            if !self.tiene_texto {
                self.tiene_texto = !pagina.text(TextExtractOptions::default()).map_err(texto)?.trim().is_empty();
            }
            for q in pagina.search(&self.aguja, 500).map_err(texto)?.iter() {
                if self.resultados.len() >= TOPE_RESULTADOS {
                    self.truncada = true;
                    return Ok(true);
                }
                let x0 = q.ul.x.min(q.ll.x).min(q.ur.x).min(q.lr.x);
                let x1 = q.ul.x.max(q.ll.x).max(q.ur.x).max(q.lr.x);
                let y0 = q.ul.y.min(q.ll.y).min(q.ur.y).min(q.lr.y);
                let y1 = q.ul.y.max(q.ll.y).max(q.ur.y).max(q.lr.y);
                self.resultados.push(Coincidencia {
                    pagina: n,
                    x: ((x0 - b.x0) / b.width()).clamp(0.0, 1.0),
                    y: ((y0 - b.y0) / b.height()).clamp(0.0, 1.0),
                    w: ((x1 - x0) / b.width()).clamp(0.0, 1.0),
                    h: ((y1 - y0) / b.height()).clamp(0.0, 1.0),
                });
            }
        }
        Ok(self.siguiente >= paginas)
    }

    fn terminar(self) {
        let escaneado = self.resultados.is_empty() && !self.tiene_texto;
        let _ = self.resp.send(Ok(Busqueda { resultados: self.resultados, escaneado, truncada: self.truncada }));
    }

    fn cancelar(self) {
        let _ = self.resp.send(Err("Búsqueda cancelada".into()));
    }
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
    // Solo las palabras que toca el dedo: un toque elige una; un arrastre elige las que cruza.
    let x0 = caja.x0 + a.0.min(b.0) * caja.width();
    let x1 = caja.x0 + a.0.max(b.0) * caja.width();
    let y0 = caja.y0 + a.1.min(b.1) * caja.height();
    let y1 = caja.y0 + a.1.max(b.1) * caja.height();
    let margen = (caja.height() * 0.005).max(1.0);
    let toque = (x1 - x0) < 3.0 && (y1 - y0) < 3.0;
    let quad = |r: Rect| Quad {
        ul: Point { x: r.x0, y: r.y0 },
        ur: Point { x: r.x1, y: r.y0 },
        ll: Point { x: r.x0, y: r.y1 },
        lr: Point { x: r.x1, y: r.y1 },
    };
    let mut quads: Vec<Quad> = palabras
        .iter()
        .map(|p| p.bounds)
        .filter(|r| {
            if toque {
                r.x0 - margen <= x0 && x0 <= r.x1 + margen && r.y0 - margen <= y0 && y0 <= r.y1 + margen
            } else {
                r.x1 >= x0 && r.x0 <= x1 && r.y1 >= y0 - margen && r.y0 <= y1 + margen
            }
        })
        .map(|r| quad(r))
        .collect();
    // Un toque justo entre dos palabras elige la más cercana de esa línea.
    if toque && quads.is_empty() {
        let cercana = palabras
            .iter()
            .filter(|p| y0 >= p.bounds.y0 - margen && y0 <= p.bounds.y1 + margen)
            .map(|p| {
                let r = p.bounds;
                let distancia = if x0 < r.x0 { r.x0 - x0 } else if x0 > r.x1 { x0 - r.x1 } else { 0.0 };
                (distancia, r)
            })
            .filter(|(distancia, _)| *distancia <= 20.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, r)) = cercana {
            quads.push(quad(r));
        }
    }
    if quads.is_empty() {
        return Err("No se encontró texto en lo que seleccionaste.".into());
    }
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
    // Subrayar y tachar no admiten grosor propio: MuPDF lo calcula del alto de la palabra.
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
fn anotar_nota(
    doc: &mut PdfDocument,
    n: i32,
    x: f32,
    y: f32,
    contenido: &str,
    color: (f32, f32, f32),
    tamano: f32,
) -> Result<i32, String> {
    let contenido = contenido.trim();
    if contenido.is_empty() {
        return Err("Escribe la nota.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    // Un ícono más grande se puede tocar con el dedo y se ve mejor.
    let lado = tamano.clamp(16.0, 96.0).min(caja.width() * 0.3);
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width()).min(caja.x1 - lado).max(caja.x0);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height()).min(caja.y1 - lado).max(caja.y0);
    let rect = Rect::new(x0, y0, x0 + lado, y0 + lado);
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_text_annotation(rect, contenido).map_err(texto)?;
    annot.set_color(anotacion_color(color)).map_err(texto)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Cuadro de texto: se abre desde el toque hacia la derecha y abajo.
fn anotar_caja(
    doc: &mut PdfDocument,
    n: i32,
    x: f32,
    y: f32,
    contenido: &str,
    color: (f32, f32, f32),
    tamano: f32,
) -> Result<i32, String> {
    let contenido = contenido.trim();
    if contenido.is_empty() {
        return Err("Escribe el texto.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let tamano = tamano.clamp(LETRA_MIN, 72.0);
    let x0 = caja.x0 + x.clamp(0.0, 1.0) * caja.width();
    let y0 = caja.y0 + y.clamp(0.0, 1.0) * caja.height();
    let rect = caja_justa(x0, y0, contenido, tamano, caja)?;
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_free_text_annotation(rect, contenido).map_err(texto)?;
    annot
        .set_default_appearance("Helv", tamano, Some(anotacion_color(color)))
        .map_err(texto)?;
    poner_borde(&mut annot, tamano, rect)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Sello de texto grande, centrado en el toque.
fn anotar_sello(
    doc: &mut PdfDocument,
    n: i32,
    x: f32,
    y: f32,
    etiqueta: &str,
    color: (f32, f32, f32),
    tamano: f32,
) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Err("La página está vacía.".into());
    }
    let tamano = tamano.clamp(8.0, 72.0);
    // El recuadro queda pegado a la etiqueta; se puede agrandar arrastrando su esquina.
    let (ancho, alto) = medida_texto(etiqueta, tamano, caja.width() * 0.9)?;
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width() - ancho / 2.0).min(caja.x1 - ancho).max(caja.x0);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height() - alto / 2.0).min(caja.y1 - alto).max(caja.y0);
    let rect = Rect::new(x0, y0, x0 + ancho, y0 + alto);
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut annot = pdf.add_free_text_annotation(rect, etiqueta).map_err(texto)?;
    annot
        .set_default_appearance("Helv", tamano, Some(anotacion_color(color)))
        .map_err(texto)?;
    annot.set_quadding(AnnotationTextAlign::Center).map_err(texto)?;
    poner_borde(&mut annot, tamano, rect)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

fn color_de(color: Option<AnnotationColor>, por_defecto: (f32, f32, f32)) -> (f32, f32, f32) {
    match color {
        Some(AnnotationColor::Rgb { red, green, blue }) => (red, green, blue),
        Some(AnnotationColor::Gray(g)) => (g, g, g),
        Some(AnnotationColor::Cmyk { cyan, magenta, yellow, key }) => {
            let k = 1.0 - key;
            ((1.0 - cyan) * k, (1.0 - magenta) * k, (1.0 - yellow) * k)
        }
        None => por_defecto,
    }
}

/// Lee una anotación de las que Dakarta sabe mover y editar.
fn leer_datos(a: &PdfAnnotation) -> Result<Option<DatosAnotacion>, String> {
    let tipo = match a.r#type().map_err(texto)? {
        PdfAnnotationType::Text => TipoAnot::Nota,
        PdfAnnotationType::FreeText => TipoAnot::Texto,
        PdfAnnotationType::Ink => TipoAnot::Tinta,
        PdfAnnotationType::Highlight => TipoAnot::Resaltar,
        PdfAnnotationType::Underline => TipoAnot::Subrayar,
        PdfAnnotationType::StrikeOut => TipoAnot::Tachar,
        _ => return Ok(None),
    };
    let trazos = if tipo == TipoAnot::Tinta { a.ink_list().map_err(texto)? } else { Vec::new() };
    if tipo == TipoAnot::Tinta && trazos.is_empty() {
        return Ok(None);
    }
    // MuPDF no da el Rect de una tinta: su caja sale de los trazos.
    let rect = if tipo == TipoAnot::Tinta { rect_de_trazos(&trazos) } else { a.rect().map_err(texto)? };
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Ok(None);
    }
    let contenido = a.contents().map_err(texto)?.unwrap_or("").to_string();
    let por_defecto = match tipo {
        TipoAnot::Nota | TipoAnot::Resaltar => (1.0, 0.8, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    // En FreeText, el color C es el relleno del cuadro; el color de las letras está en la apariencia.
    let apariencia = if tipo == TipoAnot::Texto { a.default_appearance().map_err(texto)? } else { None };
    let color = if tipo == TipoAnot::Texto {
        apariencia
            .as_ref()
            .and_then(|d| d.color)
            .map_or((0.0, 0.0, 0.0), |c| color_de(Some(c), (0.0, 0.0, 0.0)))
    } else {
        color_de(a.color().map_err(texto)?, por_defecto)
    };
    let tamano = match tipo {
        TipoAnot::Nota => rect.width(),
        TipoAnot::Texto => apariencia.as_ref().map(|d| d.size).unwrap_or(12.0),
        _ => a.border_width().map_err(texto).unwrap_or(1.0),
    };
    let opacidad = a.opacity().map_err(texto).unwrap_or(1.0);
    let quads = match tipo {
        TipoAnot::Resaltar | TipoAnot::Subrayar | TipoAnot::Tachar => a.quads().map_err(texto)?,
        _ => Vec::new(),
    };
    Ok(Some(DatosAnotacion { tipo, rect, contenido, color, tamano, opacidad, trazos, quads }))
}

/// Caja de una anotación de tinta: la que ocupan sus trazos, con un punto de margen para poder tocarla.
fn rect_de_trazos(trazos: &[Vec<Point>]) -> Rect {
    let mut puntos = trazos.iter().flatten();
    let Some(p) = puntos.next() else {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    };
    let (mut x0, mut y0, mut x1, mut y1) = (p.x, p.y, p.x, p.y);
    for p in puntos {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect::new(x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0)
}

fn datos_de(doc: &PdfDocument, n: i32, xref: i32) -> Result<DatosAnotacion, String> {
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let a = pdf
        .annotations()
        .find(|a| a.xref().ok() == Some(xref))
        .ok_or("No se encontró la anotación.")?;
    leer_datos(&a)?.ok_or_else(|| "Esta anotación no se puede cambiar.".into())
}

/// Anotaciones editables de una página, con su caja en fracción de la página.
fn anotaciones_de(doc: &PdfDocument, n: i32) -> Result<Vec<Anotacion>, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Ok(Vec::new());
    }
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut lista = Vec::new();
    for a in pdf.annotations() {
        let Ok(Some(d)) = leer_datos(&a) else { continue };
        let Ok(xref) = a.xref() else { continue };
        let r = d.rect;
        lista.push(Anotacion {
            pagina: n,
            xref,
            tipo: d.tipo.nombre().into(),
            contenido: d.contenido,
            rojo: d.color.0,
            verde: d.color.1,
            azul: d.color.2,
            tamano: d.tamano,
            x: ((r.x0 - caja.x0) / caja.width()).clamp(0.0, 1.0),
            y: ((r.y0 - caja.y0) / caja.height()).clamp(0.0, 1.0),
            w: ((r.x1 - r.x0) / caja.width()).clamp(0.0, 1.0),
            h: ((r.y1 - r.y0) / caja.height()).clamp(0.0, 1.0),
        });
    }
    Ok(lista)
}

/// Enlaces de una página que llevan a otra página del mismo PDF. Los que van a internet se ignoran.
fn enlaces_de(doc: &PdfDocument, n: i32) -> Result<Vec<Enlace>, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Ok(Vec::new());
    }
    let total = doc.page_count().map_err(texto)?;
    let mut lista = Vec::new();
    for l in pagina.links().map_err(texto)? {
        let Some(d) = l.dest else { continue };
        let destino = d.loc.page_number as i32;
        if destino < 0 || destino >= total {
            continue;
        }
        let r = l.bounds;
        lista.push(Enlace {
            pagina: destino,
            x: ((r.x0 - caja.x0) / caja.width()).clamp(0.0, 1.0),
            y: ((r.y0 - caja.y0) / caja.height()).clamp(0.0, 1.0),
            w: ((r.x1 - r.x0) / caja.width()).clamp(0.0, 1.0),
            h: ((r.y1 - r.y0) / caja.height()).clamp(0.0, 1.0),
        });
    }
    Ok(lista)
}

/// Palabras de una página con su caja, para la capa que permite seleccionar y copiar.
fn palabras_de(doc: &PdfDocument, n: i32) -> Result<Vec<Palabra>, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    if caja.width() <= 0.0 || caja.height() <= 0.0 {
        return Ok(Vec::new());
    }
    let palabras = pagina.words(TextExtractOptions::default()).map_err(texto)?;
    let mut lista = Vec::with_capacity(palabras.len());
    for (i, p) in palabras.iter().enumerate() {
        if p.text.is_empty() {
            continue;
        }
        let fin_linea = palabras.get(i + 1).map_or(true, |s| s.block != p.block || s.line != p.line);
        let r = p.bounds;
        lista.push(Palabra {
            // El espacio o el salto de línea hacen que al copiar quede separado.
            texto: format!("{}{}", p.text, if fin_linea { "\n" } else { " " }),
            x: ((r.x0 - caja.x0) / caja.width()).clamp(0.0, 1.0),
            y: ((r.y0 - caja.y0) / caja.height()).clamp(0.0, 1.0),
            w: ((r.x1 - r.x0) / caja.width()).clamp(0.0, 1.0),
            h: ((r.y1 - r.y0) / caja.height()).clamp(0.0, 1.0),
        });
    }
    Ok(lista)
}

fn aplicar_datos(a: &mut PdfAnnotation, d: &DatosAnotacion) -> Result<(), String> {
    // La tinta no tiene Rect en MuPDF: su caja sale de los trazos, que ya vienen movidos o escalados.
    if d.tipo != TipoAnot::Tinta {
        a.set_rect(d.rect).map_err(texto)?;
    }
    match d.tipo {
        TipoAnot::Nota => {
            a.set_contents(&d.contenido).map_err(texto)?;
            a.set_color(anotacion_color(d.color)).map_err(texto)?;
        }
        TipoAnot::Texto => {
            a.set_contents(&d.contenido).map_err(texto)?;
            // El color de una FreeText se usa como relleno del cuadro: aquí solo va el color del texto.
            a.set_default_appearance("Helv", d.tamano, Some(anotacion_color(d.color))).map_err(texto)?;
            poner_borde(a, d.tamano, d.rect)?;
        }
        TipoAnot::Tinta => {
            a.set_color(anotacion_color(d.color)).map_err(texto)?;
            a.set_border_width(d.tamano).map_err(texto)?;
            a.set_ink_list(d.trazos.clone()).map_err(texto)?;
        }
        TipoAnot::Resaltar => {
            a.set_color(anotacion_color(d.color)).map_err(texto)?;
            a.set_quad_points(d.quads.clone()).map_err(texto)?;
            a.set_opacity(d.opacidad.clamp(0.1, 1.0)).map_err(texto)?;
        }
        TipoAnot::Subrayar | TipoAnot::Tachar => {
            a.set_color(anotacion_color(d.color)).map_err(texto)?;
            a.set_quad_points(d.quads.clone()).map_err(texto)?;
        }
    }
    a.update().map_err(texto)?;
    Ok(())
}

fn crear_anotacion(doc: &mut PdfDocument, n: i32, d: &DatosAnotacion) -> Result<i32, String> {
    let mut pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut a = match d.tipo {
        TipoAnot::Nota => pdf.add_text_annotation(d.rect, &d.contenido).map_err(texto)?,
        TipoAnot::Texto => pdf.add_free_text_annotation(d.rect, &d.contenido).map_err(texto)?,
        TipoAnot::Tinta => pdf.add_ink_annotation(d.trazos.clone()).map_err(texto)?,
        TipoAnot::Resaltar => pdf.add_highlight_annotation(d.quads.clone()).map_err(texto)?,
        TipoAnot::Subrayar => pdf.add_underline_annotation(d.quads.clone()).map_err(texto)?,
        TipoAnot::Tachar => pdf.add_strikeout_annotation(d.quads.clone()).map_err(texto)?,
    };
    aplicar_datos(&mut a, d)?;
    a.xref().map_err(texto)
}

fn fijar(v: f32, min: f32, max: f32) -> f32 {
    if min > max { v } else { v.clamp(min, max) }
}

fn mover_anotacion(doc: &mut PdfDocument, n: i32, xref: i32, dx: f32, dy: f32) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    let mut d = datos_de(doc, n, xref)?;
    let dx = fijar(dx * caja.width(), caja.x0 - d.rect.x0, caja.x1 - d.rect.x1);
    let dy = fijar(dy * caja.height(), caja.y0 - d.rect.y0, caja.y1 - d.rect.y1);
    d.rect = d.rect.translate(dx, dy);
    if d.tipo == TipoAnot::Tinta {
        for trazo in &mut d.trazos {
            for p in trazo {
                p.x += dx;
                p.y += dy;
            }
        }
    }
    if matches!(d.tipo, TipoAnot::Resaltar | TipoAnot::Subrayar | TipoAnot::Tachar) {
        for q in &mut d.quads {
            for p in [&mut q.ul, &mut q.ur, &mut q.ll, &mut q.lr] {
                p.x += dx;
                p.y += dy;
            }
        }
    }
    // Se mueve la misma anotación: conserva su letra, su autor, el centrado del sello y lo demás.
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut a = pdf
        .annotations()
        .find(|a| a.xref().ok() == Some(xref))
        .ok_or("No se encontró la anotación.")?;
    match d.tipo {
        TipoAnot::Tinta => a.set_ink_list(d.trazos.clone()).map_err(texto)?,
        TipoAnot::Resaltar | TipoAnot::Subrayar | TipoAnot::Tachar => a.set_quad_points(d.quads.clone()).map_err(texto)?,
        TipoAnot::Nota | TipoAnot::Texto => a.set_rect(d.rect).map_err(texto)?,
    }
    a.update().map_err(texto)?;
    Ok(xref)
}

/// Letra más pequeña de un texto: más chico ya no se lee ni se puede tocar con el dedo.
const LETRA_MIN: f32 = 8.0;
/// MuPDF pone la primera línea pegada al borde de arriba y corta lo que sobresale: la tilde de la Á o la
/// virgulilla de la Ñ. Un borde invisible de este grosor baja el texto lo justo para que quepan.
fn borde_de(tamano: f32) -> f32 {
    (tamano * 0.14).max(1.0)
}

/// Borde invisible (trazos de largo cero) que deja sitio arriba para las tildes de las mayúsculas.
/// MuPDF agranda la caja al cambiar el borde: se vuelve a poner la caja justa.
fn poner_borde(a: &mut PdfAnnotation, tamano: f32, rect: Rect) -> Result<(), String> {
    a.set_border_width(borde_de(tamano)).map_err(texto)?;
    a.set_border_dash_pattern(&[0.0, 1000.0]).map_err(texto)?;
    a.set_rect(rect).map_err(texto)
}

fn ancho_de(fuente: &Font, s: &str, tamano: f32) -> f32 {
    let em: f32 = s
        .chars()
        .map(|c| fuente.encode_character(c as i32).and_then(|g| fuente.advance_glyph(g)).unwrap_or(0.6))
        .sum();
    em * tamano
}

/// Ancho y alto justos de un texto con la letra del cuadro (Helvetica). Corta las líneas en los
/// espacios, igual que MuPDF al dibujarlo, para no pasar de `max_ancho`; los Enter se respetan.
fn medida_texto(contenido: &str, tamano: f32, max_ancho: f32) -> Result<(f32, f32), String> {
    let fuente = Font::new("Helvetica").map_err(texto)?;
    // MuPDF deja de margen el doble del borde, a cada lado: 4 bordes por eje.
    let marco = 4.0 * borde_de(tamano);
    let limite = (max_ancho - marco).max(tamano);
    let (mut ancho, mut lineas) = (0.0f32, 0u32);
    for parrafo in contenido.lines() {
        let mut actual = String::new();
        for palabra in parrafo.split(' ') {
            let junto = if actual.is_empty() { palabra.to_string() } else { format!("{actual} {palabra}") };
            if !actual.is_empty() && ancho_de(&fuente, &junto, tamano) > limite {
                ancho = ancho.max(ancho_de(&fuente, &actual, tamano));
                lineas += 1;
                actual = palabra.to_string();
            } else {
                actual = junto;
            }
        }
        ancho = ancho.max(ancho_de(&fuente, &actual, tamano));
        lineas += 1;
    }
    let lineas = lineas.max(1) as f32;
    // Un poco de holgura: así MuPDF nunca corta una línea que aquí sí cupo.
    Ok((ancho + marco + 2.0, tamano + (lineas - 1.0) * tamano * 1.2 + marco + 1.0))
}

/// Cuadro pegado al texto con su esquina en (x0, y0), sin salirse de la hoja.
fn caja_justa(x0: f32, y0: f32, contenido: &str, tamano: f32, caja: Rect) -> Result<Rect, String> {
    let (ancho, alto) = medida_texto(contenido, tamano, caja.width() * 0.9)?;
    let x0 = x0.min(caja.x1 - ancho).max(caja.x0);
    let y0 = y0.min(caja.y1 - alto).max(caja.y0);
    Ok(Rect::new(x0, y0, x0 + ancho, y0 + alto))
}

fn editar_anotacion(
    doc: &mut PdfDocument,
    n: i32,
    xref: i32,
    contenido: Option<&str>,
    color: Option<(f32, f32, f32)>,
    tamano: Option<f32>,
) -> Result<i32, String> {
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    let mut d = datos_de(doc, n, xref)?;
    if let Some(c) = contenido {
        let c = c.trim();
        if c.is_empty() {
            return Err("Escribe el texto.".into());
        }
        if matches!(d.tipo, TipoAnot::Nota | TipoAnot::Texto) {
            d.contenido = c.to_string();
        }
    }
    if let Some(c) = color {
        d.color = c;
    }
    if let Some(t) = tamano {
        d.tamano = match d.tipo {
            TipoAnot::Nota => t.clamp(16.0, 96.0),
            TipoAnot::Texto => t.clamp(LETRA_MIN, 72.0),
            TipoAnot::Tinta | TipoAnot::Subrayar | TipoAnot::Tachar => t.clamp(0.5, 20.0),
            _ => d.tamano,
        };
    }
    if d.tipo == TipoAnot::Nota {
        d.rect = Rect::new(d.rect.x0, d.rect.y0, (d.rect.x0 + d.tamano).min(caja.x1), (d.rect.y0 + d.tamano).min(caja.y1));
    }
    // El cuadro se recalcula solo si cambió el texto o la letra: un cambio de color no borra el tamaño elegido.
    if d.tipo == TipoAnot::Texto && (contenido.is_some() || tamano.is_some()) {
        d.rect = caja_justa(d.rect.x0, d.rect.y0, &d.contenido, d.tamano, caja)?;
    }
    // Se cambia la misma anotación: conserva su número y el panel puede seguir aplicando cambios.
    editar_en_pagina(doc, n, xref, &d)?;
    Ok(xref)
}

fn editar_en_pagina(doc: &mut PdfDocument, n: i32, xref: i32, d: &DatosAnotacion) -> Result<(), String> {
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut a = pdf
        .annotations()
        .find(|a| a.xref().ok() == Some(xref))
        .ok_or("No se encontró la anotación.")?;
    aplicar_datos(&mut a, d)
}

/// Cambia el tamaño de la caja (en fracción de la página). La letra y los trazos acompañan al cuadro.
fn redimensionar_anotacion(doc: &mut PdfDocument, n: i32, xref: i32, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
    if !(w > 0.0) || !(h > 0.0) {
        return Err("El tamaño no es válido.".into());
    }
    let pagina = doc.load_page(n).map_err(texto)?;
    let caja = pagina.bounds().map_err(texto)?;
    let mut d = datos_de(doc, n, xref)?;
    if matches!(d.tipo, TipoAnot::Resaltar | TipoAnot::Subrayar | TipoAnot::Tachar) {
        return Err("Esta anotación no se puede agrandar.".into());
    }
    let viejo = d.rect;
    let ancho = fijar(w * caja.width(), 4.0, caja.width());
    let alto = fijar(h * caja.height(), 4.0, caja.height());
    let x0 = (caja.x0 + x.clamp(0.0, 1.0) * caja.width()).min(caja.x1 - ancho).max(caja.x0);
    let y0 = (caja.y0 + y.clamp(0.0, 1.0) * caja.height()).min(caja.y1 - alto).max(caja.y0);
    let nuevo = Rect::new(x0, y0, x0 + ancho, y0 + alto);
    let sx = if viejo.width() > 0.0 { nuevo.width() / viejo.width() } else { 1.0 };
    let sy = if viejo.height() > 0.0 { nuevo.height() / viejo.height() } else { 1.0 };
    let factor = ((sx + sy) / 2.0).clamp(0.2, 5.0);
    match d.tipo {
        TipoAnot::Tinta => {
            for trazo in &mut d.trazos {
                for p in trazo {
                    p.x = nuevo.x0 + (p.x - viejo.x0) * sx;
                    p.y = nuevo.y0 + (p.y - viejo.y0) * sy;
                }
            }
            d.tamano = (d.tamano * factor).clamp(0.5, 20.0);
        }
        TipoAnot::Texto => d.tamano = (d.tamano * factor).clamp(LETRA_MIN, 72.0),
        _ => {}
    }
    // La letra acompaña al cuadro y el cuadro vuelve a quedar pegado a la letra.
    d.rect = if d.tipo == TipoAnot::Texto { caja_justa(nuevo.x0, nuevo.y0, &d.contenido, d.tamano, caja)? } else { nuevo };
    editar_en_pagina(doc, n, xref, &d)
}

fn borrar_anotacion(doc: &mut PdfDocument, n: i32, xref: i32) -> Result<(), String> {
    let mut pagina = doc.load_pdf_page(n).map_err(texto)?;
    let encontrada = pagina.annotations().find(|a| a.xref().ok() == Some(xref));
    if let Some(a) = encontrada {
        pagina.delete_annotation(a).map_err(texto)?;
    }
    Ok(())
}

/// Deshace el último cambio de esta sesión y devuelve la página que hay que volver a pintar.
fn deshacer(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<Option<i32>, String> {
    // Si deshacer falla, el paso se queda en la lista: se puede volver a intentar y el contador no se descuadra.
    let Some(hecho) = hechas.last().cloned() else {
        return Ok(None);
    };
    let n = pagina_de(&hecho);
    deshacer_hecho(doc, hecho, hechas)?;
    hechas.pop();
    Ok(Some(n))
}

/// Vuelve al estado de la última vez que se guardó.
fn deshacer_todo(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<bool, String> {
    if hechas.is_empty() {
        return Ok(false);
    }
    while let Some(hecho) = hechas.last().cloned() {
        deshacer_hecho(doc, hecho, hechas)?;
        hechas.pop();
    }
    Ok(true)
}

fn pagina_de(hecho: &Hecho) -> i32 {
    match hecho {
        Hecho::Anotacion { n, .. } | Hecho::Cambio { n, .. } | Hecho::Borrada { n, .. } | Hecho::Campo { n, .. } => *n,
    }
}

fn deshacer_hecho(doc: &mut PdfDocument, hecho: Hecho, hechas: &mut Vec<Hecho>) -> Result<(), String> {
    match hecho {
        Hecho::Anotacion { n, xref } => borrar_anotacion(doc, n, xref),
        // Se devuelve la misma anotación a como estaba: no se rehace ni cambia de número.
        Hecho::Cambio { n, xref, antes } => editar_en_pagina(doc, n, xref, &antes),
        Hecho::Borrada { n, xref, datos } => {
            let nuevo = crear_anotacion(doc, n, &datos)?;
            // Las acciones anteriores apuntan al número viejo: ahora es este.
            for h in hechas.iter_mut() {
                h.cambiar_xref(xref, nuevo);
            }
            Ok(())
        }
        Hecho::Campo { n, xref, anterior } => fijar_campo(doc, n, xref, &anterior).map(|_| ()),
    }
}

/// El nombre del estado "marcada" de una casilla: el que no es "Off" entre sus apariencias (/AP /N).
fn valor_encendido(a: &PdfAnnotation) -> String {
    let leer = || -> Result<Option<String>, mupdf::Error> {
        let Some(ap) = a.object().get_dict("AP")? else { return Ok(None) };
        let Some(normal) = ap.get_dict("N")? else { return Ok(None) };
        for i in 0..normal.dict_len()? as i32 {
            if let Some(k) = normal.get_dict_key(i)? {
                let nombre = String::from_utf8_lossy(&k.as_name()?).into_owned();
                if nombre != "Off" {
                    return Ok(Some(nombre));
                }
            }
        }
        Ok(None)
    };
    leer().ok().flatten().unwrap_or_else(|| "Yes".into())
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
            let tipo = w.r#type().map_err(texto)?;
            // Los botones de acción (Imprimir, Enviar…) no guardan ningún valor: no se pueden llenar.
            if tipo == WidgetType::Button {
                continue;
            }
            let encendido = if matches!(tipo, WidgetType::Checkbox | WidgetType::RadioButton) {
                valor_encendido(w.annotation())
            } else {
                String::new()
            };
            let r = w.annotation().rect().map_err(texto)?;
            campos.push(Campo {
                pagina: n,
                xref: w.xref().map_err(texto)?,
                tipo: format!("{tipo:?}"),
                nombre: w.name().map_err(texto)?.unwrap_or_default(),
                valor: w.value().map_err(texto)?.unwrap_or_default(),
                readonly: w.is_readonly().map_err(texto)?,
                encendido,
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
    if widget.r#type().map_err(texto)? == WidgetType::RadioButton {
        let grupo = widget.name().map_err(texto)?;
        let elegido = widget.value().map_err(texto)?.unwrap_or_default();
        apagar_las_demas(&pagina, grupo.as_deref(), &elegido).map_err(texto)?;
    }
    Ok(anterior)
}

/// MuPDF solo cambia la opción que se toca: las demás del grupo seguían viéndose marcadas.
/// Cada una queda marcada solo si su valor es el elegido.
fn apagar_las_demas(pagina: &PdfPage, grupo: Option<&str>, elegido: &str) -> Result<(), mupdf::Error> {
    for w in pagina.widgets() {
        if w.r#type()? != WidgetType::RadioButton || w.name()?.as_deref() != grupo {
            continue;
        }
        let propio = valor_encendido(w.annotation());
        let estado = if propio == elegido { propio.as_str() } else { "Off" };
        let mut a = w.into_annotation();
        a.object().dict_put("AS", PdfObject::new_name(estado)?)?;
        // Volver a poner la misma caja avisa a MuPDF que cambió y debe redibujarla.
        let caja = a.rect()?;
        a.set_rect(caja)?;
        a.update()?;
    }
    Ok(())
}

/// Se escribe primero a un archivo aparte y solo al terminar reemplaza al destino:
/// si algo falla a medias, el archivo que ya existía queda intacto.
fn guardar_doc(doc: &PdfDocument, destino: &str) -> Result<(), String> {
    let mut opciones = PdfWriteOptions::default();
    opciones.set_compress(true);
    // Quita del archivo lo que ya no se usa (como una anotación borrada): no queda escondido en la copia.
    opciones.set_garbage_level(1);
    let temporal = format!("{destino}.guardando");
    let r = doc
        .save_with_options(&temporal, opciones)
        .map_err(texto)
        .and_then(|()| std::fs::rename(&temporal, destino).map_err(texto));
    if r.is_err() {
        let _ = std::fs::remove_file(&temporal);
    }
    r
}

/// El PDF abierto se sigue leyendo mientras se guarda: escribir encima lo dañaría.
fn mismo_archivo(abierto: &str, destino: &str) -> bool {
    match (std::fs::canonicalize(abierto), std::fs::canonicalize(destino)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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

#[cfg(test)]
mod pruebas {
    use super::*;

    fn doc_de(ancho: f32, alto: f32) -> PdfDocument {
        let mut doc = PdfDocument::new();
        doc.new_page((ancho, alto)).unwrap();
        doc
    }

    #[test]
    fn un_sello_en_una_hoja_muy_pequena_no_detiene_el_motor() {
        let mut doc = doc_de(40.0, 20.0);
        assert!(anotar_sello(&mut doc, 0, 0.5, 0.5, "APROBADO", (1.0, 0.0, 0.0), 40.0).is_ok());
    }

    #[test]
    fn no_se_guarda_encima_del_pdf_abierto_y_lo_guardado_se_vuelve_a_abrir() {
        let carpeta = std::env::temp_dir().join(format!("dakarta-prueba-{}", std::process::id()));
        std::fs::create_dir_all(&carpeta).unwrap();
        let original = carpeta.join("original.pdf").to_string_lossy().into_owned();
        let copia = carpeta.join("copia.pdf").to_string_lossy().into_owned();

        guardar_doc(&doc_de(300.0, 400.0), &original).unwrap();
        assert!(mismo_archivo(&original, &original));
        assert!(!mismo_archivo(&original, &copia));

        let mut doc = PdfDocument::open(&original).unwrap();
        anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 24.0).unwrap();
        guardar_doc(&doc, &copia).unwrap();
        // Guardar otra vez reemplaza la copia sin dejar el archivo temporal.
        guardar_doc(&doc, &copia).unwrap();
        assert!(!std::path::Path::new(&format!("{copia}.guardando")).exists());
        assert_eq!(PdfDocument::open(&copia).unwrap().page_count().unwrap(), 1);

        std::fs::remove_dir_all(&carpeta).unwrap();
    }

    #[test]
    fn un_enlace_del_indice_lleva_a_su_pagina() {
        // PDF mínimo: la página 1 tiene un enlace (como una línea del índice) que lleva a la página 3.
        let pdf = "%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R 5 0 R]/Count 3>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]/Annots[6 0 R 7 0 R]>>endobj
4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj
5 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj
6 0 obj<</Type/Annot/Subtype/Link/Rect[20 250 180 270]/Dest[5 0 R/Fit]>>endobj
7 0 obj<</Type/Annot/Subtype/Link/Rect[20 200 180 220]/A<</S/URI/URI(https://example.com)>>>>endobj
trailer<</Root 1 0 R>>
%%EOF";
        let ruta = std::env::temp_dir().join(format!("dakarta-enlaces-{}.pdf", std::process::id()));
        std::fs::write(&ruta, pdf).unwrap();
        let doc = PdfDocument::open(ruta.to_str().unwrap()).unwrap();
        let lista = enlaces_de(&doc, 0).unwrap();
        std::fs::remove_file(&ruta).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].pagina, 2);
        // Arriba de la hoja: y = (300 - 270) / 300.
        assert!((lista[0].y - 0.1).abs() < 0.01 && (lista[0].x - 0.1).abs() < 0.01);
    }

    fn anotacion(doc: &PdfDocument, n: i32, xref: i32) -> PdfAnnotation {
        doc.load_pdf_page(n).unwrap().annotations().find(|a| a.xref().ok() == Some(xref)).unwrap()
    }

    #[test]
    fn mover_un_sello_conserva_su_numero_y_el_centrado_y_se_puede_deshacer() {
        let mut doc = doc_de(300.0, 400.0);
        let xref = anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 18.0).unwrap();
        let antes = datos_de(&doc, 0, xref).unwrap();
        assert_eq!(mover_anotacion(&mut doc, 0, xref, 0.1, 0.0).unwrap(), xref);
        let a = anotacion(&doc, 0, xref);
        assert!(matches!(a.quadding().unwrap(), AnnotationTextAlign::Center));
        assert!((a.rect().unwrap().x0 - antes.rect.x0 - 30.0).abs() < 0.5);
        let mut hechas = vec![Hecho::Cambio { n: 0, xref, antes: antes.clone() }];
        assert_eq!(deshacer(&mut doc, &mut hechas).unwrap(), Some(0));
        assert!(hechas.is_empty());
        assert!((datos_de(&doc, 0, xref).unwrap().rect.x0 - antes.rect.x0).abs() < 0.5);
    }

    #[test]
    fn si_deshacer_falla_el_paso_no_se_pierde() {
        let mut doc = doc_de(300.0, 400.0);
        let antes = {
            let x = anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 18.0).unwrap();
            datos_de(&doc, 0, x).unwrap()
        };
        // Un número de anotación que no existe: deshacer falla.
        let mut hechas = vec![Hecho::Cambio { n: 0, xref: 99_999, antes }];
        assert!(deshacer(&mut doc, &mut hechas).is_err());
        assert_eq!(hechas.len(), 1);
    }

    #[test]
    fn lo_borrado_no_queda_escondido_en_la_copia_guardada() {
        let carpeta = std::env::temp_dir().join(format!("dakarta-borrado-{}", std::process::id()));
        std::fs::create_dir_all(&carpeta).unwrap();
        let original = carpeta.join("original.pdf").to_string_lossy().into_owned();
        let copia = carpeta.join("copia.pdf").to_string_lossy().into_owned();
        let mut doc = doc_de(300.0, 400.0);
        anotar_caja(&mut doc, 0, 0.1, 0.1, "SECRETO-123", (0.0, 0.0, 0.0), 12.0).unwrap();
        guardar_doc(&doc, &original).unwrap();
        let mut doc = PdfDocument::open(&original).unwrap();
        let xref = doc.load_pdf_page(0).unwrap().annotations().next().unwrap().xref().unwrap();
        borrar_anotacion(&mut doc, 0, xref).unwrap();
        let mut opciones = PdfWriteOptions::default();
        opciones.set_compress(false);
        opciones.set_garbage_level(1);
        doc.save_with_options(&copia, opciones).unwrap();
        let bytes = std::fs::read(&copia).unwrap();
        assert!(!bytes.windows(11).any(|w| w == b"SECRETO-123"));
        std::fs::remove_dir_all(&carpeta).unwrap();
    }

    #[test]
    fn la_busqueda_avanza_por_tandas_y_encuentra_en_todas_las_paginas() {
        // 10 páginas con la palabra "hola": la búsqueda necesita varias tandas de 4.
        let mut objs = vec![
            "<</Type/Catalog/Pages 2 0 R>>".to_string(),
            String::new(),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string(),
        ];
        let mut kids = Vec::new();
        for _ in 0..10 {
            let contenido = "BT /F1 12 Tf 20 100 Td (hola mundo) Tj ET";
            objs.push(format!("<</Length {}>>stream\n{}\nendstream", contenido.len(), contenido));
            let c = objs.len();
            objs.push(format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Resources<</Font<</F1 3 0 R>>>>/Contents {c} 0 R>>"));
            kids.push(format!("{} 0 R", objs.len()));
        }
        objs[1] = format!("<</Type/Pages/Kids[{}]/Count 10>>", kids.join(" "));
        let mut pdf = String::from("%PDF-1.4\n");
        for (i, o) in objs.iter().enumerate() {
            pdf.push_str(&format!("{} 0 obj{}endobj\n", i + 1, o));
        }
        pdf.push_str("trailer<</Root 1 0 R>>\n%%EOF");
        let ruta = std::env::temp_dir().join(format!("dakarta-busqueda-{}.pdf", std::process::id()));
        std::fs::write(&ruta, pdf).unwrap();
        let doc = PdfDocument::open(ruta.to_str().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut b = BusquedaEnCurso::nueva("hola", tx).ok().unwrap();
        let mut tandas = 1;
        while !b.avanzar(&doc).unwrap() {
            tandas += 1;
        }
        b.terminar();
        let r = rx.recv().unwrap().unwrap();
        std::fs::remove_file(&ruta).unwrap();
        assert_eq!(tandas, 3);
        assert_eq!(r.resultados.len(), 10);
        assert!(!r.escaneado);
    }

    #[test]
    fn las_casillas_usan_el_valor_de_su_pdf_y_los_botones_no_son_campos() {
        // Formulario de prueba con datos inventados: casillas "acepto" y "noticias", opciones F/M.
        let ruta = concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf");
        let mut doc = PdfDocument::open(ruta).unwrap();
        let campos = campos_doc(&doc).unwrap();
        assert!(campos.iter().all(|c| c.tipo != "Button"));
        let acepto = campos.iter().find(|c| c.nombre == "acepto").unwrap();
        assert_eq!(acepto.tipo, "Checkbox");
        assert!(!acepto.encendido.is_empty() && acepto.encendido != "Off");
        let (xref, encendido) = (acepto.xref, acepto.encendido.clone());
        fijar_campo(&mut doc, 0, xref, &encendido).unwrap();
        let ahora = campos_doc(&doc).unwrap();
        assert_eq!(ahora.iter().find(|c| c.xref == xref).unwrap().valor, encendido);
        // Opciones F y M: cada una con su propio valor, que no es "Yes".
        let opciones: Vec<_> = ahora.iter().filter(|c| c.nombre == "sexo").collect();
        assert_eq!(opciones.len(), 2);
        let m = opciones.iter().find(|c| c.encendido == "M").expect("la opción M usa su propio valor");
        fijar_campo(&mut doc, 0, m.xref, "M").unwrap();
        let despues = campos_doc(&doc).unwrap();
        assert!(despues.iter().filter(|c| c.nombre == "sexo").all(|c| c.valor == "M"));
        // Lo que se ve: solo la M queda marcada; la otra opción del grupo se apaga.
        let estados: Vec<String> = doc
            .load_pdf_page(0)
            .unwrap()
            .widgets()
            .filter(|w| w.name().unwrap().as_deref() == Some("sexo"))
            .map(|w| String::from_utf8_lossy(&w.annotation().object().get_dict("AS").unwrap().unwrap().as_name().unwrap()).into_owned())
            .collect();
        assert_eq!(estados.iter().filter(|e| *e != "Off").collect::<Vec<_>>(), vec!["M"]);
        // Y una casilla se desmarca al tocarla otra vez.
        fijar_campo(&mut doc, 0, xref, "Off").unwrap();
        assert_eq!(campos_doc(&doc).unwrap().iter().find(|c| c.xref == xref).unwrap().valor, "Off");
    }

    #[test]
    fn el_cuadro_de_texto_queda_pegado_al_texto() {
        let (ancho, alto) = medida_texto("Texto", 12.0, 500.0).unwrap();
        // "Texto" en Helvetica 12 mide unos 30 puntos; antes el cuadro salía de 60 o más.
        assert!(ancho > 28.0 && ancho < 42.0, "ancho {ancho}");
        assert!(alto > 12.0 && alto < 22.0, "alto {alto}");
    }

    #[test]
    fn el_enter_y_las_lineas_largas_hacen_mas_alto_el_cuadro() {
        let (_, una) = medida_texto("Hola", 12.0, 500.0).unwrap();
        let (_, dos) = medida_texto("Hola\nmundo", 12.0, 500.0).unwrap();
        assert!((dos - una - 14.4).abs() < 0.01);
        let largo = "palabra ".repeat(40);
        let (ancho, alto) = medida_texto(largo.trim(), 12.0, 200.0).unwrap();
        assert!(ancho <= 200.0 && alto > 3.0 * 14.4, "{ancho} × {alto}");
    }

    #[test]
    fn un_texto_nuevo_y_al_agrandarlo_el_cuadro_sigue_al_texto() {
        let mut doc = doc_de(300.0, 400.0);
        let xref = anotar_caja(&mut doc, 0, 0.1, 0.1, "Texto", (0.0, 0.0, 0.0), 12.0).unwrap();
        let antes = datos_de(&doc, 0, xref).unwrap().rect;
        assert!(antes.width() < 42.0, "{antes:?}");
        // Arrastrar la esquina al doble: la letra crece y el cuadro vuelve a pegarse a ella.
        redimensionar_anotacion(&mut doc, 0, xref, 0.1, 0.1, antes.width() * 2.0 / 300.0, antes.height() * 2.0 / 400.0).unwrap();
        let d = datos_de(&doc, 0, xref).unwrap();
        assert!((d.tamano - 24.0).abs() < 0.5, "letra {}", d.tamano);
        assert!((d.rect.width() - medida_texto("Texto", d.tamano, 270.0).unwrap().0).abs() < 0.5);
    }
}

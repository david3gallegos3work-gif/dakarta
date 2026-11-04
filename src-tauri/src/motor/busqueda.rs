//! Búsqueda por tandas: entre una tanda y otra el motor sigue pintando lo que se ve.

use super::*;

/// Busca en todas las páginas. Las cajas vuelven en fracción de la página, para pintarlas encima sin depender del zoom.
/// Búsqueda que avanza de a pocas páginas: entre una tanda y otra el motor sigue pintando lo que se ve,
/// y una búsqueda nueva o cerrar el buscador la cancela.
pub(super) struct BusquedaEnCurso {
    aguja: String,
    siguiente: i32,
    resultados: Vec<Coincidencia>,
    tiene_texto: bool,
    truncada: bool,
    pub(super) resp: Sender<Result<Busqueda, String>>,
}

/// Páginas que se miran por tanda: pocas, para que una página pedida no espere casi nada.
pub(super) const PAGINAS_POR_TANDA: i32 = 4;

impl BusquedaEnCurso {
    pub(super) fn nueva(aguja: &str, resp: Sender<Result<Busqueda, String>>) -> Result<Self, (String, Sender<Result<Busqueda, String>>)> {
        let aguja = aguja.trim();
        if aguja.is_empty() {
            return Err(("Escribe qué buscar.".into(), resp));
        }
        Ok(Self { aguja: aguja.to_string(), siguiente: 0, resultados: Vec::new(), tiene_texto: false, truncada: false, resp })
    }

    /// Mira la siguiente tanda de páginas. Devuelve true cuando ya terminó.
    pub(super) fn avanzar(&mut self, doc: &Document) -> Result<bool, String> {
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

    pub(super) fn terminar(self) {
        let escaneado = self.resultados.is_empty() && !self.tiene_texto;
        let _ = self.resp.send(Ok(Busqueda { resultados: self.resultados, escaneado, truncada: self.truncada }));
    }

    pub(super) fn cancelar(self) {
        let _ = self.resp.send(Err("Búsqueda cancelada".into()));
    }
}

//! Lo que va encima de cada hoja al leer: palabras para seleccionar y enlaces del índice.

use super::*;

/// Enlaces de una página que llevan a otra página del mismo PDF. Los que van a internet se ignoran.
pub(super) fn enlaces_de(doc: &Document, n: i32) -> Result<Vec<Enlace>, String> {
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
pub(super) fn palabras_de(doc: &Document, n: i32) -> Result<Vec<Palabra>, String> {
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

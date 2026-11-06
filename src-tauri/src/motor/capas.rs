//! Lo que va encima de cada hoja al leer: palabras para seleccionar y enlaces del índice.

use super::*;

/// Enlaces de una página que llevan a otra página del mismo PDF. Los que van a internet se ignoran.
pub(super) fn enlaces_de(doc: &PdfDocument, n: i32) -> Result<Vec<Enlace>, String> {
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
pub(super) fn palabras_de(doc: &PdfDocument, n: i32) -> Result<Vec<Palabra>, String> {
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

/// El texto de las páginas `desde`..=`hasta` (desde 0), para Gemini: una cadena por página, con los
/// espacios repetidos juntados. Una página escaneada (sin texto) queda vacía.
pub(super) fn textos_de(doc: &PdfDocument, desde: i32, hasta: i32) -> Result<Vec<String>, String> {
    let total = doc.page_count().map_err(texto)?;
    if desde < 0 || hasta < desde || hasta >= total {
        return Err("Esas páginas no están en el PDF.".into());
    }
    (desde..=hasta)
        .map(|n| {
            let t = doc.load_page(n).and_then(|p| p.text(TextExtractOptions::default())).map_err(texto)?;
            // Sin renglones vacíos repetidos ni espacios de más: menos para enviar.
            let mut limpio = String::with_capacity(t.len());
            let mut vacias = 0;
            for linea in t.lines() {
                let linea = linea.split_whitespace().collect::<Vec<_>>().join(" ");
                if linea.is_empty() {
                    vacias += 1;
                    if vacias > 1 {
                        continue;
                    }
                } else {
                    vacias = 0;
                }
                limpio.push_str(&linea);
                limpio.push('\n');
            }
            Ok(limpio.trim().to_string())
        })
        .collect()
}

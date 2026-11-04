//! Organizar páginas: mover, girar, quitar, duplicar o extraer hojas y guardarlo como un PDF nuevo.
//! El PDF abierto no se toca: se trabaja sobre una copia que ya lleva las anotaciones sin guardar.

use serde::Deserialize;

use super::*;

/// Una hoja del PDF nuevo: de qué página del abierto sale y cuánto se gira (múltiplo de 90).
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Hoja {
    pub pagina: i32,
    pub giro: i32,
}

/// Lo que una hoja puede heredar de más arriba: se le deja puesto para que no dependa del orden.
const HEREDABLES: [&str; 4] = ["MediaBox", "CropBox", "Resources", "Rotate"];

pub(super) fn organizar_y_guardar(doc: &PdfDocument, hojas: &[Hoja], destino: &str) -> Result<(), String> {
    if hojas.is_empty() {
        return Err("No quedó ninguna página para guardar.".into());
    }
    let total = doc.page_count().map_err(texto)?;
    if hojas.iter().any(|h| h.pagina < 0 || h.pagina >= total || h.giro % 90 != 0) {
        return Err("La lista de páginas no es válida.".into());
    }
    let temporal = format!("{destino}.organizando");
    guardar_doc(doc, &temporal)?;
    let r = PdfDocument::open(&temporal).map_err(texto).and_then(|mut copia| {
        reordenar(&mut copia, hojas)?;
        guardar_doc(&copia, destino)
    });
    let _ = std::fs::remove_file(&temporal);
    r
}

fn reordenar(doc: &mut PdfDocument, hojas: &[Hoja]) -> Result<(), String> {
    let total = doc.page_count().map_err(texto)?;
    let mut originales = Vec::with_capacity(total as usize);
    for i in 0..total {
        let mut p = doc.find_page(i).map_err(texto)?;
        for clave in HEREDABLES {
            if p.get_dict(clave).map_err(texto)?.is_none() {
                if let Some(v) = p.get_dict_inheritable(clave).map_err(texto)? {
                    p.dict_put(clave, v).map_err(texto)?;
                }
            }
        }
        originales.push(p);
    }
    // La primera vez una hoja se usa tal cual; si se repite, va una copia (una hoja no puede estar dos veces).
    let mut usada = vec![false; total as usize];
    let mut nuevas = Vec::with_capacity(hojas.len());
    for h in hojas {
        let i = h.pagina as usize;
        if usada[i] {
            nuevas.push(copiar_pagina(doc, &originales[i])?);
        } else {
            usada[i] = true;
            nuevas.push(originales[i].clone());
        }
    }
    for i in (0..total).rev() {
        doc.delete_page(i).map_err(texto)?;
    }
    for (k, p) in nuevas.iter().enumerate() {
        doc.insert_page(k as i32, p).map_err(texto)?;
    }
    for (k, h) in hojas.iter().enumerate() {
        if h.giro.rem_euclid(360) != 0 {
            let mut p = doc.load_pdf_page(k as i32).map_err(texto)?;
            let antes = p.rotation().map_err(texto)?;
            p.set_rotation((antes + h.giro).rem_euclid(360)).map_err(texto)?;
        }
    }
    Ok(())
}

fn nombre(o: &PdfObject) -> String {
    o.as_name().map(|n| String::from_utf8_lossy(&n).into_owned()).unwrap_or_default()
}

/// Copia de una hoja con sus anotaciones. Las casillas de formulario no se copian: serían otro campo
/// con el mismo nombre y llenar una cambiaría la otra.
fn copiar_pagina(doc: &mut PdfDocument, original: &PdfObject) -> Result<PdfObject, String> {
    let mut nueva = doc.new_dict().map_err(texto)?;
    for i in 0..original.dict_len().map_err(texto)? as i32 {
        let (Some(k), Some(v)) = (original.get_dict_key(i).map_err(texto)?, original.get_dict_val(i).map_err(texto)?) else {
            continue;
        };
        if !matches!(nombre(&k).as_str(), "Parent" | "Annots") {
            nueva.dict_put(k, v).map_err(texto)?;
        }
    }
    let mut referencia = doc.add_object(&nueva).map_err(texto)?;
    let Some(anotaciones) = original.get_dict("Annots").map_err(texto)? else {
        return Ok(referencia);
    };
    let mut lista = doc.new_array().map_err(texto)?;
    for j in 0..anotaciones.len().map_err(texto)? as i32 {
        let Some(a) = anotaciones.get_array(j).map_err(texto)? else { continue };
        let tipo = a.get_dict("Subtype").map_err(texto)?.map(|t| nombre(&t)).unwrap_or_default();
        if matches!(tipo.as_str(), "Widget" | "Popup") {
            continue;
        }
        let mut copia = doc.new_dict().map_err(texto)?;
        for i in 0..a.dict_len().map_err(texto)? as i32 {
            let (Some(k), Some(v)) = (a.get_dict_key(i).map_err(texto)?, a.get_dict_val(i).map_err(texto)?) else {
                continue;
            };
            if !matches!(nombre(&k).as_str(), "P" | "Popup" | "Parent" | "IRT") {
                copia.dict_put(k, v).map_err(texto)?;
            }
        }
        copia.dict_put("P", referencia.clone()).map_err(texto)?;
        lista.array_push(doc.add_object(&copia).map_err(texto)?).map_err(texto)?;
    }
    referencia.dict_put("Annots", lista).map_err(texto)?;
    Ok(referencia)
}

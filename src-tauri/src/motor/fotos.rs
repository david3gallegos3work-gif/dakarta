//! Crear un PDF desde fotos: cada foto (un JPEG ya girado y achicado por la pantalla) es una hoja.

use mupdf::{Buffer, Image};

use super::*;

/// Margen alrededor de la foto en una hoja A4, en puntos (unos 7 mm).
const MARGEN_A4: f32 = 20.0;
/// Ancho de la hoja cuando toma la forma de la foto: el de una A4 (o su alto, si la foto es apaisada).
const A4: (f32, f32) = (595.0, 842.0);

/// Tamaño de la hoja y lugar de la foto en ella (x, y, ancho, alto), en puntos.
pub(super) fn medidas(ancho: f32, alto: f32, a4: bool) -> ((f32, f32), (f32, f32, f32, f32)) {
    let apaisada = ancho > alto;
    let (hw, hh) = if apaisada { (A4.1, A4.0) } else { A4 };
    if !a4 {
        let alto_hoja = hw * alto / ancho;
        return ((hw, alto_hoja), (0.0, 0.0, hw, alto_hoja));
    }
    let escala = ((hw - 2.0 * MARGEN_A4) / ancho).min((hh - 2.0 * MARGEN_A4) / alto);
    let (w, h) = (ancho * escala, alto * escala);
    ((hw, hh), ((hw - w) / 2.0, (hh - h) / 2.0, w, h))
}

/// Arma un PDF nuevo con una hoja por foto, en ese orden. `a4`: hojas A4 con la foto centrada;
/// si no, cada hoja toma la forma de su foto. La foto va tal cual (no se vuelve a comprimir).
pub fn pdf_de_fotos(fotos: &[Vec<u8>], a4: bool, destino: &str) -> Result<(), String> {
    if fotos.is_empty() {
        return Err("No hay fotos para el PDF.".into());
    }
    let mut doc = PdfDocument::new();
    for (i, jpg) in fotos.iter().enumerate() {
        let imagen = Image::from_bytes(jpg).map_err(|_| format!("No se pudo leer la foto {}.", i + 1))?;
        let (ancho, alto) = (imagen.width() as f32, imagen.height() as f32);
        if ancho < 1.0 || alto < 1.0 {
            return Err(format!("La foto {} está vacía.", i + 1));
        }
        let ((hw, hh), (x, y, w, h)) = medidas(ancho, alto, a4);
        let foto = doc.add_image(&imagen).map_err(texto)?;
        let mut objetos = doc.new_dict().map_err(texto)?;
        objetos.dict_put("Foto", foto).map_err(texto)?;
        let mut recursos = doc.new_dict().map_err(texto)?;
        recursos.dict_put("XObject", objetos).map_err(texto)?;
        let dibujo = format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Foto Do Q");
        let dibujo = Buffer::from_bytes(dibujo.as_bytes()).map_err(texto)?;
        let contenido = doc.add_stream(&dibujo, None, false).map_err(texto)?;
        let mut caja = doc.new_array().map_err(texto)?;
        for v in [0.0, 0.0, hw, hh] {
            caja.array_push(PdfObject::new_real(v).map_err(texto)?).map_err(texto)?;
        }
        let mut hoja = doc.new_dict().map_err(texto)?;
        hoja.dict_put("Type", PdfObject::new_name("Page").map_err(texto)?).map_err(texto)?;
        hoja.dict_put("MediaBox", caja).map_err(texto)?;
        hoja.dict_put("Resources", recursos).map_err(texto)?;
        hoja.dict_put("Contents", contenido).map_err(texto)?;
        let hoja = doc.add_object(&hoja).map_err(texto)?;
        doc.insert_page(i as i32, &hoja).map_err(texto)?;
    }
    guardar_doc(&doc, destino)
}

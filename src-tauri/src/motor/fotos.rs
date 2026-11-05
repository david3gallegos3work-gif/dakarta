//! Crear un PDF desde fotos: cada foto (un JPEG ya recortado, girado y achicado por la pantalla) va en
//! su hoja, o varias en una hoja A4 a tamaño real (cédula y papeleta).

use mupdf::{Buffer, Image};
use serde::Deserialize;

use super::*;

/// Cómo se acomodan las fotos en las hojas.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Diseno {
    /// Una foto por hoja A4, centrada con margen (A4 de lado si la foto es apaisada).
    A4,
    /// Una foto por hoja, con la forma de la foto.
    Foto,
    /// Documentos de identidad a tamaño real, dos por fila en hojas A4 (cédula y papeleta, frente y reverso).
    Documentos,
}

/// Una hoja A4, en puntos.
const A4: (f32, f32) = (595.0, 842.0);
/// Margen alrededor de la foto en una hoja A4 (unos 7 mm).
const MARGEN_A4: f32 = 20.0;
/// Ancho de una cédula (tarjeta ISO de 85,6 mm), en puntos.
const ANCHO_TARJETA: f32 = 85.6 / 25.4 * 72.0;
/// Un documento más alto que ancho (fotografiado de pie) no pasa de este alto, para que entren las filas.
const ALTO_MAXIMO_DOCUMENTO: f32 = ANCHO_TARJETA;
/// Separación entre documentos y margen de arriba de la hoja (unos 12 y 18 mm).
const ESPACIO_DOCUMENTOS: f32 = 34.0;
const MARGEN_ARRIBA_DOCUMENTOS: f32 = 50.0;

/// Una foto puesta en una hoja: cuál es y su lugar (x e y desde abajo, como en el PDF; ancho y alto).
#[derive(Debug, PartialEq)]
pub(super) struct Puesta {
    pub foto: usize,
    pub caja: (f32, f32, f32, f32),
}

/// Las hojas del PDF: su tamaño y las fotos que lleva cada una. `medidas`: ancho y alto de cada foto.
pub(super) fn acomodar(medidas: &[(f32, f32)], diseno: Diseno) -> Vec<((f32, f32), Vec<Puesta>)> {
    match diseno {
        Diseno::A4 | Diseno::Foto => medidas
            .iter()
            .enumerate()
            .map(|(foto, &(ancho, alto))| {
                let (hoja, caja) = una_por_hoja(ancho, alto, diseno == Diseno::A4);
                (hoja, vec![Puesta { foto, caja }])
            })
            .collect(),
        Diseno::Documentos => documentos(medidas),
    }
}

fn una_por_hoja(ancho: f32, alto: f32, a4: bool) -> ((f32, f32), (f32, f32, f32, f32)) {
    let (hw, hh) = if ancho > alto { (A4.1, A4.0) } else { A4 };
    if !a4 {
        let alto_hoja = hw * alto / ancho;
        return ((hw, alto_hoja), (0.0, 0.0, hw, alto_hoja));
    }
    let escala = ((hw - 2.0 * MARGEN_A4) / ancho).min((hh - 2.0 * MARGEN_A4) / alto);
    let (w, h) = (ancho * escala, alto * escala);
    ((hw, hh), ((hw - w) / 2.0, (hh - h) / 2.0, w, h))
}

/// Dos por fila, en orden, a tamaño de tarjeta; cuando la hoja se llena, sigue en otra.
fn documentos(medidas: &[(f32, f32)]) -> Vec<((f32, f32), Vec<Puesta>)> {
    let (hw, hh) = A4;
    let izquierda = (hw - 2.0 * ANCHO_TARJETA - ESPACIO_DOCUMENTOS) / 2.0;
    let mut hojas: Vec<((f32, f32), Vec<Puesta>)> = Vec::new();
    let mut arriba = f32::INFINITY; // Distancia desde arriba hasta la fila siguiente (infinita: hoja nueva).
    for (fila, pares) in medidas.chunks(2).enumerate() {
        let tam: Vec<(f32, f32)> = pares
            .iter()
            .map(|&(ancho, alto)| {
                let escala = (ANCHO_TARJETA / ancho).min(ALTO_MAXIMO_DOCUMENTO / alto);
                (ancho * escala, alto * escala)
            })
            .collect();
        let alto_fila = tam.iter().map(|t| t.1).fold(0.0, f32::max);
        if arriba + alto_fila > hh - MARGEN_ARRIBA_DOCUMENTOS {
            hojas.push(((hw, hh), Vec::new()));
            arriba = MARGEN_ARRIBA_DOCUMENTOS;
        }
        let hoja = &mut hojas.last_mut().expect("hay hoja").1;
        for (col, &(w, h)) in tam.iter().enumerate() {
            // Cada documento centrado en su columna y arriba en su fila.
            let x = izquierda + col as f32 * (ANCHO_TARJETA + ESPACIO_DOCUMENTOS) + (ANCHO_TARJETA - w) / 2.0;
            hoja.push(Puesta { foto: fila * 2 + col, caja: (x, hh - arriba - h, w, h) });
        }
        arriba += alto_fila + ESPACIO_DOCUMENTOS;
    }
    hojas
}

/// Arma un PDF nuevo con las fotos, en ese orden, según el diseño. Las fotos van tal cual (no se vuelven a
/// comprimir).
pub fn pdf_de_fotos(fotos: &[Vec<u8>], diseno: Diseno, destino: &str) -> Result<(), String> {
    if fotos.is_empty() {
        return Err("No hay fotos para el PDF.".into());
    }
    let mut doc = PdfDocument::new();
    let mut imagenes = Vec::with_capacity(fotos.len());
    let mut medidas = Vec::with_capacity(fotos.len());
    for (i, jpg) in fotos.iter().enumerate() {
        let imagen = Image::from_bytes(jpg).map_err(|_| format!("No se pudo leer la foto {}.", i + 1))?;
        let (ancho, alto) = (imagen.width() as f32, imagen.height() as f32);
        if ancho < 1.0 || alto < 1.0 {
            return Err(format!("La foto {} está vacía.", i + 1));
        }
        medidas.push((ancho, alto));
        imagenes.push(doc.add_image(&imagen).map_err(texto)?);
    }
    for (n, ((hw, hh), puestas)) in acomodar(&medidas, diseno).into_iter().enumerate() {
        let mut objetos = doc.new_dict().map_err(texto)?;
        let mut dibujo = String::new();
        for p in &puestas {
            let nombre = format!("Foto{}", p.foto);
            objetos.dict_put(nombre.as_str(), imagenes[p.foto].clone()).map_err(texto)?;
            let (x, y, w, h) = p.caja;
            dibujo.push_str(&format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /{nombre} Do Q\n"));
        }
        let mut recursos = doc.new_dict().map_err(texto)?;
        recursos.dict_put("XObject", objetos).map_err(texto)?;
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
        doc.insert_page(n as i32, &hoja).map_err(texto)?;
    }
    guardar_doc(&doc, destino)
}

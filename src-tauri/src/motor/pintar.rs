//! Pintar páginas a JPEG y guardar la copia.

use super::*;

/// Se escribe primero a un archivo aparte y solo al terminar reemplaza al destino:
/// si algo falla a medias, el archivo que ya existía queda intacto.
pub(super) fn guardar_doc(doc: &PdfDocument, destino: &str) -> Result<(), String> {
    escribir(doc, destino, &Proteccion::Igual)
}

/// La contraseña de la copia: la misma del original (o ninguna si no tenía), una nueva o ninguna.
pub(super) enum Proteccion {
    Igual,
    Poner(String),
    Quitar,
}

impl Proteccion {
    /// Lo que pide la pantalla: una contraseña nueva, o vacía para quitarla.
    pub(super) fn de(clave: &str) -> Result<Self, String> {
        if clave.is_empty() {
            return Ok(Proteccion::Quitar);
        }
        // El PDF guarda hasta 127 bytes de contraseña.
        if clave.len() > 127 {
            return Err("La contraseña es demasiado larga.".into());
        }
        Ok(Proteccion::Poner(clave.to_string()))
    }
}

/// `clave`: la contraseña con que se abrió `doc` (si tenía). Poner o quitar la contraseña se hace sobre
/// una copia temporal: MuPDF, al guardar con otro cifrado, se lo deja también al documento que guarda, y
/// el abierto no debe cambiar.
pub(super) fn guardar_doc_con(doc: &PdfDocument, destino: &str, proteccion: &Proteccion, clave: Option<&str>) -> Result<(), String> {
    if matches!(proteccion, Proteccion::Igual) {
        return escribir(doc, destino, proteccion);
    }
    let temporal = format!("{destino}.protegiendo");
    let r = escribir(doc, &temporal, &Proteccion::Igual)
        .and_then(|()| abrir_pdf(&temporal, clave))
        .and_then(|copia| escribir(&copia, destino, proteccion));
    let _ = std::fs::remove_file(&temporal);
    r
}

fn escribir(doc: &PdfDocument, destino: &str, proteccion: &Proteccion) -> Result<(), String> {
    let mut opciones = PdfWriteOptions::default();
    opciones.set_compress(true);
    match proteccion {
        // Por omisión MuPDF conserva el cifrado (y la contraseña) que el PDF ya tenía.
        Proteccion::Igual => {}
        Proteccion::Poner(clave) => {
            opciones.set_encryption(Encryption::Aes256);
            opciones.set_user_password(clave);
            opciones.set_owner_password(clave);
        }
        Proteccion::Quitar => {
            opciones.set_encryption(Encryption::None);
        }
    }
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
pub(super) fn mismo_archivo(abierto: &str, destino: &str) -> bool {
    match (std::fs::canonicalize(abierto), std::fs::canonicalize(destino)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub(super) fn pintar(doc: &Document, n: i32, ancho: u32) -> Result<Vec<u8>, String> {
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
pub(super) fn pintar_recorte(doc: &Document, n: i32, r: Recorte) -> Result<Vec<u8>, String> {
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
pub(super) fn jpeg(px: &Pixmap, calidad: u8) -> Result<Vec<u8>, String> {
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, calidad)
        .encode(px.samples(), px.width() as u16, px.height() as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(texto)?;
    Ok(jpg)
}

pub(super) fn respuesta(r: Result<Vec<u8>, String>) -> Response<Vec<u8>> {
    let b = Response::builder();
    match r {
        // La pantalla guarda las páginas ya pintadas: volver a una no la vuelve a pedir.
        Ok(jpg) => b.header(header::CONTENT_TYPE, "image/jpeg").header(header::CACHE_CONTROL, "max-age=3600").body(jpg),
        Err(e) => b.status(500).header(header::CONTENT_TYPE, "text/plain; charset=utf-8").body(e.into_bytes()),
    }
    .expect("cabeceras fijas válidas")
}

//! Comprimir un PDF: las imágenes grandes se achican y se vuelven a guardar como JPEG, y el archivo se
//! guarda compacto (sin objetos repetidos ni sobrantes). Se hace en una copia: el PDF abierto no cambia.

use std::collections::HashSet;

use mupdf::Buffer;
use serde::Deserialize;

use super::*;

/// Cuánto se comprime.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Nivel {
    /// Buena calidad: las imágenes quedan nítidas al imprimir.
    Ligera,
    /// Menos peso: para enviar por WhatsApp o correo; se sigue leyendo bien en pantalla.
    Fuerte,
}

impl Nivel {
    /// Lado más largo de una imagen (en píxeles) y calidad del JPEG.
    fn medidas(self) -> (u32, u8) {
        match self {
            Nivel::Ligera => (2000, 80),
            Nivel::Fuerte => (1300, 60),
        }
    }
}

/// Guarda en `destino` una copia comprimida del PDF abierto (con sus anotaciones y su contraseña, si tenía).
pub(crate) fn comprimir_y_guardar(doc: &PdfDocument, destino: &str, clave: Option<&str>, nivel: Nivel) -> Result<(), String> {
    let temporal = format!("{destino}.comprimiendo");
    guardar_doc(doc, &temporal)?;
    let r = abrir_pdf(&temporal, clave).and_then(|mut copia| {
        comprimir_imagenes(&mut copia, nivel)?;
        guardar_compacto(&copia, destino)
    });
    let _ = std::fs::remove_file(&temporal);
    r
}

/// Achica las imágenes de todas las hojas; devuelve cuántas cambió.
pub(crate) fn comprimir_imagenes(doc: &mut PdfDocument, nivel: Nivel) -> Result<usize, String> {
    let mut vistas = HashSet::new();
    let mut cambiadas = 0;
    for i in 0..doc.page_count().map_err(texto)? {
        let hoja = doc.find_page(i).map_err(texto)?;
        let recursos = hoja.get_dict_inheritable("Resources").map_err(texto)?;
        recorrer(recursos, &mut vistas, nivel, 0, &mut cambiadas)?;
    }
    Ok(cambiadas)
}

fn nombre(o: Option<PdfObject>) -> String {
    o.and_then(|o| o.as_name().ok().map(|n| String::from_utf8_lossy(&n).into_owned())).unwrap_or_default()
}

/// Las imágenes de unos recursos, y las de los dibujos (Form) que haya dentro.
fn recorrer(recursos: Option<PdfObject>, vistas: &mut HashSet<i32>, nivel: Nivel, prof: u8, cambiadas: &mut usize) -> Result<(), String> {
    let Some(recursos) = recursos else { return Ok(()) };
    let Some(objetos) = recursos.get_dict("XObject").map_err(texto)? else { return Ok(()) };
    for k in 0..objetos.dict_len().map_err(texto)? as i32 {
        let Some(mut x) = objetos.get_dict_val(k).map_err(texto)? else { continue };
        // Solo los que están aparte (con número): así cada uno se trata una vez aunque se use en varias hojas.
        if !x.is_indirect().map_err(texto)? || !vistas.insert(x.as_indirect().map_err(texto)?) {
            continue;
        }
        match nombre(x.get_dict("Subtype").map_err(texto)?).as_str() {
            // Una imagen que no se puede achicar se deja como estaba.
            "Image" => *cambiadas += usize::from(achicar_imagen(&mut x, nivel).unwrap_or(false)),
            "Form" if prof < 5 => recorrer(x.get_dict("Resources").map_err(texto)?, vistas, nivel, prof + 1, cambiadas)?,
            _ => {}
        }
    }
    Ok(())
}

fn entero(o: &PdfObject, clave: &str) -> Result<Option<i32>, String> {
    match o.get_dict(clave).map_err(texto)? {
        Some(v) if v.is_number().map_err(texto)? => Ok(Some(v.as_int().map_err(texto)?)),
        _ => Ok(None),
    }
}

/// Cuántos componentes tiene el color de la imagen: 1 (gris) o 3 (RGB). Otros (CMYK, paletas…) no se tocan.
fn componentes(img: &PdfObject) -> Result<Option<usize>, String> {
    let Some(cs) = img.get_dict("ColorSpace").map_err(texto)? else { return Ok(None) };
    if cs.is_name().map_err(texto)? {
        return Ok(match nombre(Some(cs)).as_str() {
            "DeviceRGB" | "CalRGB" => Some(3),
            "DeviceGray" | "CalGray" => Some(1),
            _ => None,
        });
    }
    // [/ICCBased <<… /N 3>>]: lo que cuenta es su N.
    if cs.is_array().map_err(texto)? && cs.len().map_err(texto)? == 2 && nombre(cs.get_array(0).map_err(texto)?) == "ICCBased" {
        if let Some(perfil) = cs.get_array(1).map_err(texto)? {
            return Ok(match entero(&perfil, "N")? {
                Some(3) => Some(3),
                Some(1) => Some(1),
                _ => None,
            });
        }
    }
    Ok(None)
}

/// Achica una imagen y la guarda como JPEG si así pesa bastante menos. Devuelve si la cambió.
fn achicar_imagen(img: &mut PdfObject, nivel: Nivel) -> Result<bool, String> {
    // Máscaras y colores especiales: un JPEG (con pérdida) los dañaría.
    if img.get_dict("ImageMask").map_err(texto)?.is_some_and(|m| m.as_bool().unwrap_or(false))
        || img.get_dict("Mask").map_err(texto)?.is_some()
        || img.get_dict("Decode").map_err(texto)?.is_some()
        || entero(img, "BitsPerComponent")? != Some(8)
    {
        return Ok(false);
    }
    let (Some(ancho), Some(alto), Some(n)) = (entero(img, "Width")?, entero(img, "Height")?, componentes(img)?) else {
        return Ok(false);
    };
    if ancho < 1 || alto < 1 {
        return Ok(false);
    }
    let (ancho, alto) = (ancho as u32, alto as u32);
    let (maximo, calidad) = nivel.medidas();
    let filtro = nombre(img.get_dict("Filter").map_err(texto)?);
    // Un JPEG que ya es chico no gana nada: volver a comprimirlo solo le quitaría calidad.
    if filtro == "DCTDecode" && ancho.max(alto) <= maximo {
        return Ok(false);
    }
    let antes = img.read_raw_stream().map_err(texto)?.len();
    let puntos = img.read_stream().map_err(texto)?;
    // Si no son los puntos sueltos que se esperan (por ejemplo, JPEG 2000), no se toca.
    if puntos.len() != ancho as usize * alto as usize * n {
        return Ok(false);
    }
    let (nuevo_ancho, nuevo_alto, puntos) = achicar(&puntos, ancho, alto, n, maximo);
    let mut jpg = Vec::new();
    let color = if n == 3 { jpeg_encoder::ColorType::Rgb } else { jpeg_encoder::ColorType::Luma };
    jpeg_encoder::Encoder::new(&mut jpg, calidad)
        .encode(&puntos, nuevo_ancho as u16, nuevo_alto as u16, color)
        .map_err(texto)?;
    // Solo si ahorra al menos una décima parte.
    if jpg.len() * 10 > antes * 9 {
        return Ok(false);
    }
    img.write_raw_stream_buffer(&Buffer::from_bytes(&jpg).map_err(texto)?).map_err(texto)?;
    img.dict_put("Filter", PdfObject::new_name("DCTDecode").map_err(texto)?).map_err(texto)?;
    img.dict_delete("DecodeParms").map_err(texto)?;
    img.dict_put("Width", PdfObject::new_int(nuevo_ancho as i32).map_err(texto)?).map_err(texto)?;
    img.dict_put("Height", PdfObject::new_int(nuevo_alto as i32).map_err(texto)?).map_err(texto)?;
    Ok(true)
}

/// Achica (nunca agranda) a que el lado más largo no pase de `maximo`, promediando los puntos de cada bloque.
pub(crate) fn achicar(puntos: &[u8], ancho: u32, alto: u32, n: usize, maximo: u32) -> (u32, u32, Vec<u8>) {
    let lado = ancho.max(alto);
    if lado <= maximo {
        return (ancho, alto, puntos.to_vec());
    }
    let nw = ((ancho as u64 * maximo as u64) / lado as u64).max(1) as u32;
    let nh = ((alto as u64 * maximo as u64) / lado as u64).max(1) as u32;
    let mut salida = vec![0u8; nw as usize * nh as usize * n];
    let mut suma = vec![0u32; nw as usize * n];
    let mut cuenta = vec![0u32; nw as usize];
    // Columna de destino de cada columna de origen.
    let col: Vec<usize> = (0..ancho).map(|x| ((x as u64 * nw as u64) / ancho as u64) as usize).collect();
    let mut fila = 0u32;
    for y in 0..alto {
        let destino = ((y as u64 * nh as u64) / alto as u64) as u32;
        if destino != fila {
            volcar(&mut salida, &mut suma, &mut cuenta, fila, nw, n);
            fila = destino;
        }
        let origen = &puntos[y as usize * ancho as usize * n..][..ancho as usize * n];
        for (x, &c) in col.iter().enumerate() {
            for k in 0..n {
                suma[c * n + k] += origen[x * n + k] as u32;
            }
            cuenta[c] += 1;
        }
    }
    volcar(&mut salida, &mut suma, &mut cuenta, fila, nw, n);
    (nw, nh, salida)
}

fn volcar(salida: &mut [u8], suma: &mut [u32], cuenta: &mut [u32], fila: u32, nw: u32, n: usize) {
    let base = fila as usize * nw as usize * n;
    for c in 0..nw as usize {
        let total = cuenta[c].max(1);
        for k in 0..n {
            salida[base + c * n + k] = ((suma[c * n + k] + total / 2) / total) as u8;
            suma[c * n + k] = 0;
        }
        cuenta[c] = 0;
    }
}

/// Guarda quitando objetos repetidos y sobrantes, con todo comprimido. Conserva la contraseña.
fn guardar_compacto(doc: &PdfDocument, destino: &str) -> Result<(), String> {
    let mut opciones = PdfWriteOptions::default();
    opciones.set_compress(true);
    opciones.set_compress_fonts(true);
    opciones.set_garbage_level(3);
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

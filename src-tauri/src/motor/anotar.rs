//! Anotaciones: crear, leer, mover, cambiar, agrandar, borrar y deshacer.

use super::*;

pub(super) fn anotacion_color((rojo, verde, azul): (f32, f32, f32)) -> AnnotationColor {
    AnnotationColor::Rgb { red: rojo, green: verde, blue: azul }
}

/// Resalta, subraya o tacha las líneas de texto que toca el arrastre (a y b en fracción de la página).
pub(super) fn anotar_texto(
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
pub(super) fn anotar_tinta(
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

/// Cuadro de texto: se abre desde el toque hacia la derecha y abajo.
#[allow(clippy::too_many_arguments)]
pub(super) fn anotar_caja(
    doc: &mut PdfDocument,
    n: i32,
    x: f32,
    y: f32,
    contenido: &str,
    color: (f32, f32, f32),
    tamano: f32,
    postit: bool,
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
    // La nota es como un papelito pegado: el color de una FreeText es el relleno del cuadro.
    if postit {
        annot.set_color(AnnotationColor::Rgb { red: 1.0, green: 0.95, blue: 0.62 }).map_err(texto)?;
    }
    poner_borde(&mut annot, tamano, rect)?;
    annot.update().map_err(texto)?;
    annot.xref().map_err(texto)
}

/// Sello de texto grande, centrado en el toque.
pub(super) fn anotar_sello(
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

pub(super) fn color_de(color: Option<AnnotationColor>, por_defecto: (f32, f32, f32)) -> (f32, f32, f32) {
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
pub(super) fn leer_datos(a: &PdfAnnotation) -> Result<Option<DatosAnotacion>, String> {
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
pub(super) fn rect_de_trazos(trazos: &[Vec<Point>]) -> Rect {
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

pub(super) fn datos_de(doc: &PdfDocument, n: i32, xref: i32) -> Result<DatosAnotacion, String> {
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let a = pdf
        .annotations()
        .find(|a| a.xref().ok() == Some(xref))
        .ok_or("No se encontró la anotación.")?;
    leer_datos(&a)?.ok_or_else(|| "Esta anotación no se puede cambiar.".into())
}

/// Anotaciones editables de una página, con su caja en fracción de la página.
pub(super) fn anotaciones_de(doc: &PdfDocument, n: i32) -> Result<Vec<Anotacion>, String> {
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

pub(super) fn aplicar_datos(a: &mut PdfAnnotation, d: &DatosAnotacion) -> Result<(), String> {
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

pub(super) fn crear_anotacion(doc: &mut PdfDocument, n: i32, d: &DatosAnotacion) -> Result<i32, String> {
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

pub(super) fn fijar(v: f32, min: f32, max: f32) -> f32 {
    if min > max { v } else { v.clamp(min, max) }
}

pub(super) fn mover_anotacion(doc: &mut PdfDocument, n: i32, xref: i32, dx: f32, dy: f32) -> Result<i32, String> {
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
pub(super) const LETRA_MIN: f32 = 8.0;
/// MuPDF pone la primera línea pegada al borde de arriba y corta lo que sobresale: la tilde de la Á o la
/// virgulilla de la Ñ. Un borde invisible de este grosor baja el texto lo justo para que quepan.
pub(super) fn borde_de(tamano: f32) -> f32 {
    (tamano * 0.14).max(1.0)
}

/// Borde invisible (trazos de largo cero) que deja sitio arriba para las tildes de las mayúsculas.
/// MuPDF agranda la caja al cambiar el borde: se vuelve a poner la caja justa.
pub(super) fn poner_borde(a: &mut PdfAnnotation, tamano: f32, rect: Rect) -> Result<(), String> {
    a.set_border_width(borde_de(tamano)).map_err(texto)?;
    a.set_border_dash_pattern(&[0.0, 1000.0]).map_err(texto)?;
    a.set_rect(rect).map_err(texto)
}

pub(super) fn ancho_de(fuente: &Font, s: &str, tamano: f32) -> f32 {
    let em: f32 = s
        .chars()
        .map(|c| fuente.encode_character(c as i32).and_then(|g| fuente.advance_glyph(g)).unwrap_or(0.6))
        .sum();
    em * tamano
}

/// Ancho y alto justos de un texto con la letra del cuadro (Helvetica). Corta las líneas en los
/// espacios, igual que MuPDF al dibujarlo, para no pasar de `max_ancho`; los Enter se respetan.
pub(super) fn medida_texto(contenido: &str, tamano: f32, max_ancho: f32) -> Result<(f32, f32), String> {
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
pub(super) fn caja_justa(x0: f32, y0: f32, contenido: &str, tamano: f32, caja: Rect) -> Result<Rect, String> {
    let (ancho, alto) = medida_texto(contenido, tamano, caja.width() * 0.9)?;
    let x0 = x0.min(caja.x1 - ancho).max(caja.x0);
    let y0 = y0.min(caja.y1 - alto).max(caja.y0);
    Ok(Rect::new(x0, y0, x0 + ancho, y0 + alto))
}

pub(super) fn editar_anotacion(
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

pub(super) fn editar_en_pagina(doc: &mut PdfDocument, n: i32, xref: i32, d: &DatosAnotacion) -> Result<(), String> {
    let pdf = doc.load_pdf_page(n).map_err(texto)?;
    let mut a = pdf
        .annotations()
        .find(|a| a.xref().ok() == Some(xref))
        .ok_or("No se encontró la anotación.")?;
    aplicar_datos(&mut a, d)
}

/// Cambia el tamaño de la caja (en fracción de la página). La letra y los trazos acompañan al cuadro.
pub(super) fn redimensionar_anotacion(doc: &mut PdfDocument, n: i32, xref: i32, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
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

pub(super) fn borrar_anotacion(doc: &mut PdfDocument, n: i32, xref: i32) -> Result<(), String> {
    let mut pagina = doc.load_pdf_page(n).map_err(texto)?;
    let encontrada = pagina.annotations().find(|a| a.xref().ok() == Some(xref));
    if let Some(a) = encontrada {
        pagina.delete_annotation(a).map_err(texto)?;
    }
    Ok(())
}

/// Deshace el último cambio de esta sesión y devuelve la página que hay que volver a pintar.
pub(super) fn deshacer(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<Option<i32>, String> {
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
pub(super) fn deshacer_todo(doc: &mut PdfDocument, hechas: &mut Vec<Hecho>) -> Result<bool, String> {
    if hechas.is_empty() {
        return Ok(false);
    }
    while let Some(hecho) = hechas.last().cloned() {
        deshacer_hecho(doc, hecho, hechas)?;
        hechas.pop();
    }
    Ok(true)
}

pub(super) fn pagina_de(hecho: &Hecho) -> i32 {
    match hecho {
        Hecho::Anotacion { n, .. } | Hecho::Cambio { n, .. } | Hecho::Borrada { n, .. } | Hecho::Campo { n, .. } => *n,
    }
}

pub(super) fn deshacer_hecho(doc: &mut PdfDocument, hecho: Hecho, hechas: &mut Vec<Hecho>) -> Result<(), String> {
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

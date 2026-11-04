//! Campos de formulario: leerlos, llenarlos y las casillas de opción.

use super::*;

/// El nombre del estado "marcada" de una casilla: el que no es "Off" entre sus apariencias (/AP /N).
pub(super) fn valor_encendido(a: &PdfAnnotation) -> String {
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
pub(super) fn campos_doc(doc: &PdfDocument) -> Result<Vec<Campo>, String> {
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
pub(super) fn fijar_campo(doc: &mut PdfDocument, n: i32, xref: i32, valor: &str) -> Result<String, String> {
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
pub(super) fn apagar_las_demas(pagina: &PdfPage, grupo: Option<&str>, elegido: &str) -> Result<(), mupdf::Error> {
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

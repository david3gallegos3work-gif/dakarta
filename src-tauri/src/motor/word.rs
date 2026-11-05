//! Lector de Word (.docx) propio: convierte el documento a HTML con su formato (alineación, negritas,
//! viñetas, tablas, saltos de página…) y entrega el tamaño de hoja y los márgenes del propio Word, para
//! que MuPDF lo pagine casi igual que Word. El lector de Word de MuPDF pierde mucho de eso.
//! Las medidas de Word vienen en «twips» (1/20 de punto) y la letra en medios puntos.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Cursor, Read};

use roxmltree::{Document as Xml, Node};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Un Word ya convertido: lo que se le da a MuPDF (HTML, o EPUB si hay saltos de página) y la hoja
/// (ancho, alto y letra base, en puntos).
pub(super) struct Convertido {
    pub datos: Vec<u8>,
    pub tipo: &'static str,
    pub ancho: f32,
    pub alto: f32,
    pub letra: f32,
}

pub(super) fn convertir(bytes: &[u8]) -> Result<Convertido, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let leer = |zip: &mut zip::ZipArchive<Cursor<&[u8]>>, nombre: &str| -> Option<String> {
        let mut f = zip.by_name(nombre).ok()?;
        let mut s = String::new();
        f.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let documento = leer(&mut zip, "word/document.xml").ok_or("No es un documento de Word.")?;
    let estilos_xml = leer(&mut zip, "word/styles.xml");
    let numeracion_xml = leer(&mut zip, "word/numbering.xml");

    let doc = Xml::parse(&documento).map_err(|e| e.to_string())?;
    let estilos = estilos_xml.as_deref().and_then(|s| Xml::parse(s).ok());
    let numeracion = numeracion_xml.as_deref().and_then(|s| Xml::parse(s).ok());
    let mut c = Conversor::nuevo(estilos.as_ref(), numeracion.as_ref());

    let cuerpo = hijo(doc.root_element(), "body").ok_or("El Word no tiene contenido.")?;
    // La última sección manda el tamaño de hoja y los márgenes (Carta si no lo dice).
    let seccion = hijo(cuerpo, "sectPr");
    let medida = |n: Option<Node>, atributo: &str, defecto: f32| {
        n.and_then(|n| atr(n, atributo)).and_then(|v| v.parse::<f32>().ok()).map_or(defecto, |v| v / 20.0)
    };
    let tam = seccion.and_then(|s| hijo(s, "pgSz"));
    let mar = seccion.and_then(|s| hijo(s, "pgMar"));
    let (mut ancho, mut alto) = (medida(tam, "w", 612.0), medida(tam, "h", 792.0));
    if !(100.0..=5000.0).contains(&ancho) || !(100.0..=5000.0).contains(&alto) {
        (ancho, alto) = (612.0, 792.0);
    }
    let margenes = [
        medida(mar, "top", 72.0),
        medida(mar, "right", 72.0),
        medida(mar, "bottom", 72.0),
        medida(mar, "left", 72.0),
    ]
    .map(|m| m.clamp(0.0, 200.0));

    let mut cuerpo_html = String::new();
    c.bloques(cuerpo, &mut cuerpo_html);

    let letra = c.base.tamano.unwrap_or(11.0);
    let familia = familia_css(c.base.fuente.as_deref());
    let estilo = format!(
        "@page{{margin:{:.1}pt {:.1}pt {:.1}pt {:.1}pt}}\
         body{{margin:0;font-family:{familia};font-size:{letra}pt;line-height:{:.3}}}\
         p{{margin:0}}table{{border-collapse:collapse}}td{{vertical-align:top;padding:0 5.4pt}}\
         .borde td{{border:0.5pt solid #000}}",
        margenes[0], margenes[1], margenes[2], margenes[3], c.base_parrafo.interlineado.unwrap_or(1.15),
    );
    let pagina = |cuerpo: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head>\
             <meta charset=\"utf-8\"/><title>Word</title><style>{estilo}</style></head><body>{cuerpo}</body></html>"
        )
    };
    // MuPDF no hace saltos de página dentro de un HTML, pero cada capítulo de un EPUB empieza en hoja nueva:
    // con saltos, cada tramo entre saltos va como un capítulo.
    let tramos: Vec<String> = cuerpo_html.split(SALTO).map(pagina).collect();
    let (datos, tipo) = if tramos.len() == 1 {
        (tramos[0].clone().into_bytes(), "html")
    } else {
        (epub(&tramos)?, "epub")
    };
    Ok(Convertido { datos, tipo, ancho, alto, letra })
}

/// Marca de salto de página dentro del HTML que se va armando.
const SALTO: &str = "<!--salto-de-pagina-->";

/// Un EPUB mínimo, sin comprimir, con un capítulo por tramo.
fn epub(tramos: &[String]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let guardado = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut poner = |nombre: &str, contenido: &[u8]| -> Result<(), String> {
        zip.start_file(nombre, guardado).map_err(|e| e.to_string())?;
        zip.write_all(contenido).map_err(|e| e.to_string())
    };
    poner("mimetype", b"application/epub+zip")?;
    poner(
        "META-INF/container.xml",
        br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )?;
    let mut manifiesto = String::new();
    let mut orden = String::new();
    for (i, t) in tramos.iter().enumerate() {
        poner(&format!("p{i}.xhtml"), t.as_bytes())?;
        let _ = write!(manifiesto, r#"<item id="p{i}" href="p{i}.xhtml" media-type="application/xhtml+xml"/>"#);
        let _ = write!(orden, r#"<itemref idref="p{i}"/>"#);
    }
    poner(
        "content.opf",
        format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">word</dc:identifier><dc:title>Word</dc:title><dc:language>es</dc:language></metadata><manifest>{manifiesto}</manifest><spine>{orden}</spine></package>"#
        )
        .as_bytes(),
    )?;
    drop(poner);
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

// ——— Lectura del XML ———

fn es(n: Node, nombre: &str) -> bool {
    n.is_element() && n.tag_name().name() == nombre && n.tag_name().namespace() == Some(W)
}

fn hijo<'a, 'i>(n: Node<'a, 'i>, nombre: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| es(*c, nombre))
}

fn atr<'a>(n: Node<'a, '_>, nombre: &str) -> Option<&'a str> {
    n.attribute((W, nombre))
}

fn val<'a>(n: Node<'a, '_>, nombre: &str) -> Option<&'a str> {
    hijo(n, nombre).and_then(|h| atr(h, "val"))
}

/// Una propiedad sí/no de Word: `<w:b/>` es sí; `<w:b w:val="0"/>` o `false` es no.
fn bandera(n: Node, nombre: &str) -> Option<bool> {
    hijo(n, nombre).map(|h| !matches!(atr(h, "val"), Some("0" | "false" | "off" | "none")))
}

fn numero(s: Option<&str>) -> Option<f32> {
    s.and_then(|v| v.parse::<f32>().ok())
}

// ——— Formato de letra y de párrafo, con la herencia de los estilos ———

#[derive(Clone, Default)]
struct Letra {
    negrita: Option<bool>,
    cursiva: Option<bool>,
    subrayado: Option<bool>,
    tachado: Option<bool>,
    mayusculas: Option<bool>,
    tamano: Option<f32>,
    color: Option<String>,
    fondo: Option<String>,
    fuente: Option<String>,
    vertical: Option<String>,
}

impl Letra {
    fn leer(rpr: Node) -> Self {
        Letra {
            negrita: bandera(rpr, "b"),
            cursiva: bandera(rpr, "i"),
            subrayado: hijo(rpr, "u").map(|u| !matches!(atr(u, "val"), Some("none" | "0"))),
            tachado: bandera(rpr, "strike"),
            mayusculas: bandera(rpr, "caps"),
            tamano: numero(val(rpr, "sz")).map(|v| v / 2.0),
            color: val(rpr, "color").filter(|c| *c != "auto" && c.len() == 6).map(|c| format!("#{c}")),
            fondo: val(rpr, "highlight").and_then(color_resaltado).map(str::to_string).or_else(|| {
                hijo(rpr, "shd").and_then(|s| atr(s, "fill")).filter(|c| *c != "auto" && c.len() == 6).map(|c| format!("#{c}"))
            }),
            fuente: hijo(rpr, "rFonts").and_then(|f| atr(f, "ascii").or_else(|| atr(f, "hAnsi"))).map(str::to_string),
            vertical: val(rpr, "vertAlign").map(str::to_string),
        }
    }

    /// `encima` gana donde dice algo; donde no, se queda lo de abajo.
    fn sobre(&self, encima: &Letra) -> Letra {
        Letra {
            negrita: encima.negrita.or(self.negrita),
            cursiva: encima.cursiva.or(self.cursiva),
            subrayado: encima.subrayado.or(self.subrayado),
            tachado: encima.tachado.or(self.tachado),
            mayusculas: encima.mayusculas.or(self.mayusculas),
            tamano: encima.tamano.or(self.tamano),
            color: encima.color.clone().or_else(|| self.color.clone()),
            fondo: encima.fondo.clone().or_else(|| self.fondo.clone()),
            fuente: encima.fuente.clone().or_else(|| self.fuente.clone()),
            vertical: encima.vertical.clone().or_else(|| self.vertical.clone()),
        }
    }

    fn css(&self, base: &Letra) -> String {
        let mut s = String::new();
        if self.negrita == Some(true) {
            s.push_str("font-weight:bold;");
        }
        if self.cursiva == Some(true) {
            s.push_str("font-style:italic;");
        }
        match (self.subrayado == Some(true), self.tachado == Some(true)) {
            (true, true) => s.push_str("text-decoration:underline line-through;"),
            (true, false) => s.push_str("text-decoration:underline;"),
            (false, true) => s.push_str("text-decoration:line-through;"),
            _ => {}
        }
        if self.mayusculas == Some(true) {
            s.push_str("text-transform:uppercase;");
        }
        if let Some(t) = self.tamano.filter(|t| Some(*t) != base.tamano) {
            let _ = write!(s, "font-size:{t}pt;");
        }
        if let Some(c) = &self.color {
            let _ = write!(s, "color:{c};");
        }
        if let Some(c) = &self.fondo {
            let _ = write!(s, "background-color:{c};");
        }
        if self.fuente.is_some() && self.fuente != base.fuente {
            let _ = write!(s, "font-family:{};", familia_css(self.fuente.as_deref()));
        }
        match self.vertical.as_deref() {
            Some("superscript") => s.push_str("vertical-align:super;font-size:smaller;"),
            Some("subscript") => s.push_str("vertical-align:sub;font-size:smaller;"),
            _ => {}
        }
        s
    }
}

#[derive(Clone, Default)]
struct Parrafo {
    alineacion: Option<String>,
    antes: Option<f32>,
    despues: Option<f32>,
    interlineado: Option<f32>,
    interlineado_pt: Option<f32>,
    izquierda: Option<f32>,
    derecha: Option<f32>,
    primera: Option<f32>,
    salto_antes: Option<bool>,
    lista: Option<(String, usize)>,
}

impl Parrafo {
    fn leer(ppr: Node) -> Self {
        let esp = hijo(ppr, "spacing");
        let ind = hijo(ppr, "ind");
        let tw = |n: Option<Node>, a: &[&str]| n.and_then(|n| a.iter().find_map(|x| atr(n, x))).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 20.0);
        let linea = esp.and_then(|e| atr(e, "line")).and_then(|v| v.parse::<f32>().ok());
        let regla = esp.and_then(|e| atr(e, "lineRule")).unwrap_or("auto");
        let (interlineado, interlineado_pt) = match (linea, regla) {
            (Some(l), "auto") => (Some((l / 240.0).clamp(0.6, 4.0)), None),
            (Some(l), _) => (None, Some(l / 20.0)),
            _ => (None, None),
        };
        let primera = tw(ind, &["firstLine"]).or_else(|| tw(ind, &["hanging"]).map(|h| -h));
        Parrafo {
            alineacion: val(ppr, "jc").map(str::to_string),
            antes: tw(esp, &["before"]),
            despues: tw(esp, &["after"]),
            interlineado,
            interlineado_pt,
            izquierda: tw(ind, &["left", "start"]),
            derecha: tw(ind, &["right", "end"]),
            primera,
            salto_antes: bandera(ppr, "pageBreakBefore"),
            lista: hijo(ppr, "numPr").and_then(|n| {
                let id = val(n, "numId")?.to_string();
                let nivel = val(n, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                (id != "0").then_some((id, nivel))
            }),
        }
    }

    fn sobre(&self, e: &Parrafo) -> Parrafo {
        Parrafo {
            alineacion: e.alineacion.clone().or_else(|| self.alineacion.clone()),
            antes: e.antes.or(self.antes),
            despues: e.despues.or(self.despues),
            interlineado: e.interlineado.or(if e.interlineado_pt.is_some() { None } else { self.interlineado }),
            interlineado_pt: e.interlineado_pt.or(if e.interlineado.is_some() { None } else { self.interlineado_pt }),
            izquierda: e.izquierda.or(self.izquierda),
            derecha: e.derecha.or(self.derecha),
            primera: e.primera.or(self.primera),
            salto_antes: e.salto_antes.or(self.salto_antes),
            lista: e.lista.clone().or_else(|| self.lista.clone()),
        }
    }
}

#[derive(Clone, Default)]
struct Estilo {
    letra: Letra,
    parrafo: Parrafo,
    padre: Option<String>,
}

#[derive(Clone)]
struct Nivel {
    formato: String,
    texto: String,
    inicio: u32,
    izquierda: Option<f32>,
    colgante: Option<f32>,
}

struct Conversor {
    estilos: HashMap<String, Estilo>,
    estilo_normal: Option<String>,
    base: Letra,
    base_parrafo: Parrafo,
    /// numId → niveles de su lista.
    listas: HashMap<String, Vec<Nivel>>,
    contadores: HashMap<String, Vec<u32>>,
}

impl Conversor {
    fn nuevo(estilos: Option<&Xml>, numeracion: Option<&Xml>) -> Self {
        let mut c = Conversor {
            estilos: HashMap::new(),
            estilo_normal: None,
            base: Letra { tamano: Some(10.0), ..Default::default() },
            base_parrafo: Parrafo::default(),
            listas: HashMap::new(),
            contadores: HashMap::new(),
        };
        if let Some(x) = estilos {
            let raiz = x.root_element();
            if let Some(def) = hijo(raiz, "docDefaults") {
                if let Some(rpr) = hijo(def, "rPrDefault").and_then(|r| hijo(r, "rPr")) {
                    c.base = c.base.sobre(&Letra::leer(rpr));
                }
                if let Some(ppr) = hijo(def, "pPrDefault").and_then(|p| hijo(p, "pPr")) {
                    c.base_parrafo = Parrafo::leer(ppr);
                }
            }
            for e in raiz.children().filter(|n| es(*n, "style")) {
                let Some(id) = atr(e, "styleId") else { continue };
                if atr(e, "type") == Some("paragraph") && atr(e, "default") == Some("1") {
                    c.estilo_normal = Some(id.to_string());
                }
                c.estilos.insert(
                    id.to_string(),
                    Estilo {
                        letra: hijo(e, "rPr").map(Letra::leer).unwrap_or_default(),
                        parrafo: hijo(e, "pPr").map(Parrafo::leer).unwrap_or_default(),
                        padre: val(e, "basedOn").map(str::to_string),
                    },
                );
            }
            // La letra del estilo Normal es la base del documento.
            if let Some(n) = c.estilo_normal.clone() {
                let (l, p) = c.estilo(&n);
                c.base = c.base.sobre(&l);
                c.base_parrafo = c.base_parrafo.sobre(&p);
            }
        }
        if let Some(x) = numeracion {
            c.listas = leer_listas(x.root_element());
        }
        c
    }

    /// Letra y párrafo de un estilo, con lo que hereda de sus padres.
    fn estilo(&self, id: &str) -> (Letra, Parrafo) {
        let mut cadena = Vec::new();
        let mut actual = Some(id.to_string());
        while let Some(i) = actual {
            if cadena.len() > 20 || cadena.contains(&i) {
                break;
            }
            actual = self.estilos.get(&i).and_then(|e| e.padre.clone());
            cadena.push(i);
        }
        let (mut l, mut p) = (Letra::default(), Parrafo::default());
        for i in cadena.iter().rev() {
            if let Some(e) = self.estilos.get(i) {
                l = l.sobre(&e.letra);
                p = p.sobre(&e.parrafo);
            }
        }
        (l, p)
    }

    /// Párrafos y tablas de un cuerpo, una celda o un bloque.
    fn bloques(&mut self, padre: Node, html: &mut String) {
        for n in padre.children().filter(|n| n.is_element()) {
            if es(n, "p") {
                self.parrafo(n, html);
            } else if es(n, "tbl") {
                self.tabla(n, html);
            } else if es(n, "sdt") {
                if let Some(c) = hijo(n, "sdtContent") {
                    self.bloques(c, html);
                }
            } else if es(n, "customXml") || es(n, "ins") {
                self.bloques(n, html);
            }
        }
    }

    fn parrafo(&mut self, p: Node, html: &mut String) {
        let ppr = hijo(p, "pPr");
        let id_estilo = ppr.and_then(|x| val(x, "pStyle")).map(str::to_string).or_else(|| self.estilo_normal.clone());
        let (letra_estilo, parrafo_estilo) = id_estilo.as_deref().map(|i| self.estilo(i)).unwrap_or_default();
        let formato = self.base_parrafo.sobre(&parrafo_estilo).sobre(&ppr.map(Parrafo::leer).unwrap_or_default());
        let letra_parrafo = self.base.sobre(&letra_estilo);

        let mut contenido = String::new();
        let mut salto_pagina = formato.salto_antes == Some(true);
        self.corridas(p, &letra_parrafo, &mut contenido);
        if contenido.starts_with("\u{0}SALTO\u{0}") {
            contenido = contenido.replacen("\u{0}SALTO\u{0}", "", 1);
            salto_pagina = true;
        }
        // Un renglón con solo el salto (Ctrl+Enter en Word) no deja un renglón vacío en la hoja nueva.
        if salto_pagina && contenido.trim().is_empty() && formato.lista.is_none() {
            html.push_str(SALTO);
            return;
        }
        let partes: Vec<&str> = contenido.split("\u{0}SALTO\u{0}").collect();

        let mut css = String::new();
        if salto_pagina {
            html.push_str(SALTO);
        }
        if let Some(a) = formato.alineacion.as_deref() {
            let a = match a {
                "center" => "center",
                "right" | "end" => "right",
                "both" | "distribute" => "justify",
                _ => "left",
            };
            let _ = write!(css, "text-align:{a};");
        }
        let mut izquierda = formato.izquierda.unwrap_or(0.0);
        let mut primera = formato.primera.unwrap_or(0.0);
        let mut marca = String::new();
        if let Some((id, nivel)) = &formato.lista {
            if let Some((texto, izq, colg)) = self.marca_lista(id, *nivel) {
                marca = texto;
                izquierda = formato.izquierda.or(izq).unwrap_or(18.0 * (*nivel as f32 + 1.0));
                primera = -(colg.unwrap_or(18.0));
            }
        }
        if izquierda != 0.0 {
            let _ = write!(css, "margin-left:{izquierda:.1}pt;");
        }
        if let Some(d) = formato.derecha.filter(|d| *d != 0.0) {
            let _ = write!(css, "margin-right:{d:.1}pt;");
        }
        if primera != 0.0 {
            let _ = write!(css, "text-indent:{primera:.1}pt;");
        }
        if let Some(a) = formato.antes.filter(|a| *a > 0.0) {
            let _ = write!(css, "margin-top:{a:.1}pt;");
        }
        if let Some(d) = formato.despues.filter(|d| *d > 0.0) {
            let _ = write!(css, "margin-bottom:{d:.1}pt;");
        }
        if let Some(l) = formato.interlineado_pt {
            let _ = write!(css, "line-height:{l:.1}pt;");
        } else if let Some(l) = formato.interlineado {
            let _ = write!(css, "line-height:{l:.3};");
        }
        let css_letra = letra_parrafo.css(&self.base);
        let ultima = partes.len() - 1;
        for (i, parte) in partes.iter().enumerate() {
            if i > 0 {
                html.push_str(SALTO);
                // Texto y después el salto: la hoja nueva empieza con el párrafo siguiente.
                if i == ultima && parte.trim().is_empty() {
                    break;
                }
            }
            let _ = write!(html, "<p style=\"{css}{css_letra}\">");
            if i == 0 && !marca.is_empty() {
                let _ = write!(html, "<span>{}</span>\u{2002}", escapar(&marca));
            }
            // Un párrafo vacío también ocupa su renglón, como en Word.
            if parte.trim().is_empty() && (i > 0 || marca.is_empty()) {
                html.push_str("&#160;");
            } else {
                html.push_str(parte);
            }
            html.push_str("</p>");
        }
    }

    /// Los trozos de texto del párrafo, cada uno con su formato. Un salto de página queda marcado.
    fn corridas(&mut self, n: Node, letra_parrafo: &Letra, html: &mut String) {
        for c in n.children().filter(|c| c.is_element()) {
            if es(c, "r") {
                let rpr = hijo(c, "rPr");
                let estilo_letra = rpr.and_then(|r| val(r, "rStyle")).map(|i| self.estilo(i).0).unwrap_or_default();
                let letra = letra_parrafo.sobre(&estilo_letra).sobre(&rpr.map(Letra::leer).unwrap_or_default());
                let mut texto = String::new();
                for t in c.children().filter(|t| t.is_element()) {
                    if es(t, "t") {
                        texto.push_str(&escapar(t.text().unwrap_or("")));
                    } else if es(t, "tab") || es(t, "ptab") {
                        texto.push_str("\u{2003}\u{2003}");
                    } else if es(t, "br") || es(t, "cr") {
                        if atr(t, "type") == Some("page") {
                            texto.push_str("\u{0}SALTO\u{0}");
                        } else {
                            texto.push_str("<br/>");
                        }
                    } else if es(t, "noBreakHyphen") {
                        texto.push('\u{2011}');
                    } else if es(t, "sym") {
                        texto.push('•');
                    }
                }
                if texto.is_empty() {
                    continue;
                }
                let css = letra.css(letra_parrafo);
                if css.is_empty() {
                    html.push_str(&texto);
                } else {
                    // El salto de página no puede quedar dentro de un <span>.
                    let trozos: Vec<&str> = texto.split("\u{0}SALTO\u{0}").collect();
                    for (i, t) in trozos.iter().enumerate() {
                        if i > 0 {
                            html.push_str("\u{0}SALTO\u{0}");
                        }
                        if !t.is_empty() {
                            let _ = write!(html, "<span style=\"{css}\">{t}</span>");
                        }
                    }
                }
            } else if es(c, "hyperlink") || es(c, "smartTag") || es(c, "fldSimple") || es(c, "ins") || es(c, "customXml") {
                self.corridas(c, letra_parrafo, html);
            } else if es(c, "sdt") {
                if let Some(s) = hijo(c, "sdtContent") {
                    self.corridas(s, letra_parrafo, html);
                }
            }
        }
    }

    fn tabla(&mut self, t: Node, html: &mut String) {
        let tblpr = hijo(t, "tblPr");
        let estilo = tblpr.and_then(|p| val(p, "tblStyle")).unwrap_or("");
        let bordes_propios = tblpr.and_then(|p| hijo(p, "tblBorders")).map(|b| {
            b.children().filter(|x| x.is_element()).any(|x| !matches!(atr(x, "val"), Some("none" | "nil") | None))
        });
        let con_borde = bordes_propios.unwrap_or_else(|| estilo.to_ascii_lowercase().contains("grid"));
        let anchos: Vec<f32> = hijo(t, "tblGrid")
            .map(|g| g.children().filter(|c| es(*c, "gridCol")).map(|c| numero(atr(c, "w")).unwrap_or(0.0) / 20.0).collect())
            .unwrap_or_default();
        let total: f32 = anchos.iter().sum();
        let _ = write!(html, "<table{}", if con_borde { " class=\"borde\"" } else { "" });
        if total > 0.0 {
            let _ = write!(html, " style=\"width:{total:.1}pt\"");
        }
        html.push('>');
        for fila in t.children().filter(|f| es(*f, "tr")) {
            html.push_str("<tr>");
            let mut col = 0usize;
            for celda in fila.children().filter(|c| es(*c, "tc")) {
                let tcpr = hijo(celda, "tcPr");
                let span = tcpr.and_then(|p| val(p, "gridSpan")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
                // Celda unida con la de arriba: no se repite.
                if tcpr.and_then(|p| hijo(p, "vMerge")).is_some_and(|v| atr(v, "val") != Some("restart")) {
                    col += span;
                    continue;
                }
                let ancho: f32 = anchos.iter().skip(col).take(span).sum();
                col += span;
                let _ = write!(html, "<td");
                if span > 1 {
                    let _ = write!(html, " colspan=\"{span}\"");
                }
                if ancho > 0.0 {
                    let _ = write!(html, " style=\"width:{ancho:.1}pt\"");
                }
                html.push('>');
                self.bloques(celda, html);
                html.push_str("</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
    }

    /// El texto de la viñeta o del número de la lista, y su sangría.
    fn marca_lista(&mut self, id: &str, nivel: usize) -> Option<(String, Option<f32>, Option<f32>)> {
        let niveles = self.listas.get(id)?;
        let n = niveles.get(nivel)?;
        let inicios: Vec<u32> = niveles.iter().map(|n| n.inicio).collect();
        let cuenta = self.contadores.entry(id.to_string()).or_insert_with(|| inicios.iter().map(|i| i.saturating_sub(1)).collect());
        if let Some(c) = cuenta.get_mut(nivel) {
            *c += 1;
        }
        // Al subir de nivel, los de más adentro vuelven a empezar.
        for (k, c) in cuenta.iter_mut().enumerate().skip(nivel + 1) {
            *c = inicios.get(k).copied().unwrap_or(1).saturating_sub(1);
        }
        let texto = if n.formato == "bullet" || n.formato == "none" {
            match n.texto.chars().next() {
                Some('o') => "◦".to_string(),
                Some(c) if ('\u{F000}'..='\u{F0FF}').contains(&c) || c == '\u{B7}' => "•".to_string(),
                Some('§') => "▪".to_string(),
                Some(c) if n.formato == "bullet" => c.to_string(),
                _ => String::new(),
            }
        } else {
            let mut t = n.texto.clone();
            for (k, nv) in niveles.iter().enumerate().take(nivel + 1) {
                let valor = cuenta.get(k).copied().unwrap_or(1);
                t = t.replace(&format!("%{}", k + 1), &formatear(valor, &nv.formato));
            }
            t
        };
        Some((texto, n.izquierda, n.colgante))
    }
}

fn leer_listas(raiz: Node) -> HashMap<String, Vec<Nivel>> {
    let mut abstractas = HashMap::new();
    for a in raiz.children().filter(|n| es(*n, "abstractNum")) {
        let Some(id) = atr(a, "abstractNumId") else { continue };
        let mut niveles: Vec<Nivel> = Vec::new();
        for l in a.children().filter(|n| es(*n, "lvl")) {
            let ind = hijo(l, "pPr").and_then(|p| hijo(p, "ind"));
            niveles.push(Nivel {
                formato: val(l, "numFmt").unwrap_or("decimal").to_string(),
                texto: val(l, "lvlText").unwrap_or("").to_string(),
                inicio: val(l, "start").and_then(|v| v.parse().ok()).unwrap_or(1),
                izquierda: ind.and_then(|i| numero(atr(i, "left").or_else(|| atr(i, "start")))).map(|v| v / 20.0),
                colgante: ind.and_then(|i| numero(atr(i, "hanging"))).map(|v| v / 20.0),
            });
        }
        abstractas.insert(id.to_string(), niveles);
    }
    let mut listas = HashMap::new();
    for n in raiz.children().filter(|n| es(*n, "num")) {
        let (Some(id), Some(abs)) = (atr(n, "numId"), val(n, "abstractNumId")) else { continue };
        // Dos listas pueden usar la misma definición: cada una lleva su copia y cuenta por separado.
        if let Some(niveles) = abstractas.get(abs) {
            listas.insert(id.to_string(), niveles.clone());
        }
    }
    listas
}

fn formatear(n: u32, formato: &str) -> String {
    match formato {
        "lowerLetter" => letra_de(n).to_lowercase(),
        "upperLetter" => letra_de(n),
        "lowerRoman" => romano(n).to_lowercase(),
        "upperRoman" => romano(n),
        "decimalZero" => format!("{n:02}"),
        _ => n.to_string(),
    }
}

fn letra_de(n: u32) -> String {
    let n = n.max(1) - 1;
    let c = char::from(b'A' + (n % 26) as u8);
    c.to_string().repeat((n / 26 + 1) as usize)
}

fn romano(mut n: u32) -> String {
    let tabla = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    for (v, r) in tabla {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

fn color_resaltado(c: &str) -> Option<&'static str> {
    Some(match c {
        "yellow" => "#ffff00",
        "green" => "#00ff00",
        "cyan" => "#00ffff",
        "magenta" => "#ff00ff",
        "blue" => "#0000ff",
        "red" => "#ff0000",
        "darkBlue" => "#000080",
        "darkCyan" => "#008080",
        "darkGreen" => "#008000",
        "darkMagenta" => "#800080",
        "darkRed" => "#800000",
        "darkYellow" => "#808000",
        "darkGray" => "#808080",
        "lightGray" => "#c0c0c0",
        "black" => "#000000",
        _ => return None,
    })
}

/// Letras que trae Dakarta: con serifa (como Times New Roman), sin serifa (como Arial o Calibri) y de máquina.
fn familia_css(fuente: Option<&str>) -> &'static str {
    let f = fuente.unwrap_or("").to_ascii_lowercase();
    if ["courier", "consolas", "mono", "lucida console"].iter().any(|x| f.contains(x)) {
        "monospace"
    } else if ["times", "cambria", "georgia", "garamond", "book", "serif", "palatino", "century"].iter().any(|x| f.contains(x)) && !f.contains("sans") {
        "serif"
    } else if f.is_empty() {
        "serif"
    } else {
        "sans-serif"
    }
}

fn escapar(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

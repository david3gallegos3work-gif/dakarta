//! Pruebas del motor: lo que se guarda, se mueve, se busca y se deshace.

use super::*;

fn doc_de(ancho: f32, alto: f32) -> PdfDocument {
    let mut doc = PdfDocument::new();
    doc.new_page((ancho, alto)).unwrap();
    doc
}

#[test]
fn un_sello_en_una_hoja_muy_pequena_no_detiene_el_motor() {
    let mut doc = doc_de(40.0, 20.0);
    assert!(anotar_sello(&mut doc, 0, 0.5, 0.5, "APROBADO", (1.0, 0.0, 0.0), 40.0).is_ok());
}

#[test]
fn no_se_guarda_encima_del_pdf_abierto_y_lo_guardado_se_vuelve_a_abrir() {
    let carpeta = std::env::temp_dir().join(format!("dakarta-prueba-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let original = carpeta.join("original.pdf").to_string_lossy().into_owned();
    let copia = carpeta.join("copia.pdf").to_string_lossy().into_owned();

    guardar_doc(&doc_de(300.0, 400.0), &original).unwrap();
    assert!(mismo_archivo(&original, &original));
    assert!(!mismo_archivo(&original, &copia));

    let mut doc = PdfDocument::open(&original).unwrap();
    anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 24.0).unwrap();
    guardar_doc(&doc, &copia).unwrap();
    // Guardar otra vez reemplaza la copia sin dejar el archivo temporal.
    guardar_doc(&doc, &copia).unwrap();
    assert!(!std::path::Path::new(&format!("{copia}.guardando")).exists());
    assert_eq!(PdfDocument::open(&copia).unwrap().page_count().unwrap(), 1);

    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn un_enlace_del_indice_lleva_a_su_pagina() {
    // PDF mínimo: la página 1 tiene un enlace (como una línea del índice) que lleva a la página 3.
    let pdf = "%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R 5 0 R]/Count 3>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]/Annots[6 0 R 7 0 R]>>endobj
4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj
5 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj
6 0 obj<</Type/Annot/Subtype/Link/Rect[20 250 180 270]/Dest[5 0 R/Fit]>>endobj
7 0 obj<</Type/Annot/Subtype/Link/Rect[20 200 180 220]/A<</S/URI/URI(https://example.com)>>>>endobj
trailer<</Root 1 0 R>>
%%EOF";
    let ruta = std::env::temp_dir().join(format!("dakarta-enlaces-{}.pdf", std::process::id()));
    std::fs::write(&ruta, pdf).unwrap();
    let doc = PdfDocument::open(ruta.to_str().unwrap()).unwrap();
    let lista = enlaces_de(&doc, 0).unwrap();
    std::fs::remove_file(&ruta).unwrap();
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].pagina, 2);
    // Arriba de la hoja: y = (300 - 270) / 300.
    assert!((lista[0].y - 0.1).abs() < 0.01 && (lista[0].x - 0.1).abs() < 0.01);
}

fn anotacion(doc: &PdfDocument, n: i32, xref: i32) -> PdfAnnotation {
    doc.load_pdf_page(n).unwrap().annotations().find(|a| a.xref().ok() == Some(xref)).unwrap()
}

#[test]
fn mover_un_sello_conserva_su_numero_y_el_centrado_y_se_puede_deshacer() {
    let mut doc = doc_de(300.0, 400.0);
    let xref = anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 18.0).unwrap();
    let antes = datos_de(&doc, 0, xref).unwrap();
    assert_eq!(mover_anotacion(&mut doc, 0, xref, 0.1, 0.0).unwrap(), xref);
    let a = anotacion(&doc, 0, xref);
    assert!(matches!(a.quadding().unwrap(), AnnotationTextAlign::Center));
    assert!((a.rect().unwrap().x0 - antes.rect.x0 - 30.0).abs() < 0.5);
    let mut hechas = vec![Hecho::Cambio { n: 0, xref, antes: antes.clone() }];
    assert_eq!(deshacer(&mut doc, &mut hechas).unwrap(), Some(0));
    assert!(hechas.is_empty());
    assert!((datos_de(&doc, 0, xref).unwrap().rect.x0 - antes.rect.x0).abs() < 0.5);
}

#[test]
fn si_deshacer_falla_el_paso_no_se_pierde() {
    let mut doc = doc_de(300.0, 400.0);
    let antes = {
        let x = anotar_sello(&mut doc, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 18.0).unwrap();
        datos_de(&doc, 0, x).unwrap()
    };
    // Un número de anotación que no existe: deshacer falla.
    let mut hechas = vec![Hecho::Cambio { n: 0, xref: 99_999, antes }];
    assert!(deshacer(&mut doc, &mut hechas).is_err());
    assert_eq!(hechas.len(), 1);
}

#[test]
fn lo_borrado_no_queda_escondido_en_la_copia_guardada() {
    let carpeta = std::env::temp_dir().join(format!("dakarta-borrado-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let original = carpeta.join("original.pdf").to_string_lossy().into_owned();
    let copia = carpeta.join("copia.pdf").to_string_lossy().into_owned();
    let mut doc = doc_de(300.0, 400.0);
    anotar_caja(&mut doc, 0, 0.1, 0.1, "SECRETO-123", (0.0, 0.0, 0.0), 12.0, false).unwrap();
    guardar_doc(&doc, &original).unwrap();
    let mut doc = PdfDocument::open(&original).unwrap();
    let xref = doc.load_pdf_page(0).unwrap().annotations().next().unwrap().xref().unwrap();
    borrar_anotacion(&mut doc, 0, xref).unwrap();
    let mut opciones = PdfWriteOptions::default();
    opciones.set_compress(false);
    opciones.set_garbage_level(1);
    doc.save_with_options(&copia, opciones).unwrap();
    let bytes = std::fs::read(&copia).unwrap();
    assert!(!bytes.windows(11).any(|w| w == b"SECRETO-123"));
    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn la_busqueda_avanza_por_tandas_y_encuentra_en_todas_las_paginas() {
    // 10 páginas con la palabra "hola": la búsqueda necesita varias tandas de 4.
    let mut objs = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        String::new(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string(),
    ];
    let mut kids = Vec::new();
    for _ in 0..10 {
        let contenido = "BT /F1 12 Tf 20 100 Td (hola mundo) Tj ET";
        objs.push(format!("<</Length {}>>stream\n{}\nendstream", contenido.len(), contenido));
        let c = objs.len();
        objs.push(format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Resources<</Font<</F1 3 0 R>>>>/Contents {c} 0 R>>"));
        kids.push(format!("{} 0 R", objs.len()));
    }
    objs[1] = format!("<</Type/Pages/Kids[{}]/Count 10>>", kids.join(" "));
    let mut pdf = String::from("%PDF-1.4\n");
    for (i, o) in objs.iter().enumerate() {
        pdf.push_str(&format!("{} 0 obj{}endobj\n", i + 1, o));
    }
    pdf.push_str("trailer<</Root 1 0 R>>\n%%EOF");
    let ruta = std::env::temp_dir().join(format!("dakarta-busqueda-{}.pdf", std::process::id()));
    std::fs::write(&ruta, pdf).unwrap();
    let doc = PdfDocument::open(ruta.to_str().unwrap()).unwrap();
    let (tx, rx) = mpsc::channel();
    let mut b = BusquedaEnCurso::nueva("hola", tx).ok().unwrap();
    let mut tandas = 1;
    while !b.avanzar(&doc).unwrap() {
        tandas += 1;
    }
    b.terminar();
    let r = rx.recv().unwrap().unwrap();
    std::fs::remove_file(&ruta).unwrap();
    assert_eq!(tandas, 3);
    assert_eq!(r.resultados.len(), 10);
    assert!(!r.escaneado);
}

#[test]
fn las_casillas_usan_el_valor_de_su_pdf_y_los_botones_no_son_campos() {
    // Formulario de prueba con datos inventados: casillas "acepto" y "noticias", opciones F/M.
    let ruta = concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf");
    let mut doc = PdfDocument::open(ruta).unwrap();
    let campos = campos_doc(&doc).unwrap();
    assert!(campos.iter().all(|c| c.tipo != "Button"));
    let acepto = campos.iter().find(|c| c.nombre == "acepto").unwrap();
    assert_eq!(acepto.tipo, "Checkbox");
    assert!(!acepto.encendido.is_empty() && acepto.encendido != "Off");
    let (xref, encendido) = (acepto.xref, acepto.encendido.clone());
    fijar_campo(&mut doc, 0, xref, &encendido).unwrap();
    let ahora = campos_doc(&doc).unwrap();
    assert_eq!(ahora.iter().find(|c| c.xref == xref).unwrap().valor, encendido);
    // Opciones F y M: cada una con su propio valor, que no es "Yes".
    let opciones: Vec<_> = ahora.iter().filter(|c| c.nombre == "sexo").collect();
    assert_eq!(opciones.len(), 2);
    let m = opciones.iter().find(|c| c.encendido == "M").expect("la opción M usa su propio valor");
    fijar_campo(&mut doc, 0, m.xref, "M").unwrap();
    let despues = campos_doc(&doc).unwrap();
    assert!(despues.iter().filter(|c| c.nombre == "sexo").all(|c| c.valor == "M"));
    // Lo que se ve: solo la M queda marcada; la otra opción del grupo se apaga.
    let estados: Vec<String> = doc
        .load_pdf_page(0)
        .unwrap()
        .widgets()
        .filter(|w| w.name().unwrap().as_deref() == Some("sexo"))
        .map(|w| String::from_utf8_lossy(&w.annotation().object().get_dict("AS").unwrap().unwrap().as_name().unwrap()).into_owned())
        .collect();
    assert_eq!(estados.iter().filter(|e| *e != "Off").collect::<Vec<_>>(), vec!["M"]);
    // Y una casilla se desmarca al tocarla otra vez.
    fijar_campo(&mut doc, 0, xref, "Off").unwrap();
    assert_eq!(campos_doc(&doc).unwrap().iter().find(|c| c.xref == xref).unwrap().valor, "Off");
}

#[test]
fn la_nota_lleva_fondo_amarillo_y_lo_conserva_al_moverla() {
    let mut doc = doc_de(300.0, 400.0);
    let nota = anotar_caja(&mut doc, 0, 0.1, 0.1, "Nota", (0.0, 0.0, 0.0), 12.0, true).unwrap();
    let texto = anotar_caja(&mut doc, 0, 0.1, 0.5, "Texto", (0.0, 0.0, 0.0), 12.0, false).unwrap();
    mover_anotacion(&mut doc, 0, nota, 0.2, 0.0).unwrap();
    assert!(matches!(anotacion(&doc, 0, nota).color().unwrap(), Some(AnnotationColor::Rgb { .. })));
    assert!(anotacion(&doc, 0, texto).color().unwrap().is_none());
}

#[test]
fn lo_escrito_de_un_tiron_se_deshace_en_un_solo_paso() {
    let ruta = std::env::temp_dir().join(format!("dakarta-tanda-{}.pdf", std::process::id()));
    let ruta = ruta.to_string_lossy().into_owned();
    guardar_doc(&doc_de(300.0, 400.0), &ruta).unwrap();
    let motor = Motor::nuevo();
    motor.abrir(ruta.clone(), "prueba.pdf".into(), None).unwrap();
    motor.anotar_caja(0, 0.1, 0.1, "Nota".into(), (0.0, 0.0, 0.0), 12.0, true).unwrap();
    let xref = motor.anotaciones(0).unwrap()[0].xref;
    let negro = Some((0.0, 0.0, 0.0));
    // Tres pausas mientras se escribe: el primer guardado abre el paso, los siguientes se le unen.
    assert!(motor.editar_anotacion(0, xref, Some("Nota de".into()), negro, None, false).unwrap());
    assert!(!motor.editar_anotacion(0, xref, Some("Nota de prueba".into()), negro, None, true).unwrap());
    assert!(!motor.editar_anotacion(0, xref, Some("Nota de prueba\nlarga".into()), negro, None, true).unwrap());
    assert_eq!(motor.anotaciones(0).unwrap()[0].contenido, "Nota de prueba\nlarga");
    motor.deshacer().unwrap();
    assert_eq!(motor.anotaciones(0).unwrap()[0].contenido, "Nota");
    std::fs::remove_file(&ruta).unwrap();
}

#[test]
fn el_cuadro_de_texto_queda_pegado_al_texto() {
    let (ancho, alto) = medida_texto("Texto", 12.0, 500.0).unwrap();
    // "Texto" en Helvetica 12 mide unos 30 puntos; antes el cuadro salía de 60 o más.
    assert!(ancho > 28.0 && ancho < 42.0, "ancho {ancho}");
    assert!(alto > 12.0 && alto < 22.0, "alto {alto}");
}

#[test]
fn el_enter_y_las_lineas_largas_hacen_mas_alto_el_cuadro() {
    let (_, una) = medida_texto("Hola", 12.0, 500.0).unwrap();
    let (_, dos) = medida_texto("Hola\nmundo", 12.0, 500.0).unwrap();
    assert!((dos - una - 14.4).abs() < 0.01);
    let largo = "palabra ".repeat(40);
    let (ancho, alto) = medida_texto(largo.trim(), 12.0, 200.0).unwrap();
    assert!(ancho <= 200.0 && alto > 3.0 * 14.4, "{ancho} × {alto}");
}

#[test]
fn un_texto_nuevo_y_al_agrandarlo_el_cuadro_sigue_al_texto() {
    let mut doc = doc_de(300.0, 400.0);
    let xref = anotar_caja(&mut doc, 0, 0.1, 0.1, "Texto", (0.0, 0.0, 0.0), 12.0, false).unwrap();
    let antes = datos_de(&doc, 0, xref).unwrap().rect;
    assert!(antes.width() < 42.0, "{antes:?}");
    // Arrastrar la esquina al doble: la letra crece y el cuadro vuelve a pegarse a ella.
    redimensionar_anotacion(&mut doc, 0, xref, 0.1, 0.1, antes.width() * 2.0 / 300.0, antes.height() * 2.0 / 400.0).unwrap();
    let d = datos_de(&doc, 0, xref).unwrap();
    assert!((d.tamano - 24.0).abs() < 0.5, "letra {}", d.tamano);
    assert!((d.rect.width() - medida_texto("Texto", d.tamano, 270.0).unwrap().0).abs() < 0.5);
}

/// PDF de prueba con `n` hojas; cada una dice "hoja N". El tamaño está en la raíz y lo heredan las hojas.
fn pdf_numerado(n: usize) -> String {
    let mut objs = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        String::new(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_string(),
    ];
    let mut kids = Vec::new();
    for i in 0..n {
        let contenido = format!("BT /F1 12 Tf 20 100 Td (hoja {}) Tj ET", i + 1);
        objs.push(format!("<</Length {}>>stream\n{}\nendstream", contenido.len(), contenido));
        let c = objs.len();
        objs.push(format!("<</Type/Page/Parent 2 0 R/Resources<</Font<</F1 3 0 R>>>>/Contents {c} 0 R>>"));
        kids.push(format!("{} 0 R", objs.len()));
    }
    objs[1] = format!("<</Type/Pages/Kids[{}]/Count {n}/MediaBox[0 0 200 300]>>", kids.join(" "));
    let mut pdf = String::from("%PDF-1.4\n");
    for (i, o) in objs.iter().enumerate() {
        pdf.push_str(&format!("{} 0 obj{}endobj\n", i + 1, o));
    }
    pdf.push_str("trailer<</Root 1 0 R>>\n%%EOF");
    pdf
}

#[test]
fn organizar_mueve_gira_quita_y_duplica_sin_tocar_el_abierto() {
    let carpeta = std::env::temp_dir().join(format!("dakarta-organizar-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let origen = carpeta.join("tesis.pdf");
    std::fs::write(&origen, pdf_numerado(4)).unwrap();
    let destino = carpeta.join("organizado.pdf").to_string_lossy().into_owned();
    let mut doc = PdfDocument::open(origen.to_str().unwrap()).unwrap();
    // Una nota sin guardar en la hoja 2: debe ir con su hoja (y con la copia).
    anotar_caja(&mut doc, 1, 0.1, 0.1, "Ojo", (0.0, 0.0, 0.0), 12.0, true).unwrap();
    let hojas = [
        Hoja { pagina: 3, giro: 90 },
        Hoja { pagina: 1, giro: 0 },
        Hoja { pagina: 1, giro: 0 },
        Hoja { pagina: 0, giro: -90 },
    ];
    organizar_y_guardar(&doc, &hojas, &destino).unwrap();
    assert_eq!(doc.page_count().unwrap(), 4, "el PDF abierto no cambia");
    assert!(!std::path::Path::new(&format!("{destino}.organizando")).exists());

    let nuevo = PdfDocument::open(&destino).unwrap();
    assert_eq!(nuevo.page_count().unwrap(), 4);
    let textos: Vec<String> = (0..4)
        .map(|i| nuevo.load_page(i).unwrap().text(TextExtractOptions::default()).unwrap().trim().to_string())
        .collect();
    assert_eq!(textos, ["hoja 4", "hoja 2", "hoja 2", "hoja 1"]);
    let giros: Vec<i32> = (0..4).map(|i| nuevo.load_pdf_page(i).unwrap().rotation().unwrap()).collect();
    assert_eq!(giros, [90, 0, 0, 270]);
    let notas: Vec<usize> = (0..4).map(|i| nuevo.load_pdf_page(i).unwrap().annotations().count()).collect();
    assert_eq!(notas, [0, 1, 1, 0]);
    // La hoja heredaba su tamaño: sigue midiendo lo mismo.
    let b = nuevo.load_page(1).unwrap().bounds().unwrap();
    assert!((b.width() - 200.0).abs() < 0.5 && (b.height() - 300.0).abs() < 0.5);
    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn organizar_rechaza_una_lista_vacia_o_con_paginas_que_no_existen() {
    let doc = doc_de(300.0, 400.0);
    assert!(organizar_y_guardar(&doc, &[], "/tmp/no-se-usa.pdf").is_err());
    assert!(organizar_y_guardar(&doc, &[Hoja { pagina: 5, giro: 0 }], "/tmp/no-se-usa.pdf").is_err());
}

#[test]
fn duplicar_una_hoja_con_formulario_no_duplica_las_casillas() {
    let ruta = concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf");
    let doc = PdfDocument::open(ruta).unwrap();
    let destino = std::env::temp_dir().join(format!("dakarta-dup-form-{}.pdf", std::process::id()));
    let destino = destino.to_string_lossy().into_owned();
    organizar_y_guardar(&doc, &[Hoja { pagina: 0, giro: 0 }, Hoja { pagina: 0, giro: 0 }], &destino).unwrap();
    let mut nuevo = PdfDocument::open(&destino).unwrap();
    assert_eq!(nuevo.page_count().unwrap(), 2);
    assert!(nuevo.load_pdf_page(0).unwrap().widgets().count() > 0);
    assert_eq!(nuevo.load_pdf_page(1).unwrap().widgets().count(), 0);
    let acepto = campos_doc(&nuevo).unwrap().into_iter().find(|c| c.nombre == "acepto").unwrap();
    fijar_campo(&mut nuevo, 0, acepto.xref, &acepto.encendido).unwrap();
    std::fs::remove_file(&destino).unwrap();
}

fn prueba(nombre: &str) -> String {
    format!("{}/pruebas/{nombre}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn reconoce_el_tipo_por_el_nombre_o_por_los_primeros_bytes() {
    assert_eq!(tipo_de("Informe.DOCX", b""), Some("docx"));
    assert_eq!(tipo_de("libro.epub", b""), Some("epub"));
    // Android: la ruta no trae nombre; se mira el contenido.
    let cab = |n: &str| leer_cabecera(&prueba(n)).unwrap();
    assert_eq!(tipo_de("57", &cab("libro.epub")), Some("epub"));
    assert_eq!(tipo_de("57", &cab("documento.docx")), Some("docx"));
    assert_eq!(tipo_de("57", &cab("foto.jpg")), Some("jpg"));
    assert_eq!(tipo_de("57", &cab("nota.txt")), Some("txt"));
    assert_eq!(tipo_de("57", &[0u8, 1, 2, 3, 200, 7]), None);
}

#[test]
fn abre_word_epub_txt_e_imagen_para_leer_buscar_y_copiar() {
    let motor = Motor::nuevo();
    for (archivo, palabra) in [("documento.docx", "Informe"), ("libro.epub", "Capítulo"), ("nota.txt", "Jamón"), ("foto.jpg", "")] {
        // Se copian a un nombre sin extensión, como llegan en Android.
        let sin_nombre = std::env::temp_dir().join(format!("dakarta-fmt-{}-{archivo}-57", std::process::id()));
        std::fs::copy(prueba(archivo), &sin_nombre).unwrap();
        for (ruta, nombre) in [(prueba(archivo), archivo.to_string()), (sin_nombre.to_string_lossy().into_owned(), "57".into())] {
            let info = motor.abrir(ruta, nombre, None).unwrap_or_else(|e| panic!("{archivo}: {e}"));
            assert!(!info.pdf, "{archivo}");
            assert!(info.paginas >= 1, "{archivo}");
            if !palabra.is_empty() {
                let texto: String = motor.palabras(0).unwrap().into_iter().map(|p| p.texto).collect();
                assert!(texto.contains(palabra), "{archivo}: {texto}");
            }
            assert!(motor.anotar_caja(0, 0.1, 0.1, "x".into(), (0.0, 0.0, 0.0), 12.0, false).unwrap_err().contains("solo se puede"));
            assert!(motor.anotaciones(0).unwrap().is_empty());
        }
        std::fs::remove_file(&sin_nombre).unwrap();
    }
}

#[test]
fn un_word_antiguo_da_un_mensaje_claro() {
    let ruta = std::env::temp_dir().join(format!("dakarta-doc-viejo-{}", std::process::id()));
    let mut bytes = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1".to_vec();
    bytes.resize(2048, 0);
    std::fs::write(&ruta, bytes).unwrap();
    let e = abrir_doc(ruta.to_str().unwrap(), "informe.docx").err().unwrap();
    assert!(e.contains("Office antiguo"), "{e}");
    std::fs::remove_file(&ruta).unwrap();
}

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
    motor.abrir(ruta.clone(), None, None).unwrap();
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
        Hoja { pagina: 3, giro: 90, archivo: ABIERTO },
        Hoja { pagina: 1, giro: 0, archivo: ABIERTO },
        Hoja { pagina: 1, giro: 0, archivo: ABIERTO },
        Hoja { pagina: 0, giro: -90, archivo: ABIERTO },
    ];
    organizar_y_guardar(&doc, &hojas, &destino, None).unwrap();
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
    assert!(organizar_y_guardar(&doc, &[], "/tmp/no-se-usa.pdf", None).is_err());
    assert!(organizar_y_guardar(&doc, &[Hoja { pagina: 5, giro: 0, archivo: ABIERTO }], "/tmp/no-se-usa.pdf", None).is_err());
}

#[test]
fn duplicar_una_hoja_con_formulario_no_duplica_las_casillas() {
    let ruta = concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf");
    let doc = PdfDocument::open(ruta).unwrap();
    let destino = std::env::temp_dir().join(format!("dakarta-dup-form-{}.pdf", std::process::id()));
    let destino = destino.to_string_lossy().into_owned();
    organizar_y_guardar(&doc, &[Hoja { pagina: 0, giro: 0, archivo: ABIERTO }, Hoja { pagina: 0, giro: 0, archivo: ABIERTO }], &destino, None).unwrap();
    let mut nuevo = PdfDocument::open(&destino).unwrap();
    assert_eq!(nuevo.page_count().unwrap(), 2);
    assert!(nuevo.load_pdf_page(0).unwrap().widgets().count() > 0);
    assert_eq!(nuevo.load_pdf_page(1).unwrap().widgets().count(), 0);
    let acepto = campos_doc(&nuevo).unwrap().into_iter().find(|c| c.nombre == "acepto").unwrap();
    fijar_campo(&mut nuevo, 0, acepto.xref, &acepto.encendido).unwrap();
    std::fs::remove_file(&destino).unwrap();
}

/// PDF de prueba en memoria con hojas que dicen «<prefijo> N».
fn pdf_con_texto(prefijo: &str, n: usize) -> PdfDocument {
    // Nombre único: las pruebas corren a la vez y no deben pisarse el archivo.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let k = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ruta = std::env::temp_dir().join(format!("dakarta-{prefijo}-{}-{k}.pdf", std::process::id()));
    std::fs::write(&ruta, pdf_numerado(n).replace("(hoja ", &format!("({prefijo} "))).unwrap();
    let doc = PdfDocument::open(ruta.to_str().unwrap()).unwrap();
    std::fs::remove_file(&ruta).unwrap();
    doc
}

fn textos(doc: &PdfDocument) -> Vec<String> {
    (0..doc.page_count().unwrap())
        .map(|i| doc.load_page(i).unwrap().text(TextExtractOptions::default()).unwrap().trim().to_string())
        .collect()
}

#[test]
fn unir_intercala_hojas_de_dos_pdfs_con_sus_notas() {
    let mut a = pdf_con_texto("a", 2);
    let mut b = pdf_con_texto("b", 3);
    anotar_caja(&mut a, 1, 0.1, 0.1, "Nota de A", (0.0, 0.0, 0.0), 12.0, true).unwrap();
    anotar_sello(&mut b, 0, 0.5, 0.5, "PAGADO", (0.0, 0.5, 0.0), 18.0).unwrap();
    let destino = std::env::temp_dir().join(format!("dakarta-unido-{}.pdf", std::process::id()));
    let destino = destino.to_string_lossy().into_owned();
    let hojas = [
        Hoja { pagina: 0, giro: 0, archivo: 0 },
        Hoja { pagina: 1, giro: 90, archivo: ABIERTO },
        Hoja { pagina: 2, giro: 0, archivo: 0 },
        Hoja { pagina: 0, giro: 0, archivo: ABIERTO },
    ];
    unir_y_guardar(Some((&a, None)), &[(&b, None)], &hojas, &destino).unwrap();
    assert_eq!((a.page_count().unwrap(), b.page_count().unwrap()), (2, 3), "los originales no cambian");
    assert!(!std::path::Path::new(&format!("{destino}.uniendo")).exists());
    let u = PdfDocument::open(&destino).unwrap();
    assert_eq!(textos(&u), ["b 1", "a 2", "b 3", "a 1"]);
    let giros: Vec<i32> = (0..4).map(|i| u.load_pdf_page(i).unwrap().rotation().unwrap()).collect();
    assert_eq!(giros, [0, 90, 0, 0]);
    let notas: Vec<usize> = (0..4).map(|i| u.load_pdf_page(i).unwrap().annotations().count()).collect();
    assert_eq!(notas, [1, 1, 0, 0], "el sello va con b 1 y la nota con a 2");
    std::fs::remove_file(&destino).unwrap();
}

#[test]
fn unir_sin_pdf_abierto_y_con_formulario_agregado() {
    let b = pdf_con_texto("b", 2);
    let form = PdfDocument::open(concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf")).unwrap();
    let destino = std::env::temp_dir().join(format!("dakarta-unido2-{}.pdf", std::process::id()));
    let destino = destino.to_string_lossy().into_owned();
    let hojas = [Hoja { pagina: 1, giro: 0, archivo: 0 }, Hoja { pagina: 0, giro: 0, archivo: 1 }];
    unir_y_guardar(None, &[(&b, None), (&form, None)], &hojas, &destino).unwrap();
    let u = PdfDocument::open(&destino).unwrap();
    assert_eq!(u.page_count().unwrap(), 2);
    assert_eq!(textos(&u)[0], "b 2");
    assert!(textos(&u)[1].contains("Formulario de prueba"));
    // Las casillas del segundo PDF no se copian (serían campos sueltos que no se llenan).
    assert_eq!(u.load_pdf_page(1).unwrap().widgets().count(), 0);
    // Sin PDF abierto, pedir una hoja «del abierto» es un error claro.
    assert!(unir_y_guardar(None, &[(&b, None)], &[Hoja { pagina: 0, giro: 0, archivo: ABIERTO }], &destino).is_err());
    std::fs::remove_file(&destino).unwrap();
}

#[test]
fn girar_texto_en_pasos_de_90_sigue_siendo_texto_y_se_deshace() {
    let mut doc = doc_de(300.0, 400.0);
    let t = anotar_caja(&mut doc, 0, 0.3, 0.3, "Texto girado", (0.0, 0.0, 0.0), 14.0, true).unwrap();
    let antes = datos_de(&doc, 0, t).unwrap();
    // 80° con el dedo se redondea a 90°: el cuadro queda de lado, con el texto intacto.
    girar_anotacion(&mut doc, 0, t, 80.0).unwrap();
    let d = datos_de(&doc, 0, t).unwrap();
    assert_eq!(d.giro, 270, "/Rotate gira contra el reloj: 90° con el reloj es 270");
    assert!((d.rect.width() - antes.rect.height()).abs() < 0.5 && (d.rect.height() - antes.rect.width()).abs() < 0.5);
    assert_eq!(d.contenido, "Texto girado");
    // Cambiar el texto de un cuadro girado lo deja girado y a la medida.
    editar_anotacion(&mut doc, 0, t, Some("Otro texto más largo"), None, None).unwrap();
    let e = datos_de(&doc, 0, t).unwrap();
    assert_eq!(e.giro, 270);
    assert!(e.rect.height() > e.rect.width());
    let mut hechas = vec![Hecho::Cambio { n: 0, xref: t, antes: antes.clone() }];
    deshacer(&mut doc, &mut hechas).unwrap();
    assert_eq!(datos_de(&doc, 0, t).unwrap().giro, 0);
    // Un giro pequeño (menos de 45°) no cambia un texto.
    girar_anotacion(&mut doc, 0, t, 30.0).unwrap();
    assert_eq!(datos_de(&doc, 0, t).unwrap().giro, 0);
}

#[test]
fn girar_lapiz_a_cualquier_angulo_sin_salirse_de_la_hoja() {
    let mut doc = doc_de(300.0, 400.0);
    let k = anotar_tinta(&mut doc, 0, (0.8, 0.0, 0.0), 3.0, &[vec![(0.1, 0.5), (0.9, 0.5)]]).unwrap();
    girar_anotacion(&mut doc, 0, k, 90.0).unwrap();
    let d = datos_de(&doc, 0, k).unwrap();
    // La raya horizontal de 240 puntos queda vertical, y entera dentro de la hoja.
    assert!(d.rect.height() > 230.0 && d.rect.width() < 15.0, "{:?}", d.rect);
    assert!(d.rect.y0 >= 0.0 && d.rect.y1 <= 400.0);
    let h = anotar_texto(&mut doc, 0, TipoTexto::Resaltar, (1.0, 1.0, 0.0), (0.1, 0.1), (0.2, 0.2));
    if let Ok(x) = h {
        assert!(girar_anotacion(&mut doc, 0, x, 90.0).is_err(), "el resaltado sigue al texto: no se gira");
    }
}

#[test]
fn un_pdf_con_contrasena_se_abre_con_ella_y_la_conserva_al_organizar() {
    let carpeta = std::env::temp_dir().join(format!("dakarta-clave-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let ruta = |n: &str| carpeta.join(n).to_string_lossy().into_owned();
    let original = PdfDocument::open(concat!(env!("CARGO_MANIFEST_DIR"), "/pruebas/formulario.pdf")).unwrap();
    // Proteger: copia cifrada con la contraseña.
    guardar_doc_con(&original, &ruta("protegido.pdf"), &Proteccion::de("Ñandú-2026").unwrap(), None).unwrap();
    assert_eq!(abrir_pdf(&ruta("protegido.pdf"), None).err().unwrap(), PIDE_CLAVE);
    assert_eq!(abrir_pdf(&ruta("protegido.pdf"), Some("otra")).err().unwrap(), CLAVE_MAL);
    let (doc, info) = abrir_doc(&ruta("protegido.pdf"), Some("Ñandú-2026")).unwrap();
    assert!(info.con_clave);
    assert_eq!(doc.page_count().unwrap(), 1);
    // Guardar copia u organizar un PDF con contraseña: la copia sigue con la misma contraseña.
    guardar_doc(&doc, &ruta("copia.pdf")).unwrap();
    assert_eq!(abrir_pdf(&ruta("copia.pdf"), None).err().unwrap(), PIDE_CLAVE);
    organizar_y_guardar(&doc, &[Hoja { pagina: 0, giro: 90, archivo: ABIERTO }], &ruta("organizado.pdf"), Some("Ñandú-2026")).unwrap();
    assert!(abrir_pdf(&ruta("organizado.pdf"), Some("Ñandú-2026")).is_ok());
    assert_eq!(abrir_pdf(&ruta("organizado.pdf"), None).err().unwrap(), PIDE_CLAVE);
    // Cambiar la contraseña: la nueva abre la copia y la vieja ya no.
    guardar_doc_con(&doc, &ruta("cambiada.pdf"), &Proteccion::de("nueva").unwrap(), Some("Ñandú-2026")).unwrap();
    assert!(abrir_pdf(&ruta("cambiada.pdf"), Some("nueva")).is_ok());
    assert_eq!(abrir_pdf(&ruta("cambiada.pdf"), Some("Ñandú-2026")).err().unwrap(), CLAVE_MAL);
    // Unir un PDF con contraseña con otro: el unido lleva la del primero y las hojas de los dos.
    let otro = abrir_pdf(&ruta("cambiada.pdf"), Some("nueva")).unwrap();
    let hojas = [Hoja { pagina: 0, giro: 0, archivo: ABIERTO }, Hoja { pagina: 0, giro: 0, archivo: 0 }];
    // El abierto no cambió: sigue con su contraseña.
    unir_y_guardar(Some((&doc, Some("Ñandú-2026"))), &[(&otro, Some("nueva"))], &hojas, &ruta("unido.pdf")).unwrap();
    assert_eq!(abrir_pdf(&ruta("unido.pdf"), Some("Ñandú-2026")).unwrap().page_count().unwrap(), 2);
    assert_eq!(abrir_pdf(&ruta("unido.pdf"), None).err().unwrap(), PIDE_CLAVE);
    // Quitar la contraseña: la copia se abre sin pedirla.
    guardar_doc_con(&doc, &ruta("sin-clave.pdf"), &Proteccion::de("").unwrap(), Some("Ñandú-2026")).unwrap();
    let (_, info) = abrir_doc(&ruta("sin-clave.pdf"), None).unwrap();
    assert!(!info.con_clave);
    // Un PDF sin contraseña no pide nada aunque se mande una.
    assert!(!abrir_doc(&ruta("sin-clave.pdf"), Some("x")).unwrap().1.con_clave);
    assert!(Proteccion::de(&"x".repeat(200)).is_err());
    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn un_pdf_con_contrasena_se_lee_entero_y_no_cambia_al_proteger_una_copia() {
    // Antes, saber si tenía contraseña la hacía olvidar: el PDF abría con las hojas en blanco.
    let mut doc = pdf_con_texto("leer", 3);
    let carpeta = std::env::temp_dir().join(format!("dakarta-leer-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let ruta = |n: &str| carpeta.join(n).to_string_lossy().into_owned();
    guardar_doc_con(&doc, &ruta("p.pdf"), &Proteccion::de("abc").unwrap(), None).unwrap();
    let (p, info) = abrir_doc(&ruta("p.pdf"), Some("abc")).unwrap();
    assert!(info.con_clave);
    assert_eq!(textos(&p), ["leer 1", "leer 2", "leer 3"]);
    // Una copia del protegido (igual que al guardar anotaciones) también se lee entera.
    guardar_doc(&p, &ruta("igual.pdf")).unwrap();
    assert_eq!(textos(&abrir_pdf(&ruta("igual.pdf"), Some("abc")).unwrap()), ["leer 1", "leer 2", "leer 3"]);
    // Un PDF agregado para unir, con contraseña, aporta sus hojas con su texto.
    let hojas = [Hoja { pagina: 0, giro: 0, archivo: ABIERTO }, Hoja { pagina: 2, giro: 0, archivo: 0 }];
    unir_y_guardar(Some((&doc, None)), &[(&p, Some("abc"))], &hojas, &ruta("unido.pdf")).unwrap();
    assert_eq!(textos(&abrir_pdf(&ruta("unido.pdf"), None).unwrap()), ["leer 1", "leer 3"]);
    // Tras proteger una copia, el abierto no cambia: se sigue leyendo y se guarda sin contraseña.
    anotar_caja(&mut doc, 0, 0.1, 0.1, "Nota", (0.0, 0.0, 0.0), 12.0, true).unwrap();
    guardar_doc_con(&doc, &ruta("q.pdf"), &Proteccion::de("xyz").unwrap(), None).unwrap();
    assert_eq!(textos(&doc)[2], "leer 3");
    guardar_doc(&doc, &ruta("sigue.pdf")).unwrap();
    assert_eq!(textos(&abrir_pdf(&ruta("sigue.pdf"), None).unwrap()), ["leer 1", "leer 2", "leer 3"]);
    assert_eq!(textos(&abrir_doc(&ruta("q.pdf"), Some("xyz")).unwrap().0)[1], "leer 2");
    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn crear_pdf_desde_fotos_pone_una_hoja_por_foto() {
    // Dos «fotos»: una vertical (una hoja pintada) y una apaisada de 400×200.
    let vertical = pintar(&pdf_con_texto("foto", 1), 0, 300).unwrap();
    let mut px = Pixmap::new_with_w_h(&Colorspace::device_rgb(), 400, 200, false).unwrap();
    px.clear_with(90).unwrap();
    let apaisada = jpeg(&px, 85).unwrap();
    let carpeta = std::env::temp_dir().join(format!("dakarta-fotos-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let ruta = |n: &str| carpeta.join(n).to_string_lossy().into_owned();
    let medida = |doc: &PdfDocument, n: i32| {
        let b = doc.load_page(n).unwrap().bounds().unwrap();
        (b.width().round(), b.height().round())
    };

    pdf_de_fotos(&[vertical.clone(), apaisada.clone()], Diseno::A4, &ruta("a4.pdf")).unwrap();
    let doc = abrir_pdf(&ruta("a4.pdf"), None).unwrap();
    assert_eq!(doc.page_count().unwrap(), 2);
    assert_eq!(medida(&doc, 0), (595.0, 842.0));
    assert_eq!(medida(&doc, 1), (842.0, 595.0), "la apaisada va en A4 de lado");
    // La foto se ve: la hoja pintada no queda en blanco.
    assert!(pintar(&doc, 0, 200).unwrap().len() > pintar(&pdf_vacio(), 0, 200).unwrap().len());

    pdf_de_fotos(&[apaisada.clone()], Diseno::Foto, &ruta("forma.pdf")).unwrap();
    let doc = abrir_pdf(&ruta("forma.pdf"), None).unwrap();
    assert_eq!(medida(&doc, 0), (842.0, 421.0), "la hoja toma la forma de la foto");

    // Documentos: cédula y papeleta (frente y reverso) en una sola hoja A4.
    let docs = [apaisada.clone(), apaisada.clone(), apaisada.clone(), apaisada];
    pdf_de_fotos(&docs, Diseno::Documentos, &ruta("documentos.pdf")).unwrap();
    let doc = abrir_pdf(&ruta("documentos.pdf"), None).unwrap();
    assert_eq!(doc.page_count().unwrap(), 1);
    assert_eq!(medida(&doc, 0), (595.0, 842.0));

    assert!(pdf_de_fotos(&[], Diseno::A4, &ruta("nada.pdf")).is_err());
    assert!(pdf_de_fotos(&[b"no es una foto".to_vec()], Diseno::A4, &ruta("mala.pdf")).is_err());
    std::fs::remove_dir_all(&carpeta).unwrap();
}

fn pdf_vacio() -> PdfDocument {
    let mut doc = PdfDocument::new();
    doc.new_page((595.0, 842.0)).unwrap();
    doc
}

#[test]
fn documentos_van_a_tamano_real_dos_por_fila() {
    use super::fotos::{acomodar, Puesta};
    let tarjeta = (1000.0, 631.0); // proporción de una cédula
    let hojas = acomodar(&[tarjeta; 4], Diseno::Documentos);
    assert_eq!(hojas.len(), 1);
    let p = &hojas[0].1;
    assert_eq!(p.iter().map(|x| x.foto).collect::<Vec<_>>(), [0, 1, 2, 3]);
    // 85,6 mm de ancho: 242,6 puntos.
    assert!((p[0].caja.2 - 242.6).abs() < 0.5);
    // Misma fila arriba; la segunda fila más abajo; dos columnas.
    assert_eq!(p[0].caja.1, p[1].caja.1);
    assert!(p[2].caja.1 < p[0].caja.1);
    assert!(p[1].caja.0 > p[0].caja.0 + p[0].caja.2);
    let dentro = |q: &Puesta| q.caja.0 >= 0.0 && q.caja.1 >= 0.0 && q.caja.0 + q.caja.2 <= 595.0 && q.caja.1 + q.caja.3 <= 842.0;
    assert!(p.iter().all(dentro));
    // Muchos documentos: siguen en otra hoja, sin salirse.
    let hojas = acomodar(&[tarjeta; 14], Diseno::Documentos);
    assert!(hojas.len() >= 2);
    assert_eq!(hojas.iter().map(|h| h.1.len()).sum::<usize>(), 14);
    assert!(hojas.iter().all(|h| h.1.iter().all(dentro)));
    // Uno fotografiado de pie no se sale de su fila.
    let hojas = acomodar(&[(631.0, 1000.0), tarjeta], Diseno::Documentos);
    assert!(hojas[0].1[0].caja.3 <= 242.7);
}


/// Una «foto» con detalle (no se comprime casi nada), en JPEG de buena calidad.
fn foto_grande(ancho: u16, alto: u16) -> Vec<u8> {
    let mut puntos = Vec::with_capacity(ancho as usize * alto as usize * 3);
    let mut semilla = 7u32;
    for y in 0..alto as u32 {
        for x in 0..ancho as u32 {
            semilla = semilla.wrapping_mul(1103515245).wrapping_add(12345);
            let ruido = (semilla >> 24) as u8 / 4;
            puntos.extend_from_slice(&[(x / 8) as u8 ^ ruido, (y / 8) as u8, ((x + y) / 16) as u8 ^ ruido]);
        }
    }
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, 95).encode(&puntos, ancho, alto, jpeg_encoder::ColorType::Rgb).unwrap();
    jpg
}

#[test]
fn comprimir_achica_las_imagenes_grandes_y_conserva_la_contrasena() {
    use super::comprimir::{comprimir_imagenes, comprimir_y_guardar, Nivel};
    let carpeta = std::env::temp_dir().join(format!("dakarta-comprimir-{}", std::process::id()));
    std::fs::create_dir_all(&carpeta).unwrap();
    let ruta = |n: &str| carpeta.join(n).to_string_lossy().into_owned();
    let peso = |r: &str| std::fs::metadata(r).unwrap().len();
    // Un PDF de dos fotos grandes, con contraseña.
    pdf_de_fotos(&[foto_grande(3000, 2000), foto_grande(2000, 3000)], Diseno::A4, &ruta("fotos.pdf")).unwrap();
    let original = abrir_pdf(&ruta("fotos.pdf"), None).unwrap();
    guardar_doc_con(&original, &ruta("clave.pdf"), &Proteccion::de("abc").unwrap(), None).unwrap();
    let doc = abrir_pdf(&ruta("clave.pdf"), Some("abc")).unwrap();
    comprimir_y_guardar(&doc, &ruta("ligera.pdf"), Some("abc"), Nivel::Ligera).unwrap();
    comprimir_y_guardar(&doc, &ruta("fuerte.pdf"), Some("abc"), Nivel::Fuerte).unwrap();
    let (antes, ligera, fuerte) = (peso(&ruta("clave.pdf")), peso(&ruta("ligera.pdf")), peso(&ruta("fuerte.pdf")));
    assert!(ligera < antes * 3 / 4, "ligera: {antes} -> {ligera}");
    assert!(fuerte < ligera, "fuerte: {ligera} -> {fuerte}");
    // Sigue con contraseña, con sus dos hojas, y las fotos se ven.
    assert_eq!(abrir_pdf(&ruta("fuerte.pdf"), None).err().unwrap(), PIDE_CLAVE);
    let comprimido = abrir_pdf(&ruta("fuerte.pdf"), Some("abc")).unwrap();
    assert_eq!(comprimido.page_count().unwrap(), 2);
    assert!(pintar(&comprimido, 0, 200).unwrap().len() > pintar(&pdf_vacio(), 0, 200).unwrap().len() * 2);
    // El PDF abierto no cambió.
    assert_eq!(peso(&ruta("clave.pdf")), antes);

    // Una imagen sin pérdida (como un PNG o un escaneo) grande también se achica.
    let mut px = Pixmap::new_with_w_h(&Colorspace::device_rgb(), 2600, 1800, false).unwrap();
    let mut semilla = 1u32;
    for (i, v) in px.samples_mut().iter_mut().enumerate() {
        semilla = semilla.wrapping_mul(1103515245).wrapping_add(12345);
        *v = ((i / 3 % 2600) as u32 / 12 + (semilla >> 26)) as u8;
    }
    let mut sin_perdida = PdfDocument::new();
    let img = mupdf::Image::from_pixmap(&px).unwrap();
    let ancho_antes = img.width();
    pdf_con_imagen(&mut sin_perdida, &img);
    guardar_doc(&sin_perdida, &ruta("png.pdf")).unwrap();
    let doc = abrir_pdf(&ruta("png.pdf"), None).unwrap();
    comprimir_y_guardar(&doc, &ruta("png-fuerte.pdf"), None, Nivel::Fuerte).unwrap();
    assert!(peso(&ruta("png-fuerte.pdf")) < peso(&ruta("png.pdf")) / 2, "{} -> {}", peso(&ruta("png.pdf")), peso(&ruta("png-fuerte.pdf")));
    assert_eq!(ancho_antes, 2600);
    let mut chica = abrir_pdf(&ruta("fuerte.pdf"), Some("abc")).unwrap();
    assert_eq!(comprimir_imagenes(&mut chica, Nivel::Fuerte).unwrap(), 0, "ya están chicas: no se vuelven a comprimir");
    std::fs::remove_dir_all(&carpeta).unwrap();
}

#[test]
fn achicar_promedia_sin_deformar() {
    use super::comprimir::achicar;
    // 4×2 en gris → lado máximo 2: queda 2×1 con el promedio de cada bloque.
    let (w, h, p) = achicar(&[0, 100, 200, 200, 0, 100, 200, 200], 4, 2, 1, 2);
    assert_eq!((w, h), (2, 1));
    assert_eq!(p, [50, 200]);
    // Si ya es chica, queda igual.
    assert_eq!(achicar(&[1, 2, 3], 1, 1, 3, 10), (1, 1, vec![1, 2, 3]));
}

/// Una hoja con una sola imagen.
fn pdf_con_imagen(doc: &mut PdfDocument, img: &mupdf::Image) {
    let foto = doc.add_image(img).unwrap();
    let mut objetos = doc.new_dict().unwrap();
    objetos.dict_put("Im", foto).unwrap();
    let mut recursos = doc.new_dict().unwrap();
    recursos.dict_put("XObject", objetos).unwrap();
    let dibujo = mupdf::Buffer::from_bytes(b"q 595 0 0 842 0 0 cm /Im Do Q").unwrap();
    let contenido = doc.add_stream(&dibujo, None, false).unwrap();
    let mut hoja = doc.new_dict().unwrap();
    hoja.dict_put("Type", PdfObject::new_name("Page").unwrap()).unwrap();
    hoja.dict_put("MediaBox", doc.new_object_from_str("[0 0 595 842]").unwrap()).unwrap();
    hoja.dict_put("Resources", recursos).unwrap();
    hoja.dict_put("Contents", contenido).unwrap();
    let hoja = doc.add_object(&hoja).unwrap();
    doc.insert_page(0, &hoja).unwrap();
}

#[test]
fn textos_para_gemini_por_pagina() {
    let doc = pdf_con_texto("gemini", 3);
    assert_eq!(textos_de(&doc, 0, 2).unwrap(), ["gemini 1", "gemini 2", "gemini 3"]);
    assert_eq!(textos_de(&doc, 1, 1).unwrap(), ["gemini 2"]);
    assert!(textos_de(&doc, 2, 5).is_err());
    assert!(textos_de(&doc, 2, 1).is_err());
    // Una hoja sin texto (como un escaneo) queda vacía.
    assert_eq!(textos_de(&pdf_vacio(), 0, 0).unwrap(), [""]);
}

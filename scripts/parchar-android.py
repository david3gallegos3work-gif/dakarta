# Cambios al proyecto Android que genera Tauri (se aplican en GitHub Actions después de "tauri android init").
# Abrir con Dakarta: un PDF tocado en WhatsApp, Descargas, un correo… abre la app, y la pantalla lo recoge con
# DakartaAndroid.pendiente(). También permite elegir Dakarta como lector predeterminado ("Siempre").
# El botón "Abrir" usa el selector de Android (no el de Tauri): su permiso sí se puede guardar para reabrir el
# archivo otro día. El selector de Tauri pide el archivo con ACTION_GET_CONTENT, que solo da permiso temporal.
# "Atrás" (botón o gesto) cierra el panel o la búsqueda antes de salir de la app.
import glob, sys

act = glob.glob('src-tauri/gen/android/app/src/main/java/**/MainActivity.kt', recursive=True)[0]
s = open(act).read()
LLAMADA = 'super.onCreate(savedInstanceState)\n'
if LLAMADA not in s:
    sys.exit('MainActivity: no se encontró onCreate')
ATRAS = '''    onBackPressedDispatcher.addCallback(this, object : androidx.activity.OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val w = vista
        if (w == null) {
          isEnabled = false
          onBackPressedDispatcher.onBackPressed()
          isEnabled = true
          return
        }
        // La pantalla dice si cerró algo (el índice o la búsqueda) o si toca salir de la app.
        w.evaluateJavascript("window.DakartaAtras ? window.DakartaAtras() : false") { r ->
          if (r != "true") {
            isEnabled = false
            onBackPressedDispatcher.onBackPressed()
            isEnabled = true
          }
        }
      }
    })
'''
s = s.replace(LLAMADA, LLAMADA + '    recibir(intent)\n' + ATRAS, 1)
METODOS = r'''
  private val puente = PuenteDakarta(this)
  private var vista: android.webkit.WebView? = null

  override fun onNewIntent(intent: android.content.Intent) {
    super.onNewIntent(intent)
    recibir(intent)
  }

  private fun recibir(i: android.content.Intent?) {
    if (i == null || i.action != android.content.Intent.ACTION_VIEW) return
    // Desde "Recientes" Android repite el pedido viejo, cuyo permiso sobre el archivo pudo vencer.
    if ((i.flags and android.content.Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return
    puente.uri = i.dataString
    // Que el archivo siga abriéndose desde "Últimos archivos" al volver a la app (si el proveedor lo permite).
    val uri = i.data
    if (uri != null && (i.flags and android.content.Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION) != 0) {
      try {
        contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
      } catch (_: Exception) { }
    }
  }

  override fun onActivityResult(pedido: Int, resultado: Int, datos: android.content.Intent?) {
    super.onActivityResult(pedido, resultado, datos)
    if (pedido == PEDIDO_GUARDAR) {
      val destino = if (resultado == android.app.Activity.RESULT_OK) datos?.data else null
      if (destino != null) {
        try {
          contentResolver.takePersistableUriPermission(destino, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } catch (_: Exception) { }
      }
      puente.guardado = destino?.toString()
      vista?.evaluateJavascript("window.dispatchEvent(new Event('dakarta-guardar'))", null)
      return
    }
    if (pedido != PEDIDO_PDF || resultado != android.app.Activity.RESULT_OK) return
    val uri = datos?.data ?: return
    try {
      contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
    } catch (_: Exception) { }
    puente.uri = uri.toString()
    // La pantalla ya escucha "focus"; así recoge el archivo aunque Android no lo avise solo.
    vista?.evaluateJavascript("window.dispatchEvent(new Event('focus'))", null)
  }

  override fun onWebViewCreate(webView: android.webkit.WebView) {
    super.onWebViewCreate(webView)
    vista = webView
    puente.vista = webView
    webView.addJavascriptInterface(puente, "DakartaAndroid")
  }
}

class PuenteDakarta(private val actividad: android.app.Activity) {
  @Volatile var uri: String? = null
  @Volatile var guardado: String? = null
  @Volatile var vista: android.webkit.WebView? = null

  @android.webkit.JavascriptInterface
  fun pendiente(): String? {
    val u = uri
    uri = null
    return u
  }

  /** Nombre que muestra el proveedor para el archivo (WhatsApp, Gmail, Descargas…). */
  @android.webkit.JavascriptInterface
  fun nombre(uri: String): String? = try {
    actividad.contentResolver.query(
      android.net.Uri.parse(uri),
      arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null
    )?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
  } catch (_: Exception) { null }

  /** El permiso del selector de archivos se guarda para poder reabrir el archivo otro día. */
  @android.webkit.JavascriptInterface
  fun persistir(uri: String) {
    try {
      actividad.contentResolver.takePersistableUriPermission(
        android.net.Uri.parse(uri), android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION
      )
    } catch (_: Exception) { }
  }

  /** "Salir" del cuadro de cambios sin guardar: repite el "atrás"; la pantalla ya no lo detiene. */
  @android.webkit.JavascriptInterface
  fun salir() {
    actividad.runOnUiThread {
      (actividad as androidx.activity.ComponentActivity).onBackPressedDispatcher.onBackPressed()
    }
  }

  /** Menú de compartir de Android con el documento que está abierto (PDF, Word, EPUB…). */
  @android.webkit.JavascriptInterface
  fun compartir(uri: String, nombre: String) {
    actividad.runOnUiThread {
      try {
        val u = android.net.Uri.parse(uri)
        val i = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
          type = actividad.contentResolver.getType(u) ?: "application/pdf"
          putExtra(android.content.Intent.EXTRA_STREAM, u)
          clipData = android.content.ClipData.newRawUri(nombre, u)
          addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        actividad.startActivity(android.content.Intent.createChooser(i, "Compartir"))
      } catch (_: Exception) { }
    }
  }

  /** "Guardar copia": el sistema pregunta dónde; el destino vuelve por onActivityResult. */
  @android.webkit.JavascriptInterface
  fun guardar(nombre: String) {
    actividad.runOnUiThread {
      try {
        val i = android.content.Intent(android.content.Intent.ACTION_CREATE_DOCUMENT).apply {
          addCategory(android.content.Intent.CATEGORY_OPENABLE)
          type = "application/pdf"
          putExtra(android.content.Intent.EXTRA_TITLE, nombre)
          addFlags(
            android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or
              android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION
          )
        }
        actividad.startActivityForResult(i, PEDIDO_GUARDAR)
      } catch (_: Exception) {
        // Sin selector de destino: la pantalla recibe "cancelado" y no se guarda nada.
        vista?.evaluateJavascript("window.dispatchEvent(new Event('dakarta-guardar'))", null)
      }
    }
  }

  /** El destino elegido se entrega una sola vez. */
  @android.webkit.JavascriptInterface
  fun destino(): String? {
    val d = guardado
    guardado = null
    return d
  }

  /** Copia la copia editada de la caché al destino elegido por el usuario. La de la caché se borra
   *  siempre (puede pesar tanto como el PDF); si la copia falla, se borra el archivo a medias del destino. */
  @android.webkit.JavascriptInterface
  fun copiar(destino: String, origen: String): Boolean {
    val archivo = java.io.File(origen)
    val listo = try {
      actividad.contentResolver.openOutputStream(android.net.Uri.parse(destino), "wt")?.use { salida ->
        archivo.inputStream().use { entrada -> entrada.copyTo(salida) }
        true
      } ?: false
    } catch (_: Exception) { false }
    archivo.delete()
    if (!listo) borrar(destino)
    return listo
  }

  /** Quita el archivo que se creó al elegir el destino, cuando al final no se pudo guardar nada en él. */
  @android.webkit.JavascriptInterface
  fun borrar(destino: String) {
    try {
      android.provider.DocumentsContract.deleteDocument(actividad.contentResolver, android.net.Uri.parse(destino))
    } catch (_: Exception) { }
  }

  /** Selector del sistema con permiso guardable; el archivo vuelve por onActivityResult. */
  @android.webkit.JavascriptInterface
  fun elegir() {
    actividad.runOnUiThread {
      val i = android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(android.content.Intent.CATEGORY_OPENABLE)
        // PDF para todo; los demás formatos (Word, EPUB, TXT, imágenes…) para leer.
        type = "*/*"
        putExtra(android.content.Intent.EXTRA_MIME_TYPES, TIPOS_ABRIR + arrayOf("image/*"))
        addFlags(
          android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or
            android.content.Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
        )
      }
      actividad.startActivityForResult(i, PEDIDO_PDF)
    }
  }
}

private const val PEDIDO_PDF = 0x0DA1
private val TIPOS_ABRIR = arrayOf(
  "application/pdf",
  "application/epub+zip",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  "application/x-mobipocket-ebook",
  "application/x-fictionbook+xml",
  "application/vnd.ms-xpsdocument",
  "application/oxps",
  "application/vnd.comicbook+zip",
  "application/x-cbz",
  "text/plain",
  // Muchas apps (Gemini, WhatsApp…) entregan un Word con estas etiquetas: Dakarta mira el contenido real.
  "application/msword",
  "application/octet-stream"
)
private const val PEDIDO_GUARDAR = 0x0DA2
'''
fin = s.rstrip().rfind('}')
s = s[:fin].rstrip() + '\n' + METODOS
open(act, 'w').write(s)

man = 'src-tauri/gen/android/app/src/main/AndroidManifest.xml'
m = open(man).read()
FILTRO = '''</intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="content" />
                <data android:mimeType="application/pdf" />
                <data android:mimeType="application/epub+zip" />
                <data android:mimeType="application/vnd.openxmlformats-officedocument.wordprocessingml.document" />
                <data android:mimeType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" />
                <data android:mimeType="application/vnd.openxmlformats-officedocument.presentationml.presentation" />
                <data android:mimeType="application/x-mobipocket-ebook" />
                <data android:mimeType="application/x-fictionbook+xml" />
                <data android:mimeType="application/vnd.ms-xpsdocument" />
                <data android:mimeType="application/oxps" />
                <data android:mimeType="application/vnd.comicbook+zip" />
                <data android:mimeType="application/x-cbz" />
                <data android:mimeType="text/plain" />
                <data android:mimeType="application/msword" />
                <data android:mimeType="application/octet-stream" />
            </intent-filter>'''
if '</intent-filter>' not in m:
    sys.exit('Manifest: no se encontró intent-filter')
open(man, 'w').write(m.replace('</intent-filter>', FILTRO, 1))
print('Android listo: Abrir con Dakarta (PDF y otros formatos para leer)')

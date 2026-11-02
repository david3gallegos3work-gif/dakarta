# Cambios al proyecto Android que genera Tauri (se aplican en GitHub Actions después de "tauri android init").
# Abrir con Dakarta: un PDF tocado en WhatsApp, Descargas, un correo… abre la app, y la pantalla lo recoge con
# DakartaAndroid.pendiente(). También permite elegir Dakarta como lector predeterminado ("Siempre").
import glob, sys

act = glob.glob('src-tauri/gen/android/app/src/main/java/**/MainActivity.kt', recursive=True)[0]
s = open(act).read()
LLAMADA = 'super.onCreate(savedInstanceState)\n'
if LLAMADA not in s:
    sys.exit('MainActivity: no se encontró onCreate')
s = s.replace(LLAMADA, LLAMADA + '    recibir(intent)\n', 1)
METODOS = r'''
  private val puente = PuenteDakarta()

  override fun onNewIntent(intent: android.content.Intent) {
    super.onNewIntent(intent)
    recibir(intent)
  }

  private fun recibir(i: android.content.Intent?) {
    if (i == null || i.action != android.content.Intent.ACTION_VIEW) return
    // Desde "Recientes" Android repite el pedido viejo, cuyo permiso sobre el archivo pudo vencer.
    if ((i.flags and android.content.Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0) return
    puente.uri = i.dataString
  }

  override fun onWebViewCreate(webView: android.webkit.WebView) {
    super.onWebViewCreate(webView)
    webView.addJavascriptInterface(puente, "DakartaAndroid")
  }
}

class PuenteDakarta {
  @Volatile var uri: String? = null

  @android.webkit.JavascriptInterface
  fun pendiente(): String? {
    val u = uri
    uri = null
    return u
  }
}
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
            </intent-filter>'''
if '</intent-filter>' not in m:
    sys.exit('Manifest: no se encontró intent-filter')
open(man, 'w').write(m.replace('</intent-filter>', FILTRO, 1))
print('Android listo: Abrir con Dakarta')

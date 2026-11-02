import { getVersion } from '@tauri-apps/api/app'

const ULTIMA = 'https://api.github.com/repos/david3gallegos3work-gif/dakarta/releases/latest'

export type Actualizacion = { version: string; url: string }

/** ¿a es más nueva que b? Número por número: 0.10.0 es más nueva que 0.9.0. */
export function esMasNueva(a: string, b: string) {
  const x = a.split('.').map(Number)
  const y = b.split('.').map(Number)
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] || 0) - (y[i] || 0)
    if (d) return d > 0
  }
  return false
}

export async function buscarActualizacion(): Promise<Actualizacion | null> {
  try {
    const r = await fetch(ULTIMA, { headers: { Accept: 'application/vnd.github+json' } })
    if (!r.ok) return null
    const ultima: { tag_name: string; assets: { name: string; browser_download_url: string }[] } = await r.json()
    const version = ultima.tag_name.replace(/^v/, '')
    // Con su número en el nombre: el navegador no lo confunde con la versión ya descargada.
    const nombre = /android/i.test(navigator.userAgent) ? `Dakarta-${version}.apk` : `Dakarta-Windows-${version}.exe`
    // Mientras se publica, una versión puede tener aún solo uno de los dos archivos: se avisa cuando esté el de este aparato.
    const archivo = ultima.assets.find((a) => a.name === nombre)
    return archivo && esMasNueva(version, await getVersion()) ? { version, url: archivo.browser_download_url } : null
  } catch {
    return null // sin internet: se vuelve a mirar la próxima vez que se abra la app
  }
}

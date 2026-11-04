import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { esMasNueva, ultimaVersion } from './version.ts'

const DESCARGAS = 'https://github.com/david3gallegos3work-gif/dakarta/releases'
// Enlaces fijos: siempre bajan la última versión publicada.
const APPS = [
  { nombre: 'Android', tipo: '.apk', url: `${DESCARGAS}/latest/download/Dakarta.apk` },
  { nombre: 'Windows', tipo: '.exe', url: `${DESCARGAS}/latest/download/Dakarta-Windows.exe` },
]

type Estado = 'buscando' | 'sin-internet' | 'al-dia' | { version: string; url: string }

/** Versión instalada, si hay una nueva y los enlaces para instalar Dakarta en otro aparato. */
export function AcercaDe({ onAviso }: { onAviso: (texto: string, error?: boolean) => void }) {
  const [instalada, setInstalada] = useState('')
  const [estado, setEstado] = useState<Estado>('buscando')
  const android = /android/i.test(navigator.userAgent)

  useEffect(() => {
    let vivo = true
    void (async () => {
      const actual = await getVersion().catch(() => '')
      const ultima = await ultimaVersion()
      if (!vivo) return
      setInstalada(actual)
      if (!ultima || !actual) setEstado('sin-internet')
      else if (ultima.url && esMasNueva(ultima.version, actual)) setEstado({ version: ultima.version, url: ultima.url })
      else setEstado('al-dia')
    })()
    return () => {
      vivo = false
    }
  }, [])

  const abrir = (url: string) => invoke('descargar_version', { url }).catch((e) => onAviso(String(e), true))
  const copiar = (url: string) =>
    navigator.clipboard
      .writeText(url)
      .then(() => onAviso('Enlace copiado.'))
      .catch(() => onAviso('No se pudo copiar el enlace.', true))

  return (
    <div className="acerca">
      <p className="acerca-version">
        Versión instalada <b>{instalada || '…'}</b> · App de {android ? 'Android' : 'Windows'}
      </p>
      {estado === 'buscando' && <p className="acerca-estado">Buscando versiones nuevas…</p>}
      {estado === 'sin-internet' && <p className="acerca-estado">No se pudo revisar si hay versión nueva (sin internet).</p>}
      {estado === 'al-dia' && <p className="acerca-estado al-dia">✓ Es la más reciente</p>}
      {typeof estado === 'object' && (
        <div className="acerca-nueva">
          <span>Hay una versión nueva ({estado.version}).</span>
          <button className="grande" onClick={() => abrir(estado.url)}>
            Descargar
          </button>
        </div>
      )}
      <h3>Apps para instalar</h3>
      <ul className="acerca-apps">
        {APPS.map((a) => (
          <li key={a.nombre}>
            <span>
              <b>{a.nombre}</b> ({a.tipo})
            </span>
            <button onClick={() => copiar(a.url)} aria-label={`Copiar enlace de ${a.nombre}`} title="Copiar enlace">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <rect x="8" y="8" width="12" height="12" rx="2" fill="none" stroke="currentColor" strokeWidth="2" />
                <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" fill="none" stroke="currentColor" strokeWidth="2" />
              </svg>
            </button>
            <button onClick={() => abrir(a.url)} aria-label={`Descargar ${a.nombre}`} title="Descargar">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 19h14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          </li>
        ))}
      </ul>
      <button className="acerca-enlace" onClick={() => abrir(`${DESCARGAS}/latest`)}>
        Página de descargas ↗
      </button>
    </div>
  )
}

import { getVersion } from '@tauri-apps/api/app'
import { useEffect, useState } from 'react'
import { Indice } from './Paneles.tsx'
import { nombreLegible, type Reciente } from './recientes.ts'
import type { Entrada } from './tipos.ts'
import type { Actualizacion } from './version.ts'

/** Íconos de trazo (como los de la barra): 24×24, del color del texto. */
const TRAZOS = {
  abrir: 'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
  pdf: 'M6 2h9l5 5v15H6z M14 2v6h6',
  indice: 'M8 6h13M8 12h13M8 18h13M3.5 6h.5M3.5 12h.5M3.5 18h.5',
  organizar: 'M4.5 3h4a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-4A1.5 1.5 0 0 1 3 10.5v-6A1.5 1.5 0 0 1 4.5 3z M15.5 3h4A1.5 1.5 0 0 1 21 4.5v2A1.5 1.5 0 0 1 19.5 8h-4A1.5 1.5 0 0 1 14 6.5v-2A1.5 1.5 0 0 1 15.5 3z M15.5 12h4a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-6a1.5 1.5 0 0 1 1.5-1.5z M4.5 16h4a1.5 1.5 0 0 1 1.5 1.5v2A1.5 1.5 0 0 1 8.5 21h-4A1.5 1.5 0 0 1 3 19.5v-2A1.5 1.5 0 0 1 4.5 16z',
  unir: 'M8 3v6a4 4 0 0 0 4 4 4 4 0 0 1 4 4v4 M16 3v6a4 4 0 0 1-4 4 M5 18l3 3 3-3',
  comprimir: 'M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7',
  clave: 'M6 10h12a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z M8 10V7a4 4 0 0 1 8 0v3',
  sinClave: 'M6 10h12a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z M8 10V7a4 4 0 0 1 7.5-2',
  escaner: 'M4 8V6a2 2 0 0 1 2-2h2 M16 4h2a2 2 0 0 1 2 2v2 M20 16v2a2 2 0 0 1-2 2h-2 M8 20H6a2 2 0 0 1-2-2v-2 M4 12h16',
  gemini: 'M12 2c.6 5.4 4.6 9.4 10 10-5.4.6-9.4 4.6-10 10-.6-5.4-4.6-9.4-10-10C7.4 11.4 11.4 7.4 12 2z',
  x: 'M6 6l12 12M18 6L6 18',
  seguir: 'M9 6l6 6-6 6',
  volver: 'M15 6l-6 6 6 6',
}
function Icono({ n, tam = 22 }: { n: keyof typeof TRAZOS; tam?: number }) {
  return (
    <svg width={tam} height={tam} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={TRAZOS[n]} />
    </svg>
  )
}

export type Herramienta = 'organizar' | 'unir' | 'comprimir' | 'clave' | 'quitarClave' | 'escaner' | 'gemini' | 'version'

/**
 * Menú lateral (☰): el PDF abierto, los últimos archivos (el abierto, marcado), Gemini, las herramientas en
 * cuadrícula y la versión de Dakarta. «Índice» muestra el índice del PDF en el mismo panel.
 */
export function MenuLateral({
  nombre,
  paginas,
  conClave,
  indice,
  recientes,
  rutaActual,
  abriendo,
  nueva,
  onAbrir,
  onReciente,
  onQuitarReciente,
  onIr,
  onHerramienta,
}: {
  nombre: string
  paginas: number
  conClave: boolean
  indice: Entrada[]
  recientes: Reciente[]
  rutaActual: string
  abriendo: boolean
  nueva: Actualizacion | null
  onAbrir: () => void
  onReciente: (r: Reciente) => void
  onQuitarReciente: (r: Reciente) => void
  onIr: (n: number) => void
  onHerramienta: (h: Herramienta) => void
}) {
  const [verIndice, setVerIndice] = useState(false)
  const [version, setVersion] = useState('')
  useEffect(() => {
    getVersion().then(setVersion, () => {})
  }, [])

  if (verIndice)
    return (
      <div className="menu-indice">
        <button className="menu-volver" onClick={() => setVerIndice(false)}>
          <Icono n="volver" tam={20} /> Volver al menú
        </button>
        <h3>Índice</h3>
        {indice.length ? (
          <Indice entradas={indice} ir={onIr} />
        ) : (
          <p className="menu-nota">
            Este PDF no trae índice lateral. Si tiene una página de índice al inicio, toca sus líneas: si traen enlace, te llevan a esa página.
          </p>
        )}
      </div>
    )

  const herramientas: { h: Herramienta | 'indice'; n: keyof typeof TRAZOS; texto: string; color: string }[] = [
    { h: 'indice', n: 'indice', texto: 'Índice', color: 'azul' },
    { h: 'organizar', n: 'organizar', texto: 'Organizar', color: 'verde' },
    { h: 'unir', n: 'unir', texto: 'Unir PDFs', color: 'naranja' },
    { h: 'comprimir', n: 'comprimir', texto: 'Comprimir', color: 'morado' },
    { h: 'clave', n: 'clave', texto: conClave ? 'Cambiar contraseña' : 'Contraseña', color: 'rojo' },
    { h: 'escaner', n: 'escaner', texto: 'Escáner', color: 'celeste' },
    ...(conClave ? [{ h: 'quitarClave' as const, n: 'sinClave' as const, texto: 'Quitar contraseña', color: 'gris' }] : []),
  ]

  return (
    <div className="menu-lateral">
      <div className="menu-abierto">
        <span className="menu-icono-pdf">
          <Icono n="pdf" tam={26} />
        </span>
        <div>
          <strong>{nombreLegible(nombre)}</strong>
          <span>
            Abierto · {paginas} {paginas === 1 ? 'página' : 'páginas'}
            {conClave ? ' · con contraseña' : ''}
          </span>
        </div>
      </div>
      <button className="menu-abrir" onClick={onAbrir} disabled={abriendo}>
        <Icono n="abrir" tam={20} /> {abriendo ? 'Abriendo…' : 'Abrir otro PDF'}
      </button>

      {recientes.length > 0 && (
        <>
          <h3>Últimos archivos</h3>
          <ul className="menu-recientes">
            {recientes.slice(0, 5).map((r) => {
              const abierto = r.ruta === rutaActual
              return (
                <li key={r.ruta} className={abierto ? 'abierto' : undefined}>
                  <button className="menu-reciente" onClick={() => onReciente(r)} disabled={abriendo || abierto} title={r.nombre}>
                    <Icono n="pdf" tam={18} />
                    <span>{nombreLegible(r.nombre)}</span>
                    {abierto && <em>ABIERTO</em>}
                  </button>
                  <button
                    className="menu-quitar"
                    onClick={() => onQuitarReciente(r)}
                    aria-label={`Quitar «${r.nombre}» de la lista`}
                    title="Quitar de la lista (no borra el archivo)"
                  >
                    <Icono n="x" tam={16} />
                  </button>
                </li>
              )
            })}
          </ul>
        </>
      )}

      <button className="menu-gemini" onClick={() => onHerramienta('gemini')}>
        <Icono n="gemini" tam={30} />
        <span>
          <strong>Gemini</strong>
          Resumir y preguntarle al PDF
        </span>
      </button>

      <h3>Herramientas</h3>
      <div className="menu-cuadricula">
        {herramientas.map((x) => (
          <button key={x.h} className="menu-herramienta" onClick={() => (x.h === 'indice' ? setVerIndice(true) : onHerramienta(x.h))}>
            <span className={`menu-ficha ${x.color}`}>
              <Icono n={x.n} tam={24} />
            </span>
            {x.texto}
          </button>
        ))}
      </div>

      <button className="menu-version" onClick={() => onHerramienta('version')}>
        <span className="menu-logo">D</span>
        <span>
          Dakarta · versión y descargas
          <small className={nueva ? 'hay-nueva' : undefined}>
            {nueva ? `Hay una versión nueva: ${nueva.version}` : version ? `Versión ${version}` : 'Ver versión'}
          </small>
        </span>
        <Icono n="seguir" tam={18} />
      </button>
    </div>
  )
}

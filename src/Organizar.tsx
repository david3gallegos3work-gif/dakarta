import { convertFileSrc } from '@tauri-apps/api/core'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { Diseno } from './fotos.ts'
import { ABIERTO, fichasDe, moverMarcadas, ordenarPorArchivo, type Ficha, type Fuente } from './organizar.ts'
import type { Hoja } from './tipos.ts'
import { MINIATURA } from './zoom.ts'

/** Mantener presionada una ficha este tiempo la levanta para arrastrarla. */
const ESPERA_ARRASTRE = 350

/** La ficha mide 3 de ancho por 4 de alto. Una hoja girada de lado se achica para entrar completa. */
function giroQueEntra(giro: number, proporcion = 4 / 3) {
  if (giro % 180 === 0) return `rotate(${giro}deg)`
  const ancho = 1
  const alto = proporcion
  const normal = Math.min(3 / ancho, 4 / alto)
  const girada = Math.min(3 / alto, 4 / ancho)
  return `rotate(${giro}deg) scale(${girada / normal})`
}

type Arrastre = { id: number; x: number; y: number; destino: number | null }

/** Al unir, cada PDF lleva su color en el borde de sus hojas. */
const COLORES_ARCHIVO = ['#1565c0', '#e65100', '#2e7d32', '#6a1b9a', '#c62828', '#00838f', '#ad1457', '#5d4037']

/** Organizar las hojas del PDF abierto, unir varios PDFs o crear un PDF desde fotos. */
export type Modo = 'organizar' | 'unir' | 'fotos'

const android = typeof navigator !== 'undefined' && /android/i.test(navigator.userAgent)

/**
 * Pantalla con todas las hojas en miniatura: mover, girar, quitar, duplicar o extraer, con deshacer.
 * Al unir, además se agregan otros PDFs y sus hojas se intercalan; con fotos, cada foto es una hoja. Nada
 * se cambia en ningún PDF hasta guardar; ahí se arma un PDF nuevo con este orden.
 */
export function Organizar({
  fuentes: fuentesIniciales,
  modo,
  onAgregar,
  onFotos,
  onRecortar,
  miniaturas,
  guardando,
  onGuardar,
  onCerrar,
  aviso,
  error,
  onAviso,
}: {
  fuentes: Fuente[]
  modo: Modo
  /** Pide uno o más PDFs para unir y devuelve los que se agregaron (vacío si se canceló). */
  onAgregar: () => Promise<Fuente[]>
  /** Fotos: prepara las elegidas (o la tomada con la cámara) y devuelve una fuente por foto. */
  onFotos?: (archivos: File[]) => Promise<Fuente[]>
  /** Fotos: recortar la foto de ese número de archivo. */
  onRecortar?: (archivo: number) => void
  /** Fotos: la miniatura de cada una, por su número de archivo. */
  miniaturas?: Record<number, string>
  guardando: boolean
  /** `diseno`: solo con fotos; cómo van las fotos en las hojas. */
  onGuardar: (hojas: Hoja[], extraer: boolean, diseno: Diseno) => void
  onCerrar: () => void
  /** Avisos de la app (copia guardada, errores): esta pantalla tapa los de siempre, así que los muestra ella. */
  aviso: string
  error: string
  onAviso: (texto: string) => void
}) {
  const unir = modo === 'unir'
  const fotos = modo === 'fotos'
  const [diseno, setDiseno] = useState<Diseno>('a4')
  const [fuentes, setFuentes] = useState(fuentesIniciales)
  const siguienteId = useRef(0)
  const [lista, setLista] = useState(() =>
    fuentesIniciales.flatMap((f) => {
      const fichas = fichasDe(f, siguienteId.current)
      siguienteId.current += fichas.length
      return fichas
    }),
  )
  const [historial, setHistorial] = useState<Ficha[][]>([])
  const [marcadas, setMarcadas] = useState<Set<number>>(new Set())
  const [arrastre, setArrastre] = useState<Arrastre | null>(null)
  // Alto entre ancho de cada página («archivo:página»), para achicar bien las giradas.
  const [proporciones, setProporciones] = useState<Record<string, number>>({})
  const [agregando, setAgregando] = useState(false)
  const reloj = useRef<ReturnType<typeof setTimeout>>(undefined)
  const inicio = useRef<{ id: number; x: number; y: number } | null>(null)
  const arrastrando = useRef(false)
  const rejilla = useRef<HTMLDivElement>(null)
  // Las miniaturas se piden de nuevo cada vez que se abre: así muestran las anotaciones recién hechas.
  const apertura = useMemo(() => Date.now(), [])

  const cambiar = (nueva: Ficha[]) => {
    setHistorial((h) => [...h, lista])
    setLista(nueva)
  }
  const deshacer = () => {
    const anterior = historial.at(-1)
    if (!anterior) return
    setHistorial((h) => h.slice(0, -1))
    setLista(anterior)
    setMarcadas((m) => new Set([...m].filter((id) => anterior.some((f) => f.id === id))))
  }

  const marcadasEnOrden = lista.filter((f) => marcadas.has(f.id))
  const alternar = (id: number) =>
    setMarcadas((m) => {
      const n = new Set(m)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  const mover = (paso: -1 | 1) => cambiar(moverMarcadas(lista, marcadas, paso))
  const girar = () => cambiar(lista.map((f) => (marcadas.has(f.id) ? { ...f, giro: (f.giro + 90) % 360 } : f)))
  const duplicar = () => {
    const nueva: Ficha[] = []
    const copias = new Set<number>()
    for (const f of lista) {
      nueva.push(f)
      if (marcadas.has(f.id)) {
        const id = siguienteId.current++
        nueva.push({ ...f, id })
        copias.add(id)
      }
    }
    cambiar(nueva)
    setMarcadas(copias)
  }
  const quitar = () => {
    if (!fotos && marcadasEnOrden.length === lista.length) {
      onAviso('Debe quedar al menos una página.')
      return
    }
    cambiar(lista.filter((f) => !marcadas.has(f.id)))
    setMarcadas(new Set())
  }
  const hojas = (fichas: Ficha[]): Hoja[] => fichas.map((f) => ({ pagina: f.pagina, giro: f.giro, archivo: f.archivo }))
  const agregar = async (pedir: () => Promise<Fuente[]> = onAgregar) => {
    setAgregando(true)
    try {
      const nuevas = await pedir()
      if (!nuevas.length) return
      const fichas = nuevas.flatMap((f) => {
        const lista = fichasDe(f, siguienteId.current)
        siguienteId.current += lista.length
        return lista
      })
      setFuentes((x) => [...x, ...nuevas])
      cambiar([...lista, ...fichas])
    } finally {
      setAgregando(false)
    }
  }
  const color = (f: Ficha) => COLORES_ARCHIVO[Math.max(0, fuentes.findIndex((x) => x.archivo === f.archivo)) % COLORES_ARCHIVO.length]
  const clave = (f: Ficha) => `${f.archivo}:${f.pagina}`

  // Mientras se arrastra una ficha, el dedo no debe desplazar la lista.
  useEffect(() => {
    const el = rejilla.current
    if (!el) return
    const frenar = (e: TouchEvent) => {
      if (arrastrando.current) e.preventDefault()
    }
    el.addEventListener('touchmove', frenar, { passive: false })
    return () => el.removeEventListener('touchmove', frenar)
  }, [])
  useEffect(() => () => clearTimeout(reloj.current), [])

  const fichaBajo = (x: number, y: number) => {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-i]')
    return el ? Number(el.dataset.i) : null
  }
  const soltar = () => {
    clearTimeout(reloj.current)
    inicio.current = null
    const a = arrastre
    arrastrando.current = false
    setArrastre(null)
    if (!a || a.destino === null) return
    const desde = lista.findIndex((f) => f.id === a.id)
    if (desde < 0 || desde === a.destino) return
    const nueva = lista.slice()
    const [f] = nueva.splice(desde, 1)
    nueva.splice(a.destino, 0, f)
    cambiar(nueva)
  }

  const fichaArrastrada = arrastre && lista.find((f) => f.id === arrastre.id)
  const url = (f: Ficha) =>
    fotos
      ? (miniaturas?.[f.archivo] ?? '')
      : `${convertFileSrc(String(f.pagina), 'pagina')}?w=${MINIATURA}${f.archivo === ABIERTO ? '' : `&a=${f.archivo}`}&o=${apertura}`
  const titulo = unir ? 'Unir PDFs' : fotos ? 'Crear PDF desde fotos' : 'Organizar páginas'
  /** Botón que abre la galería (varias fotos) o la cámara (una). */
  const elegirFotos = (camara: boolean) => (
    <label className={agregando ? 'boton-archivo desactivado' : 'boton-archivo'}>
      {camara ? 'Cámara' : agregando ? 'Preparando…' : '+ Fotos'}
      <input
        type="file"
        accept="image/*"
        multiple={!camara}
        capture={camara ? 'environment' : undefined}
        disabled={agregando}
        hidden
        onChange={(e) => {
          const archivos = [...(e.currentTarget.files ?? [])]
          e.currentTarget.value = ''
          if (archivos.length && onFotos) void agregar(() => onFotos(archivos))
        }}
      />
    </label>
  )

  return (
    <div className="organizar" role="dialog" aria-modal="true" aria-label={titulo}>
      <header className="barra organizar-barra">
        <button onClick={onCerrar} aria-label="Volver" title="Volver">
          ←
        </button>
        <strong>{titulo}</strong>
        <span className="organizar-cuenta">{lista.length} hojas</span>
        <button className="derecha" onClick={deshacer} disabled={!historial.length} aria-label="Deshacer" title="Deshacer">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M9 14 4 9l5-5" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
            <path d="M4 9h9.5a5.5 5.5 0 0 1 0 11H11" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" />
          </svg>
        </button>
      </header>
      {error ? (
        <button className="organizar-ayuda organizar-error" onClick={() => onAviso('')}>
          {error} (tocar para cerrar)
        </button>
      ) : (
        <p className={aviso ? 'organizar-ayuda organizar-ok' : 'organizar-ayuda'} role="status">
          {aviso ||
            (fotos && diseno === 'documentos' && !marcadas.size
              ? 'Van dos por fila a tamaño real: cédula (frente y reverso), luego papeleta (frente y reverso). Marca cada foto y toca «Recortar» para dejar solo el documento.'
              : fotos && !lista.length
              ? `Toca «+ Fotos» para elegirlas${android ? ' o «Cámara» para tomar una' : ''}. Cada foto será una hoja.`
              : unir && !lista.length
              ? 'Toca «Agregar PDF» para elegir los PDFs que quieres unir.'
              : 'Toca las hojas para marcarlas. Mantén presionada una y arrástrala para moverla.')}
        </p>
      )}
      {fotos && (
        <label className="organizar-diseno">
          Diseño:
          <select value={diseno} onChange={(e) => setDiseno(e.target.value as Diseno)}>
            <option value="a4">Una foto por hoja A4</option>
            <option value="foto">Una foto por hoja, tamaño de la foto</option>
            <option value="documentos">Cédula y papeleta (tamaño real)</option>
          </select>
        </label>
      )}
      {unir && fuentes.length > 0 && (
        <ul className="organizar-archivos">
          {fuentes.map((f, i) => (
            <li key={f.archivo}>
              <span className="organizar-color" style={{ background: COLORES_ARCHIVO[i % COLORES_ARCHIVO.length] }} />
              {f.nombre}
            </li>
          ))}
        </ul>
      )}
      <div className="organizar-rejilla" ref={rejilla}>
        {lista.map((f, i) => (
          <button
            key={f.id}
            data-i={i}
            className={[
              'ficha',
              marcadas.has(f.id) ? 'marcada' : '',
              arrastre?.id === f.id ? 'levantada' : '',
              arrastre?.destino === i && arrastre.id !== f.id ? 'destino' : '',
            ].join(' ')}
            aria-pressed={marcadas.has(f.id)}
            aria-label={`Hoja ${i + 1}${fotos ? '' : `, página ${f.pagina + 1} del original`}${f.giro ? `, girada ${f.giro}°` : ''}`}
            style={unir ? { borderBottomColor: color(f) } : undefined}
            onClick={() => {
              if (!arrastrando.current) alternar(f.id)
            }}
            onContextMenu={(e) => e.preventDefault()}
            onPointerDown={(e) => {
              inicio.current = { id: f.id, x: e.clientX, y: e.clientY }
              clearTimeout(reloj.current)
              reloj.current = setTimeout(() => {
                const p = inicio.current
                if (!p) return
                arrastrando.current = true
                setArrastre({ id: p.id, x: p.x, y: p.y, destino: null })
              }, ESPERA_ARRASTRE)
            }}
            onPointerMove={(e) => {
              const p = inicio.current
              if (!p) return
              if (!arrastrando.current) {
                // Se movió antes de levantarla: es desplazar la lista, no arrastrar.
                if (Math.hypot(e.clientX - p.x, e.clientY - p.y) > 10) {
                  clearTimeout(reloj.current)
                  inicio.current = null
                }
                return
              }
              setArrastre((a) => a && { ...a, x: e.clientX, y: e.clientY, destino: fichaBajo(e.clientX, e.clientY) })
            }}
            onPointerUp={soltar}
            onPointerCancel={soltar}
          >
            <span className="ficha-hoja">
              <img
                src={url(f)}
                alt=""
                loading="lazy"
                draggable={false}
                style={{ transform: giroQueEntra(f.giro, proporciones[clave(f)]), ...(unir ? { outline: `3px solid ${color(f)}` } : {}) }}
                onLoad={(e) => {
                  const { naturalWidth: w, naturalHeight: h } = e.currentTarget
                  if (w && h && !(clave(f) in proporciones)) setProporciones((p) => ({ ...p, [clave(f)]: h / w }))
                }}
              />
            </span>
            <span className="ficha-numero">
              {i + 1}
              {!fotos && (unir || f.pagina !== i) && <small> (pág. {f.pagina + 1})</small>}
            </span>
          </button>
        ))}
      </div>
      {fichaArrastrada && arrastre && (
        <img
          className="ficha-fantasma"
          src={url(fichaArrastrada)}
          alt=""
          style={{ left: arrastre.x, top: arrastre.y, transform: `translate(-50%, -50%) rotate(${fichaArrastrada.giro}deg)` }}
        />
      )}
      <div className="organizar-acciones">
        {marcadasEnOrden.length > 0 ? (
          <>
            <span className="organizar-cuenta">{marcadasEnOrden.length} marcadas</span>
            <button onClick={() => mover(-1)} aria-label="Mover antes" title="Mover antes">
              ◀
            </button>
            <button onClick={() => mover(1)} aria-label="Mover después" title="Mover después">
              ▶
            </button>
            {fotos && onRecortar && marcadasEnOrden.length === 1 && (
              <button
                onClick={() => {
                  onRecortar(marcadasEnOrden[0].archivo)
                  setMarcadas(new Set())
                }}
              >
                Recortar
              </button>
            )}
            <button onClick={girar}>Girar</button>
            <button onClick={duplicar}>Duplicar</button>
            <button onClick={quitar}>Quitar</button>
            {!fotos && (
              <button onClick={() => onGuardar(hojas(marcadasEnOrden), true, diseno)} disabled={guardando}>
                Extraer
              </button>
            )}
            <button onClick={() => setMarcadas(new Set())}>Desmarcar</button>
          </>
        ) : fotos ? (
          <>
            {elegirFotos(false)}
            {android && elegirFotos(true)}
          </>
        ) : unir ? (
          <>
            <button onClick={() => void agregar()} disabled={agregando}>
              {agregando ? 'Abriendo…' : '+ Agregar PDF'}
            </button>
            <button onClick={() => cambiar(ordenarPorArchivo(lista, fuentes))} disabled={fuentes.length < 2}>
              Ordenar por archivo
            </button>
          </>
        ) : (
          <span className="organizar-cuenta">Marca hojas para moverlas, girarlas, quitarlas o extraerlas.</span>
        )}
        <button className="grande" onClick={() => onGuardar(hojas(lista), false, diseno)} disabled={guardando || !lista.length}>
          {guardando ? 'Guardando…' : unir ? 'Unir' : fotos ? 'Crear PDF' : 'Guardar copia'}
        </button>
      </div>
    </div>
  )
}

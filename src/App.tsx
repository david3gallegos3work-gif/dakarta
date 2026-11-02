import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import logo from '../assets/logo.jpg'
import { buscarActualizacion, type Actualizacion } from './version.ts'
import { ANCHO_ENTERA_MAX, anchoEntera, imanZoom, limitarZoom, MINIATURA, recorte, type Recorte } from './zoom.ts'

declare global {
  interface Window {
    /** Solo en Android (scripts/parchar-android.py): el PDF que llegó por "Abrir con". */
    DakartaAndroid?: { pendiente(): string | null }
  }
}

type Entrada = { titulo: string; pagina: number | null; hijos: Entrada[] }
type Info = { paginas: number; ancho: number; alto: number; indice: Entrada[]; ms: number }
type Doc = { info: Info; id: number }
/** Punto de una página que debe quedar en el mismo lugar de la pantalla tras un zoom o un giro. */
type Ancla = { i: number; fx: number; fy: number; vx: number; vy: number }
type Rango = [number, number]

const MARGEN = 8
/** Páginas a cada lado de la actual cuya miniatura ya se pide. */
const VENTANA = 15

const fijar = (poner: (f: (r: Rango) => Rango) => void, a: number, b: number) =>
  poner((r) => (r[0] === a && r[1] === b ? r : [a, b]))

const leerNoche = () => {
  try {
    return localStorage.getItem('noche') === '1'
  } catch {
    return false
  }
}

/** Índice de la página que está a la altura y (las páginas están en orden de arriba abajo). */
function buscarPagina(hijos: HTMLCollectionOf<HTMLElement>, y: number) {
  let lo = 0
  let hi = hijos.length - 1
  while (lo < hi) {
    const m = (lo + hi + 1) >> 1
    if (hijos[m].offsetTop <= y) lo = m
    else hi = m - 1
  }
  return lo
}

export default function App() {
  const [doc, setDoc] = useState<Doc | null>(null)
  const [error, setError] = useState('')
  const [abriendo, setAbriendo] = useState(false)
  const [zoom, setZoom] = useState(1)
  const [noche, setNoche] = useState(leerNoche)
  const [verIndice, setVerIndice] = useState(false)
  const [actual, setActual] = useState(0)
  const [centro, setCentro] = useState(0)
  const [visibles, setVisibles] = useState<Rango>([0, 0])
  const [cercanas, setCercanas] = useState<Rango>([0, 0])
  const [recortes, setRecortes] = useState<Recorte[]>([])
  const [anchoVisor, setAnchoVisor] = useState(0)
  const [msPagina, setMsPagina] = useState<number | null>(null)
  const [nueva, setNueva] = useState<Actualizacion | null>(null)
  const visor = useRef<HTMLDivElement>(null)
  const contenido = useRef<HTMLDivElement>(null)
  const zoomRef = useRef(zoom)
  const ajuste = useRef<Ancla | null>(null)
  const reloj = useRef(0)
  const relojVecinas = useRef(0)
  const ultimoY = useRef(0)
  zoomRef.current = zoom

  const hijos = () => contenido.current!.children as HTMLCollectionOf<HTMLElement>

  // La página actual se calcula al instante; el centro de las miniaturas cambia cada pocas páginas.
  const medir = useCallback(() => {
    const v = visor.current!
    const h = hijos()
    if (!h.length) return
    const a = buscarPagina(h, v.scrollTop + v.clientHeight / 2)
    setActual(a)
    setCentro((c) => (Math.abs(c - a) >= 5 ? a : c))
  }, [])

  // Al detenerse: primero lo que está a la vista, para que se vea nítido cuanto antes, y un momento después las vecinas.
  const aterrizar = useCallback(() => {
    const v = visor.current!
    const h = hijos()
    if (!h.length) return
    const { scrollTop: y, clientHeight: alto } = v
    const a = buscarPagina(h, y)
    const b = buscarPagina(h, y + alto)
    fijar(setVisibles, a, b)
    // Con mucho zoom la hoja entera no alcanza: se pide con su detalle exacto solo lo que se está mirando.
    const vista = { x: v.scrollLeft, y, w: v.clientWidth, h: alto }
    setRecortes((previos) => {
      const lista: Recorte[] = []
      for (let n = a; n <= b; n++) {
        const el = h[n]
        if (el.offsetWidth * devicePixelRatio <= ANCHO_ENTERA_MAX) continue
        const caja = { x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight }
        const r = recorte(n, caja, vista, devicePixelRatio, previos.find((p) => p.n === n))
        if (r) lista.push(r)
      }
      return lista
    })
    clearTimeout(relojVecinas.current)
    relojVecinas.current = window.setTimeout(
      () => fijar(setCercanas, buscarPagina(h, y - alto), buscarPagina(h, y + 2 * alto)),
      250,
    )
  }, [])

  // Mientras se desplaza: despacio, se pide nítido lo que está a la vista; muy rápido, solo miniaturas
  // (así no se piden al motor páginas nítidas de las que pasan volando).
  const seguir = useCallback(
    function seguir() {
      const v = visor.current!
      const h = hijos()
      const y = v.scrollTop
      const movido = Math.abs(y - ultimoY.current)
      ultimoY.current = y
      if (movido === 0) {
        reloj.current = 0
        aterrizar()
        return
      }
      if (h.length && movido < v.clientHeight) fijar(setVisibles, buscarPagina(h, y), buscarPagina(h, y + v.clientHeight))
      reloj.current = window.setTimeout(seguir, 100)
    },
    [aterrizar],
  )

  /** El punto (px, py) del contenido debe quedar en el punto (vx, vy) de la pantalla tras el próximo cambio de tamaño. */
  const anclar = useCallback((px: number, py: number, vx: number, vy: number) => {
    const h = hijos()
    if (!h.length) return
    const i = buscarPagina(h, py)
    const el = h[i]
    ajuste.current = { i, vx, vy, fx: (px - el.offsetLeft) / el.offsetWidth, fy: (py - el.offsetTop) / el.offsetHeight }
  }, [])

  /** Devuelve false si el zoom no cambió (ya estaba en el tope). */
  const cambiarZoom = useCallback(
    (
      pedido: number,
      vx = visor.current!.clientWidth / 2,
      vy = visor.current!.clientHeight / 2,
      px = visor.current!.scrollLeft + vx,
      py = visor.current!.scrollTop + vy,
    ) => {
      const z = imanZoom(limitarZoom(pedido))
      if (z === zoomRef.current) return false
      anclar(px, py, vx, vy)
      // Ya, sin esperar a pintar: varios pasos seguidos de la rueda se suman en vez de perderse.
      zoomRef.current = z
      setZoom(z)
      return true
    },
    [anclar],
  )

  useLayoutEffect(() => {
    const v = visor.current!
    const a = ajuste.current
    contenido.current!.style.transform = ''
    contenido.current!.style.willChange = ''
    const el = a && hijos()[a.i]
    if (a && el) {
      v.scrollLeft = el.offsetLeft + a.fx * el.offsetWidth - a.vx
      v.scrollTop = el.offsetTop + a.fy * el.offsetHeight - a.vy
    }
    ajuste.current = null
    medir()
    aterrizar()
  }, [zoom, anchoVisor, doc, medir, aterrizar])

  useEffect(() => {
    const v = visor.current!
    const ro = new ResizeObserver(() => {
      anclar(v.scrollLeft, v.scrollTop, 0, 0)
      setAnchoVisor(v.clientWidth)
    })
    ro.observe(v)
    // Apenas termina de desplazarse se pide lo nítido, sin esperar al siguiente repaso del reloj.
    v.addEventListener('scrollend', aterrizar)
    return () => {
      ro.disconnect()
      v.removeEventListener('scrollend', aterrizar)
    }
  }, [anclar, aterrizar])

  // Zoom: Ctrl + rueda o teclas en la PC, dos dedos en el celular. Mientras los dedos se mueven solo se estira
  // la imagen (fluido); al soltar, el motor vuelve a pintar las páginas nítidas al nuevo tamaño.
  useEffect(() => {
    const v = visor.current!
    const c = contenido.current!
    const punto = (x: number, y: number) => {
      const r = v.getBoundingClientRect()
      return [x - r.left, y - r.top] as const
    }
    const rueda = (e: WheelEvent) => {
      if (!e.ctrlKey) return
      e.preventDefault()
      cambiarZoom(zoomRef.current * (e.deltaY < 0 ? 1.1 : 1 / 1.1), ...punto(e.clientX, e.clientY))
    }
    const tecla = (e: KeyboardEvent) => {
      if (!e.ctrlKey) return
      if (e.key === '+' || e.key === '=') cambiarZoom(zoomRef.current * 1.25)
      else if (e.key === '-') cambiarZoom(zoomRef.current / 1.25)
      else if (e.key === '0') cambiarZoom(1)
      else return
      e.preventDefault()
    }
    let pinza: { d0: number; px: number; py: number; vx: number; vy: number; s: number } | null = null
    const dist = (t: TouchList) => Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY)
    const medio = (t: TouchList) => punto((t[0].clientX + t[1].clientX) / 2, (t[0].clientY + t[1].clientY) / 2)
    const inicio = (e: TouchEvent) => {
      if (e.touches.length !== 2) return
      const [vx, vy] = medio(e.touches)
      pinza = { d0: dist(e.touches), px: v.scrollLeft + vx, py: v.scrollTop + vy, vx, vy, s: 1 }
      c.style.willChange = 'transform'
    }
    const mover = (e: TouchEvent) => {
      if (!pinza || e.touches.length !== 2) return
      if (e.cancelable) e.preventDefault()
      const z0 = zoomRef.current
      const [vx, vy] = medio(e.touches)
      pinza.s = limitarZoom((z0 * dist(e.touches)) / pinza.d0) / z0
      pinza.vx = vx
      pinza.vy = vy
      // El punto de la página que quedó entre los dedos los sigue, aunque el primer dedo ya haya empezado a desplazar.
      const tx = vx + v.scrollLeft - pinza.px * pinza.s
      const ty = vy + v.scrollTop - pinza.py * pinza.s
      c.style.transform = `translate(${tx}px, ${ty}px) scale(${pinza.s})`
    }
    const fin = (e: TouchEvent) => {
      if (!pinza || e.touches.length === 2) return
      const { s, px, py, vx, vy } = pinza
      pinza = null
      if (cambiarZoom(zoomRef.current * s, vx, vy, px, py)) return
      // El zoom no cambió, pero los dedos pudieron mover la página.
      c.style.transform = ''
      c.style.willChange = ''
      v.scrollLeft = px - vx
      v.scrollTop = py - vy
    }
    v.addEventListener('wheel', rueda, { passive: false })
    v.addEventListener('touchstart', inicio, { passive: true })
    v.addEventListener('touchmove', mover, { passive: false })
    v.addEventListener('touchend', fin)
    v.addEventListener('touchcancel', fin)
    window.addEventListener('keydown', tecla)
    return () => {
      v.removeEventListener('wheel', rueda)
      v.removeEventListener('touchstart', inicio)
      v.removeEventListener('touchmove', mover)
      v.removeEventListener('touchend', fin)
      v.removeEventListener('touchcancel', fin)
      window.removeEventListener('keydown', tecla)
    }
  }, [cambiarZoom])

  useEffect(() => {
    try {
      localStorage.setItem('noche', noche ? '1' : '0')
    } catch {
      // sin almacenamiento: el modo noche no se recuerda al volver a abrir
    }
  }, [noche])

  const ir = useCallback(
    (n: number) => {
      const el = hijos()[n]
      if (!el) return
      visor.current!.scrollTop = el.offsetTop - MARGEN
      ultimoY.current = visor.current!.scrollTop
      aterrizar()
    },
    [aterrizar],
  )

  const alCargar = useCallback((ms: number) => setMsPagina(Math.round(ms)), [])

  async function cargar(pedido: Promise<Info | null>) {
    setAbriendo(true)
    setError('')
    try {
      const info = await pedido
      if (info) {
        setDoc((d) => ({ info, id: (d?.id ?? 0) + 1 }))
        setZoom(1)
        setVerIndice(false)
        setMsPagina(null)
        setCentro(0)
        setVisibles([0, 0])
        setCercanas([0, 0])
        setRecortes([])
        ajuste.current = null
        visor.current!.scrollTo(0, 0)
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setAbriendo(false)
    }
  }

  const abrir = () => cargar(invoke<Info | null>('elegir_pdf'))

  // "Abrir con Dakarta": Windows lo entrega al arrancar; Android también con la app ya abierta, al volver a ella.
  useEffect(() => {
    const abrirRuta = (ruta: string) => cargar(invoke<Info>('abrir_ruta', { ruta }))
    const pendiente = () => {
      const ruta = window.DakartaAndroid?.pendiente()
      if (ruta) abrirRuta(ruta)
    }
    invoke<string | null>('archivo_inicial').then((ruta) => {
      if (ruta) abrirRuta(ruta)
    })
    pendiente()
    document.addEventListener('visibilitychange', pendiente)
    window.addEventListener('focus', pendiente)
    buscarActualizacion().then(setNueva)
    return () => {
      document.removeEventListener('visibilitychange', pendiente)
      window.removeEventListener('focus', pendiente)
    }
  }, [])

  const ancho = Math.max(100, Math.min(anchoVisor - 2 * MARGEN, 1000)) * zoom
  // Mientras se sigue cambiando el zoom la imagen anterior solo se estira; la nítida se pide al terminar.
  const [anchoPedido, setAnchoPedido] = useState(ancho)
  useEffect(() => {
    const t = setTimeout(() => setAnchoPedido(ancho), 150)
    return () => clearTimeout(t)
  }, [ancho])

  return (
    <div className={noche ? 'app noche' : 'app'}>
      <header className="barra">
        <button onClick={abrir} disabled={abriendo}>
          {abriendo ? 'Abriendo…' : 'Abrir'}
        </button>
        {doc && (
          <>
            <button onClick={() => setVerIndice((x) => !x)} aria-pressed={verIndice} aria-label="Índice" title="Índice">
              ☰
            </button>
            <span className="pag">
              <input
                key={actual}
                type="number"
                inputMode="numeric"
                min={1}
                max={doc.info.paginas}
                defaultValue={actual + 1}
                aria-label="Ir a la página"
                onKeyDown={(e) => {
                  if (e.key !== 'Enter') return
                  ir(Math.min(doc.info.paginas, Math.max(1, Number(e.currentTarget.value) || 1)) - 1)
                  e.currentTarget.blur()
                }}
              />
              / {doc.info.paginas}
            </span>
            <button onClick={() => cambiarZoom(zoom / 1.25)} aria-label="Alejar" title="Alejar">
              −
            </button>
            <button onClick={() => cambiarZoom(1)} title="Ajustar al ancho" className="porcentaje">
              {Math.round(zoom * 100)}%
            </button>
            <button onClick={() => cambiarZoom(zoom * 1.25)} aria-label="Acercar" title="Acercar">
              +
            </button>
          </>
        )}
        <button
          onClick={() => setNoche((x) => !x)}
          aria-pressed={noche}
          aria-label="Modo noche"
          title="Modo noche"
          className={doc ? '' : 'derecha'}
        >
          {noche ? '☀' : '☾'}
        </button>
      </header>

      <div className="cuerpo">
        {doc && verIndice && (
          <nav className="indice" aria-label="Índice">
            {doc.info.indice.length ? (
              <Indice
                entradas={doc.info.indice}
                ir={(n) => {
                  ir(n)
                  if (anchoVisor < 700) setVerIndice(false)
                }}
              />
            ) : (
              <p>Este documento no tiene índice.</p>
            )}
          </nav>
        )}
        <div
          className="visor"
          ref={visor}
          onScroll={() => {
            medir()
            clearTimeout(relojVecinas.current)
            if (!reloj.current) reloj.current = window.setTimeout(seguir, 100)
          }}
        >
          <div className="contenido" ref={contenido}>
            {doc && (
              <Paginas
                key={doc.id}
                doc={doc}
                ancho={ancho}
                anchoPedido={anchoPedido}
                centro={centro}
                visibles={visibles}
                cercanas={cercanas}
                recortes={recortes}
                alCargar={alCargar}
              />
            )}
          </div>
        </div>
        {!doc && (
          <div className="bienvenida">
            <img src={logo} alt="Dakarta PDF" />
            <button className="grande" onClick={abrir} disabled={abriendo}>
              {abriendo ? 'Abriendo…' : 'Abrir un PDF'}
            </button>
            {error && <p className="error">{error}</p>}
            <p className="creditos">
              Gratis, sin anuncios y sin rastreo. Motor PDF: MuPDF © Artifex Software, Inc. Licencia AGPL-3.0.
            </p>
          </div>
        )}
        {doc && error && (
          <button className="aviso-error" onClick={() => setError('')}>
            {error} (tocar para cerrar)
          </button>
        )}
        {doc && (
          <div className="tiempos">
            Abrió en {doc.info.ms} ms{msPagina !== null && ` · Página en ${msPagina} ms`}
          </div>
        )}
        {nueva && (
          <div className="aviso-version" role="status">
            <span>Hay una versión nueva de Dakarta ({nueva.version}).</span>
            <button
              onClick={() => {
                invoke('descargar_version', { url: nueva.url }).catch((e) => setError(String(e)))
              }}
            >
              Descargar
            </button>
            <button onClick={() => setNueva(null)} aria-label="Cerrar aviso">
              ×
            </button>
          </div>
        )}
      </div>
    </div>
  )
}

/**
 * Todas las páginas reservan su espacio. Cada una muestra, de abajo arriba: su número, su miniatura (una vez pedida
 * se queda, por eso al volver a una hoja ya se ve), su imagen nítida (solo cerca de la pantalla) y, con mucho zoom,
 * el recorte con el detalle exacto de lo que se está mirando.
 */
const Paginas = memo(function Paginas({
  doc,
  ancho,
  anchoPedido,
  centro,
  visibles,
  cercanas,
  recortes,
  alCargar,
}: {
  doc: Doc
  ancho: number
  anchoPedido: number
  centro: number
  visibles: Rango
  cercanas: Rango
  recortes: Recorte[]
  alCargar: (ms: number) => void
}) {
  // Alto/ancho real de las páginas ya pintadas que no miden como la primera.
  const [prop, setProp] = useState<Record<number, number>>({})
  const vistas = useRef(new Set<number>())
  const base = doc.info.alto / doc.info.ancho
  const medida = useCallback(
    (n: number, real: number) =>
      setProp((x) => (Math.abs(real - (x[n] ?? base)) > 0.01 * base ? { ...x, [n]: real } : x)),
    [base],
  )
  for (let i = Math.max(0, centro - VENTANA); i <= Math.min(doc.info.paginas - 1, centro + VENTANA); i++) {
    vistas.current.add(i)
  }
  const w = anchoEntera(anchoPedido * devicePixelRatio)
  const dentro = (r: Rango, n: number) => n >= r[0] && n <= r[1]
  return Array.from({ length: doc.info.paginas }, (_, n) => (
    <Pagina
      key={n}
      n={n}
      id={doc.id}
      ancho={ancho}
      alto={ancho * (prop[n] ?? base)}
      mini={vistas.current.has(n)}
      nitida={dentro(visibles, n) || dentro(cercanas, n) ? w : 0}
      recorte={recortes.find((r) => r.n === n)}
      medida={medida}
      alCargar={alCargar}
    />
  ))
})

const Pagina = memo(function Pagina({
  n,
  id,
  ancho,
  alto,
  mini,
  nitida,
  recorte,
  medida,
  alCargar,
}: {
  n: number
  id: number
  ancho: number
  alto: number
  mini: boolean
  nitida: number
  recorte?: Recorte
  medida: (n: number, real: number) => void
  alCargar: (ms: number) => void
}) {
  const url = (w: number) => `${convertFileSrc(String(n), 'pagina')}?w=${w}&d=${id}`
  return (
    <div className="pagina" style={{ width: ancho, height: alto }}>
      <span className="numero">{n + 1}</span>
      {mini && (
        <img
          src={url(MINIATURA)}
          alt=""
          decoding="async"
          draggable={false}
          onLoad={(e) => medida(n, e.currentTarget.naturalHeight / e.currentTarget.naturalWidth)}
        />
      )}
      {nitida > 0 && (
        <Imagen
          src={url(nitida)}
          alCargar={(ms, real) => {
            alCargar(ms)
            medida(n, real)
          }}
        />
      )}
      {recorte && <Detalle r={recorte} id={id} alCargar={alCargar} />}
    </div>
  )
})

/**
 * El recorte con el detalle exacto. Se posiciona en % de la página: al cambiar el zoom sigue en su sitio, solo estirado.
 * El anterior se queda visible hasta que llega el nuevo; encima, el nuevo es transparente mientras no ha cargado.
 */
function Detalle({ r, id, alCargar }: { r: Recorte; id: number; alCargar: (ms: number) => void }) {
  const direccion = (c: Recorte) =>
    `${convertFileSrc(String(c.n), 'pagina')}?w=${c.W}&h=${c.H}&x=${c.x}&y=${c.y}&cw=${c.cw}&ch=${c.ch}&d=${id}`
  const t0 = useMemo(() => performance.now(), [r])
  const [listo, setListo] = useState<Recorte | null>(null)
  return (listo && listo !== r ? [listo, r] : [r]).map((c) => (
    <img
      key={direccion(c)}
      src={direccion(c)}
      alt=""
      decoding="sync"
      draggable={false}
      style={{
        left: `${(c.x / c.W) * 100}%`,
        top: `${(c.y / c.H) * 100}%`,
        width: `${(c.cw / c.W) * 100}%`,
        height: `${(c.ch / c.H) * 100}%`,
      }}
      onLoad={() => {
        if (c === r) alCargar(performance.now() - t0)
        setListo(c)
      }}
    />
  ))
}

function Imagen({ src, alCargar }: { src: string; alCargar: (ms: number, proporcion: number) => void }) {
  // Al cambiar el zoom cambia src y el mismo <img> sigue mostrando la imagen anterior hasta que llega la nítida.
  const t0 = useMemo(() => performance.now(), [src])
  const [fallo, setFallo] = useState('')
  if (fallo === src) return <span className="fallo">No se pudo mostrar esta página</span>
  return (
    <img
      src={src}
      alt=""
      decoding="async"
      draggable={false}
      onLoad={(e) => alCargar(performance.now() - t0, e.currentTarget.naturalHeight / e.currentTarget.naturalWidth)}
      onError={() => setFallo(src)}
    />
  )
}

function Indice({ entradas, ir }: { entradas: Entrada[]; ir: (n: number) => void }) {
  return (
    <ul>
      {entradas.map((e, i) => (
        <li key={i}>
          <button disabled={e.pagina === null} onClick={() => e.pagina !== null && ir(e.pagina)}>
            {e.titulo}
          </button>
          {e.hijos.length > 0 && <Indice entradas={e.hijos} ir={ir} />}
        </li>
      ))}
    </ul>
  )
}

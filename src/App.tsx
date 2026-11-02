import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import logo from '../assets/logo.jpg'
import { guardarReciente, leerRecientes, olvidarReciente, type Reciente } from './recientes.ts'
import { buscarActualizacion, type Actualizacion } from './version.ts'
import { ANCHO_ENTERA_MAX, anchoEntera, imanZoom, limitarZoom, MINIATURA, recorte, type Recorte } from './zoom.ts'

declare global {
  interface Window {
    /** Solo en Android (scripts/parchar-android.py): el PDF que llegó por "Abrir con", su nombre y el selector. */
    DakartaAndroid?: {
      pendiente(): string | null
      nombre(uri: string): string | null
      persistir(uri: string): void
      elegir?(): void
    }
    /** Android: el botón o gesto "atrás" pregunta si la pantalla lo usó o si toca salir. */
    DakartaAtras?: () => boolean
  }
}

type Entrada = { titulo: string; pagina: number | null; hijos: Entrada[] }
type Info = {
  paginas: number
  ancho: number
  alto: number
  indice: Entrada[]
  ms: number
  /** Para volver a abrirlo después: una ruta en Windows, un content:// en Android. */
  ruta: string
  nombre: string
}
type Doc = { info: Info; id: number }
type Coincidencia = { pagina: number; x: number; y: number; w: number; h: number }
type Busqueda = { texto: string; resultados: Coincidencia[]; escaneado: boolean; truncada: boolean }
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
  const [nueva, setNueva] = useState<Actualizacion | null>(null)
  const [recientes, setRecientes] = useState<Reciente[]>(leerRecientes)
  const [verBuscar, setVerBuscar] = useState(false)
  const [textoBuscar, setTextoBuscar] = useState('')
  const [busqueda, setBusqueda] = useState<Busqueda | null>(null)
  const [buscando, setBuscando] = useState(false)
  const [coincidencia, setCoincidencia] = useState(-1)
  const visor = useRef<HTMLDivElement>(null)
  const contenido = useRef<HTMLDivElement>(null)
  const arrastre = useRef<{ x: number; y: number } | null>(null)
  const zoomRef = useRef(zoom)
  const ajuste = useRef<Ancla | null>(null)
  const reloj = useRef(0)
  const relojVecinas = useRef(0)
  const ultimoY = useRef(0)
  const pedidoBusqueda = useRef(0)
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
    let toque: { x: number; y: number; t: number } | null = null
    let ultimoToque = 0
    let ultimoPunto = { x: 0, y: 0 }
    const dist = (t: TouchList) => Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY)
    const medio = (t: TouchList) => punto((t[0].clientX + t[1].clientX) / 2, (t[0].clientY + t[1].clientY) / 2)
    const inicio = (e: TouchEvent) => {
      if (e.touches.length !== 2) {
        toque = e.touches.length === 1 ? { x: e.touches[0].clientX, y: e.touches[0].clientY, t: performance.now() } : null
        return
      }
      toque = null
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
      if (pinza) {
        if (e.touches.length === 2) return
        const { s, px, py, vx, vy } = pinza
        pinza = null
        if (cambiarZoom(zoomRef.current * s, vx, vy, px, py)) return
        // El zoom no cambió, pero los dedos pudieron mover la página.
        c.style.transform = ''
        c.style.willChange = ''
        v.scrollLeft = px - vx
        v.scrollTop = py - vy
        return
      }
      // Dos toques seguidos y quietos en el mismo lugar: normal ↔ mitad (50%).
      if (e.type === 'touchcancel' || e.touches.length !== 0 || !toque) {
        toque = null
        return
      }
      const { clientX: x, clientY: y } = e.changedTouches[0]
      const ahora = performance.now()
      const quieto = Math.hypot(x - toque.x, y - toque.y) < 12 && ahora - toque.t < 300
      toque = null
      if (!quieto) {
        ultimoToque = 0
        return
      }
      const [vx, vy] = punto(x, y)
      if (ahora - ultimoToque < 300 && Math.hypot(x - ultimoPunto.x, y - ultimoPunto.y) < 40) {
        ultimoToque = 0
        cambiarZoom(zoomRef.current === 1 ? 0.5 : 1, vx, vy)
      } else {
        ultimoToque = ahora
        ultimoPunto = { x, y }
      }
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

  /** Lleva la coincidencia a la vista: la página arriba y el texto a un tercio de la pantalla. */
  const irA = useCallback(
    (c: Coincidencia) => {
      const el = hijos()[c.pagina]
      if (!el) return
      const v = visor.current!
      const y = el.offsetTop + (c.y + c.h / 2) * el.offsetHeight
      v.scrollTop = Math.max(0, y - v.clientHeight / 3)
      const x = el.offsetLeft + (c.x + c.w / 2) * el.offsetWidth
      if (x < v.scrollLeft || x > v.scrollLeft + v.clientWidth) v.scrollLeft = Math.max(0, x - v.clientWidth / 2)
      ultimoY.current = v.scrollTop
      aterrizar()
    },
    [aterrizar],
  )

  const cerrarBuscar = useCallback(() => {
    pedidoBusqueda.current++
    setVerBuscar(false)
    setBusqueda(null)
    setBuscando(false)
    setCoincidencia(-1)
  }, [])

  const buscar = useCallback(async () => {
    const texto = textoBuscar.trim()
    if (!texto) return
    const id = ++pedidoBusqueda.current
    setBuscando(true)
    setError('')
    try {
      const r = await invoke<Omit<Busqueda, 'texto'>>('buscar', { texto })
      if (id !== pedidoBusqueda.current) return
      setBusqueda({ ...r, texto })
      setCoincidencia(r.resultados.length ? 0 : -1)
      if (r.resultados.length) irA(r.resultados[0])
    } catch (e) {
      if (id === pedidoBusqueda.current) setError(String(e))
    } finally {
      if (id === pedidoBusqueda.current) setBuscando(false)
    }
  }, [textoBuscar, irA])

  const mover = useCallback(
    (paso: number) => {
      if (!busqueda?.resultados.length) return
      const total = busqueda.resultados.length
      const i = (coincidencia + paso + total) % total
      setCoincidencia(i)
      irA(busqueda.resultados[i])
    },
    [busqueda, coincidencia, irA],
  )

  async function cargar(pedido: Promise<Info | null>, reciente?: Reciente) {
    setAbriendo(true)
    setError('')
    try {
      const info = await pedido
      if (info) {
        setDoc((d) => ({ info, id: (d?.id ?? 0) + 1 }))
        setZoom(1)
        setVerIndice(false)
        setCentro(0)
        setVisibles([0, 0])
        setCercanas([0, 0])
        setRecortes([])
        ajuste.current = null
        visor.current!.scrollTo(0, 0)
        setVerBuscar(false)
        setBusqueda(null)
        setCoincidencia(-1)
        pedidoBusqueda.current++

        const puente = window.DakartaAndroid
        let nombre = info.nombre
        if (puente) {
          try {
            puente.persistir(info.ruta)
            nombre = puente.nombre(info.ruta) || nombre
          } catch {
            // el aviso nativo no está disponible: se usa el nombre que dio el sistema
          }
        }
        setRecientes((lista) => guardarReciente(lista, { ruta: info.ruta, nombre }))
      }
    } catch (e) {
      console.error(e)
      setError(reciente ? `No se pudo abrir «${reciente.nombre}». Ábrelo otra vez con «Abrir».` : 'No se pudo abrir el archivo. Inténtalo otra vez con «Abrir».')
      if (reciente) setRecientes((lista) => olvidarReciente(lista, reciente.ruta))
    } finally {
      setAbriendo(false)
    }
  }

  // En Android el selector lo abre la propia app: así el permiso queda guardado y el archivo se puede reabrir otro día.
  const abrir = () => {
    const puente = window.DakartaAndroid
    if (puente?.elegir) {
      puente.elegir()
      return
    }
    cargar(invoke<Info | null>('elegir_pdf'))
  }

  // El botón o gesto "atrás" de Android cierra primero el panel o la búsqueda; si no hay nada abierto, sale.
  useEffect(() => {
    window.DakartaAtras = () => {
      if (verIndice) {
        setVerIndice(false)
        return true
      }
      if (verBuscar) {
        cerrarBuscar()
        return true
      }
      return false
    }
    return () => {
      delete window.DakartaAtras
    }
  }, [verIndice, verBuscar, cerrarBuscar])

  // "Abrir con Dakarta": Windows lo entrega al arrancar; Android también con la app ya abierta, al volver a ella.
  // Si nadie mandó un archivo, se reabre el último que se usó.
  useEffect(() => {
    let llegoArchivo = false
    const abrirRuta = (ruta: string, reciente?: Reciente) => {
      llegoArchivo = true
      cargar(invoke<Info>('abrir_ruta', { ruta }), reciente)
    }
    const pendiente = () => {
      const ruta = window.DakartaAndroid?.pendiente()
      if (ruta) abrirRuta(ruta)
    }
    pendiente()
    invoke<string | null>('archivo_inicial').then((ruta) => {
      if (ruta) abrirRuta(ruta)
      else if (!llegoArchivo) {
        const [ultimo] = leerRecientes()
        if (ultimo) abrirRuta(ultimo.ruta, ultimo)
      }
    })
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
        {doc && (
          <>
            <button onClick={() => setVerIndice((x) => !x)} aria-pressed={verIndice} aria-label="Índice" title="Índice">
              ☰
            </button>
            <button
              onClick={() => (verBuscar ? cerrarBuscar() : setVerBuscar(true))}
              aria-pressed={verBuscar}
              aria-label="Buscar en el documento"
              title="Buscar en el documento"
            >
              🔍
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

      {verBuscar && doc && (
        <div className="buscar">
          <div className="fila">
            <input
              autoFocus
              type="search"
              value={textoBuscar}
              onChange={(e) => setTextoBuscar(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  buscar()
                } else if (e.key === 'Escape') {
                  cerrarBuscar()
                }
              }}
              placeholder="Buscar en el documento"
              enterKeyHint="search"
              aria-label="Buscar en el documento"
            />
            <span className="cuenta" aria-live="polite">
              {buscando
                ? 'Buscando…'
                : busqueda && (busqueda.resultados.length ? `${coincidencia + 1} de ${busqueda.resultados.length}` : 'Sin resultados')}
            </span>
            <button onClick={() => mover(-1)} disabled={!busqueda?.resultados.length} aria-label="Resultado anterior" title="Resultado anterior">
              ↑
            </button>
            <button onClick={() => mover(1)} disabled={!busqueda?.resultados.length} aria-label="Resultado siguiente" title="Resultado siguiente">
              ↓
            </button>
            <button onClick={cerrarBuscar} aria-label="Cerrar búsqueda" title="Cerrar búsqueda">
              ×
            </button>
          </div>
          {busqueda?.truncada && <p className="buscar-nota">Hay más resultados. Escribe una palabra más específica.</p>}
          {busqueda && !busqueda.resultados.length && (
            <p className="buscar-aviso">
              {busqueda.escaneado
                ? 'Este PDF parece escaneado: no tiene texto para buscar.'
                : `No se encontró «${busqueda.texto}».`}
            </p>
          )}
        </div>
      )}

      <div className="cuerpo">
        {doc && verIndice && (
          <nav
            className="indice"
            aria-label="Índice"
            onTouchStart={(e) => {
              arrastre.current = e.touches.length === 1 ? { x: e.touches[0].clientX, y: e.touches[0].clientY } : null
            }}
            onTouchEnd={(e) => {
              const a = arrastre.current
              arrastre.current = null
              if (!a) return
              const t = e.changedTouches[0]
              // Deslizar el dedo hacia la izquierda cierra el panel, como retroceder.
              if (a.x - t.clientX > 60 && Math.abs(t.clientY - a.y) < 50) setVerIndice(false)
            }}
          >
            <button className="abrir-indice" onClick={abrir} disabled={abriendo}>
              {abriendo ? 'Abriendo…' : 'Abrir'}
            </button>
            {recientes.length > 0 && (
              <div className="recientes-panel">
                <h2>Últimos archivos</h2>
                <ul>
                  {recientes.slice(0, 4).map((r) => (
                    <li key={r.ruta}>
                      <button
                        onClick={() => cargar(invoke<Info>('abrir_ruta', { ruta: r.ruta }), r)}
                        disabled={abriendo}
                        title={r.nombre}
                      >
                        {r.nombre}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
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
                busqueda={busqueda}
                coincidencia={coincidencia}
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
            {recientes.length > 0 && (
              <div className="recientes">
                <h2>Últimos archivos</h2>
                <ul>
                  {recientes.map((r) => (
                    <li key={r.ruta}>
                      <button
                        onClick={() => cargar(invoke<Info>('abrir_ruta', { ruta: r.ruta }), r)}
                        disabled={abriendo}
                        title={r.nombre}
                      >
                        {r.nombre}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
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
  busqueda,
  coincidencia,
}: {
  doc: Doc
  ancho: number
  anchoPedido: number
  centro: number
  visibles: Rango
  cercanas: Rango
  recortes: Recorte[]
  busqueda: Busqueda | null
  coincidencia: number
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
  const porPagina = useMemo(() => {
    const m = new Map<number, { c: Coincidencia; i: number }[]>()
    busqueda?.resultados.forEach((c, i) => {
      const lista = m.get(c.pagina)
      if (lista) lista.push({ c, i })
      else m.set(c.pagina, [{ c, i }])
    })
    return m
  }, [busqueda])
  return Array.from({ length: doc.info.paginas }, (_, n) => {
    // Solo se pintan los resaltados de lo que se está viendo: una palabra común no llena la pantalla de capas.
    const resaltados = dentro(visibles, n) || dentro(cercanas, n) ? porPagina.get(n) : undefined
    return (
      <Pagina
        key={n}
        n={n}
        id={doc.id}
        ancho={ancho}
        alto={ancho * (prop[n] ?? base)}
        mini={vistas.current.has(n)}
        nitida={dentro(visibles, n) || dentro(cercanas, n) ? w : 0}
        recorte={recortes.find((r) => r.n === n)}
        resaltados={resaltados}
        actual={resaltados?.some((x) => x.i === coincidencia) ? coincidencia : -1}
        medida={medida}
      />
    )
  })
})

const Pagina = memo(function Pagina({
  n,
  id,
  ancho,
  alto,
  mini,
  nitida,
  recorte,
  resaltados,
  actual,
  medida,
}: {
  n: number
  id: number
  ancho: number
  alto: number
  mini: boolean
  nitida: number
  recorte?: Recorte
  resaltados?: { c: Coincidencia; i: number }[]
  actual: number
  medida: (n: number, real: number) => void
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
      {nitida > 0 && <Imagen src={url(nitida)} alCargar={(real) => medida(n, real)} />}
      {recorte && <Detalle r={recorte} id={id} />}
      {resaltados?.map(({ c, i }) => (
        <span
          key={i}
          className={i === actual ? 'resaltado actual' : 'resaltado'}
          style={{
            left: `${c.x * 100}%`,
            top: `${c.y * 100}%`,
            width: `${c.w * 100}%`,
            height: `${c.h * 100}%`,
          }}
        />
      ))}
    </div>
  )
})

/**
 * El recorte con el detalle exacto. Se posiciona en % de la página: al cambiar el zoom sigue en su sitio, solo estirado.
 * El anterior se queda visible hasta que llega el nuevo; encima, el nuevo es transparente mientras no ha cargado.
 */
function Detalle({ r, id }: { r: Recorte; id: number }) {
  const direccion = (c: Recorte) =>
    `${convertFileSrc(String(c.n), 'pagina')}?w=${c.W}&h=${c.H}&x=${c.x}&y=${c.y}&cw=${c.cw}&ch=${c.ch}&d=${id}`
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
      onLoad={() => setListo(c)}
    />
  ))
}

function Imagen({ src, alCargar }: { src: string; alCargar: (proporcion: number) => void }) {
  // Al cambiar el zoom cambia src y el mismo <img> sigue mostrando la imagen anterior hasta que llega la nítida.
  const [fallo, setFallo] = useState('')
  if (fallo === src) return <span className="fallo">No se pudo mostrar esta página</span>
  return (
    <img
      src={src}
      alt=""
      decoding="async"
      draggable={false}
      onLoad={(e) => alCargar(e.currentTarget.naturalHeight / e.currentTarget.naturalWidth)}
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

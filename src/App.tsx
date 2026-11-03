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
      compartir?(uri: string, nombre: string): void
      guardar?(nombre: string): void
      destino?(): string | null
      copiar?(destino: string, origen: string): boolean
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
type Herramienta =
  | 'mover'
  | 'resaltar'
  | 'subrayar'
  | 'tachar'
  | 'lapiz'
  | 'nota'
  | 'texto'
  | 'firma'
  | 'formulario'
  | 'sello'
type Campo = {
  pagina: number
  xref: number
  tipo: string
  nombre: string
  valor: string
  readonly: boolean
  x: number
  y: number
  w: number
  h: number
}
/** Anotación propia que se puede tocar para mover, cambiar o borrar. */
type Anotacion = {
  pagina: number
  xref: number
  tipo: 'nota' | 'texto' | 'tinta' | 'resaltar' | 'subrayar' | 'tachar'
  contenido: string
  rojo: number
  verde: number
  azul: number
  tamano: number
  x: number
  y: number
  w: number
  h: number
}
type Preview = {
  left: number
  top: number
  width: number
  height: number
  modo: 'texto' | 'tinta'
  x0: number
  y0: number
  x1: number
  y1: number
  puntos: [number, number][]
}

const COLORES_RESALTADO: ReadonlyArray<readonly [string, string]> = [
  ['#ffeb3b', 'Amarillo'],
  ['#a5d6a7', 'Verde'],
  ['#f8bbd0', 'Rosado'],
  ['#90caf9', 'Celeste'],
]
const COLORES_FUERTES: ReadonlyArray<readonly [string, string]> = [
  ['#000000', 'Negro'],
  ['#d32f2f', 'Rojo'],
  ['#1565c0', 'Azul'],
  ['#2e7d32', 'Verde'],
  ['#6a1b9a', 'Morado'],
]
const COLORES_NOTA: ReadonlyArray<readonly [string, string]> = [
  ['#ffc107', 'Amarillo'],
  ['#f44336', 'Rojo'],
  ['#2196f3', 'Azul'],
  ['#4caf50', 'Verde'],
]
const COLORES_SELLO: ReadonlyArray<readonly [string, string]> = [
  ['#2e7d32', 'Verde'],
  ['#1565c0', 'Azul'],
  ['#c62828', 'Rojo'],
  ['#6a1b9a', 'Morado'],
]
const SELLOS: ReadonlyArray<readonly [string, string]> = [
  ['APROBADO', '#2e7d32'],
  ['RECIBIDO', '#1565c0'],
  ['PAGADO', '#2e7d32'],
  ['CONFIDENCIAL', '#c62828'],
]
const GROSORES: ReadonlyArray<readonly [number, string]> = [
  [1.5, 'Fino'],
  [3, 'Medio'],
  [6, 'Grueso'],
]
const TAMANOS_TEXTO: ReadonlyArray<readonly [number, string]> = [
  [10, 'Pequeño'],
  [12, 'Normal'],
  [18, 'Grande'],
  [24, 'Título'],
]
const TAMANOS_NOTA: ReadonlyArray<readonly [number, string]> = [
  [24, 'Pequeña'],
  [40, 'Normal'],
  [56, 'Grande'],
]
const TAMANOS_SELLO: ReadonlyArray<readonly [number, string]> = [
  [18, 'Normal'],
  [24, 'Grande'],
  [32, 'Muy grande'],
]
const clamp01 = (v: number) => Math.min(1, Math.max(0, v))
const rgb = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16)
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}
const hex = (r: number, g: number, b: number) =>
  '#' +
  [r, g, b]
    .map((v) =>
      Math.round(clamp01(v) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')

/** Android: el selector de "Guardar copia" avisa por un evento; aquí se espera su destino. */
function esperarDestino(nombre: string): Promise<string | null> {
  return new Promise((resolve) => {
    const puente = window.DakartaAndroid
    if (!puente?.guardar) {
      resolve(null)
      return
    }
    const al = () => {
      window.removeEventListener('dakarta-guardar', al)
      resolve(puente.destino?.() ?? null)
    }
    window.addEventListener('dakarta-guardar', al)
    puente.guardar(nombre)
  })
}

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
  const [editando, setEditando] = useState(false)
  const [herramienta, setHerramienta] = useState<Herramienta>('mover')
  const [colorTexto, setColorTexto] = useState(COLORES_FUERTES[0][0])
  const [colorResaltado, setColorResaltado] = useState(COLORES_RESALTADO[0][0])
  const [colorLapiz, setColorLapiz] = useState(COLORES_FUERTES[0][0])
  const [colorNota, setColorNota] = useState(COLORES_NOTA[0][0])
  const [colorSello, setColorSello] = useState(COLORES_SELLO[0][0])
  const [grosor, setGrosor] = useState(3)
  const [tamanoTexto, setTamanoTexto] = useState(12)
  const [tamanoSello, setTamanoSello] = useState(18)
  const [tamanoNota, setTamanoNota] = useState(40)
  /** La versión de cada página: al dibujar solo se vuelve a pedir esa, no todas. */
  const [revisiones, setRevisiones] = useState<Record<number, number>>({})
  /** Para deshacer todo, que afecta a varias páginas. */
  const [revisionTotal, setRevisionTotal] = useState(0)
  const [anots, setAnots] = useState<Record<number, Anotacion[]>>({})
  const [seleccion, setSeleccion] = useState<Anotacion | null>(null)
  const [campoPedido, setCampoPedido] = useState<{ campo: Campo; valor: string } | null>(null)
  const [sucio, setSucio] = useState(false)
  const [hechas, setHechas] = useState(0)
  const [avisoEdicion, setAvisoEdicion] = useState(false)
  const [ayudaManual, setAyudaManual] = useState(false)
  const [recordarAviso, setRecordarAviso] = useState(false)
  const [preview, setPreview] = useState<Preview | null>(null)
  const [rutaActual, setRutaActual] = useState('')
  const [nombreActual, setNombreActual] = useState('')
  const [exito, setExito] = useState('')
  const [guardando, setGuardando] = useState(false)
  const [textoPedido, setTextoPedido] = useState<{ modo: 'nota' | 'texto'; n: number; x: number; y: number } | null>(null)
  const [textoEntrada, setTextoEntrada] = useState('')
  const [padAbierto, setPadAbierto] = useState(false)
  const [padTrazos, setPadTrazos] = useState<[number, number][][]>([])
  const [firmaTrazos, setFirmaTrazos] = useState<[number, number][][] | null>(null)
  const [campos, setCampos] = useState<Campo[]>([])
  const [selloAbierto, setSelloAbierto] = useState(false)
  const [selloPendiente, setSelloPendiente] = useState<{ texto: string } | null>(null)
  const [verBuscar, setVerBuscar] = useState(false)
  const [textoBuscar, setTextoBuscar] = useState('')
  const [busqueda, setBusqueda] = useState<Busqueda | null>(null)
  const [buscando, setBuscando] = useState(false)
  const [coincidencia, setCoincidencia] = useState(-1)
  const visor = useRef<HTMLDivElement>(null)
  const contenido = useRef<HTMLDivElement>(null)
  const arrastre = useRef<{ x: number; y: number } | null>(null)
  const gesto = useRef<{
    n: number
    page: HTMLElement
    modo: 'texto' | 'tinta' | 'tap'
    tipo: Herramienta
    puntos: [number, number][]
  } | null>(null)
  const padActual = useRef<[number, number][] | null>(null)
  const avisoVisto = useRef(false)
  const sucioRef = useRef(sucio)
  const editandoRef = useRef(editando)
  const herramientaRef = useRef(herramienta)
  const zoomRef = useRef(zoom)
  const ajuste = useRef<Ancla | null>(null)
  const reloj = useRef(0)
  const relojVecinas = useRef(0)
  const ultimoY = useRef(0)
  const pedidoBusqueda = useRef(0)
  zoomRef.current = zoom
  sucioRef.current = sucio
  editandoRef.current = editando
  herramientaRef.current = herramienta

  const hijos = () => contenido.current!.children as HTMLCollectionOf<HTMLElement>

  /** Los errores internos de MuPDF no se muestran en crudo: se guarda el detalle para revisarlo. */
  const fallo = useCallback((e: unknown) => {
    console.error(e)
    const detalle = String(e)
    setError(detalle.startsWith('MuPDF error') ? 'No se pudo hacer ese cambio en el PDF. Inténtalo otra vez.' : detalle)
  }, [])

  const cargarAnotaciones = useCallback(async (n: number) => {
    try {
      const lista = await invoke<Anotacion[]>('anotaciones', { pagina: n })
      setAnots((m) => ({ ...m, [n]: lista }))
    } catch (e) {
      console.error(e)
    }
  }, [])

  /** Vuelve a pintar una sola página y recarga sus anotaciones. */
  const refrescar = useCallback(
    (n: number) => {
      setRevisiones((r) => ({ ...r, [n]: (r[n] ?? 0) + 1 }))
      void cargarAnotaciones(n)
    },
    [cargarAnotaciones],
  )

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
      // Con una herramienta de edición el dedo dibuja o selecciona: no se mueve ni se hace zoom.
      if (editandoRef.current && herramientaRef.current !== 'mover') {
        toque = null
        return
      }
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
        setRutaActual(info.ruta)
        setNombreActual(info.nombre)
        setEditando(false)
        setSucio(false)
        setHechas(0)
        setPreview(null)
        setSeleccion(null)
        setAnots({})
        setRevisiones({})
        setRevisionTotal(0)
        avisoVisto.current = false
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

  const puedeCambiar = () => !sucio || confirm('Hay cambios sin guardar. ¿Abrir otro PDF y perderlos?')

  // En Android el selector lo abre la propia app: así el permiso queda guardado y el archivo se puede reabrir otro día.
  const abrir = () => {
    if (!puedeCambiar()) return
    const puente = window.DakartaAndroid
    if (puente?.elegir) {
      puente.elegir()
      return
    }
    cargar(invoke<Info | null>('elegir_pdf'))
  }

  const abrirReciente = (r: Reciente) => {
    if (!puedeCambiar()) return
    cargar(invoke<Info>('abrir_ruta', { ruta: r.ruta }), r)
  }

  const entrarEditar = () => {
    setEditando(true)
    setHerramienta('mover')
    setSeleccion(null)
    setAyudaManual(false)
    let oculto = false
    try {
      oculto = localStorage.getItem('avisoEdicion') === '1'
    } catch {
      // sin almacenamiento: el aviso se muestra siempre
    }
    if (!oculto && !avisoVisto.current) setAvisoEdicion(true)
  }

  const cerrarAviso = () => {
    if (!ayudaManual) {
      if (recordarAviso) {
        try {
          localStorage.setItem('avisoEdicion', '1')
        } catch {
          // sin almacenamiento: se vuelve a mostrar la próxima vez
        }
      }
      avisoVisto.current = true
    }
    setAvisoEdicion(false)
    setAyudaManual(false)
  }

  const abrirAyuda = () => {
    setAyudaManual(true)
    setAvisoEdicion(true)
  }

  const salirEditar = () => {
    setEditando(false)
    setHerramienta('mover')
    setPreview(null)
    setSeleccion(null)
  }

  /** Elegir una herramienta; volver a tocarla la suelta y deja mover anotaciones. */
  const elegir = (h: Herramienta) => {
    setSeleccion(null)
    setHerramienta((actual) => (actual === h ? 'mover' : h))
  }

  const deshacer = async () => {
    try {
      const pagina = await invoke<number | null>('deshacer')
      if (pagina === null) return
      refrescar(pagina)
      const quedan = Math.max(0, hechas - 1)
      setHechas(quedan)
      setSucio(quedan > 0)
    } catch (e) {
      fallo(e)
    }
  }

  const deshacerTodo = async () => {
    if (!confirm('¿Quitar todos los cambios de esta sesión? Lo que ya guardaste no se toca.')) return
    try {
      const hecho = await invoke<boolean>('deshacer_todo')
      if (!hecho) return
      setRevisionTotal((r) => r + 1)
      setAnots({})
      setSeleccion(null)
      setHechas(0)
      setSucio(false)
    } catch (e) {
      fallo(e)
    }
  }

  const ponerTexto = async () => {
    const p = textoPedido
    const texto = textoEntrada.trim()
    if (!p || !texto) return
    const [rojo, verde, azul] = rgb(p.modo === 'nota' ? colorNota : colorTexto)
    try {
      if (p.modo === 'nota') {
        await invoke('anotar_nota', { pagina: p.n, x: p.x, y: p.y, contenido: texto, rojo, verde, azul, tamano: tamanoNota })
      } else {
        await invoke('anotar_caja', { pagina: p.n, x: p.x, y: p.y, contenido: texto, rojo, verde, azul, tamano: tamanoTexto })
      }
      setTextoPedido(null)
      setTextoEntrada('')
      setHerramienta('mover')
      refrescar(p.n)
      setSucio(true)
      setHechas((h) => h + 1)
    } catch (e) {
      fallo(e)
    }
  }

  const elegirSello = (texto: string, color: string) => {
    setSelloPendiente({ texto })
    setColorSello(color)
    setSelloAbierto(false)
    setHerramienta('sello')
  }

  const colocarSello = async (n: number, fx: number, fy: number) => {
    const s = selloPendiente
    setSelloPendiente(null)
    setHerramienta('mover')
    if (!s) return
    const [rojo, verde, azul] = rgb(colorSello)
    try {
      await invoke('anotar_sello', { pagina: n, x: fx, y: fy, texto: s.texto, rojo, verde, azul, tamano: tamanoSello })
      refrescar(n)
      setSucio(true)
      setHechas((h) => h + 1)
    } catch (e) {
      fallo(e)
    }
  }

  const abrirFormulario = async () => {
    setSeleccion(null)
    setHerramienta('formulario')
    try {
      const lista = await invoke<Campo[]>('campos')
      setCampos(lista)
      if (!lista.length) setExito('Este PDF no tiene formularios.')
    } catch (e) {
      fallo(e)
    }
  }

  const ponerValorCampo = useCallback(
    async (c: Campo, valor: string) => {
      try {
        await invoke('poner_campo', { pagina: c.pagina, xref: c.xref, valor })
        setCampos(await invoke<Campo[]>('campos'))
        refrescar(c.pagina)
        setSucio(true)
        setHechas((h) => h + 1)
      } catch (e) {
        fallo(e)
      }
    },
    [refrescar, fallo],
  )

  const tocarCampo = useCallback(
    (c: Campo) => {
      if (c.readonly) {
        setError('Este campo está protegido y no se puede cambiar.')
        return
      }
      if (/button|checkbox|radio/i.test(c.tipo)) {
        void ponerValorCampo(c, c.valor && c.valor !== 'Off' ? 'Off' : 'Yes')
        return
      }
      setCampoPedido({ campo: c, valor: c.valor })
    },
    [ponerValorCampo],
  )

  const ponerCampo = async () => {
    const p = campoPedido
    setCampoPedido(null)
    if (p) await ponerValorCampo(p.campo, p.valor)
  }

  const seleccionarAnotacion = useCallback((a: Anotacion) => setSeleccion(a), [])

  const moverAnotacion = useCallback(
    (a: Anotacion, dx: number, dy: number) => {
      // Se mueve en pantalla al instante; el motor confirma y repinta esa página.
      setAnots((m) => ({
        ...m,
        [a.pagina]: (m[a.pagina] ?? []).map((x) =>
          x.xref === a.xref ? { ...x, x: clamp01(x.x + dx), y: clamp01(x.y + dy) } : x,
        ),
      }))
      invoke('mover_anotacion', { pagina: a.pagina, xref: a.xref, dx, dy })
        .then(() => {
          refrescar(a.pagina)
          setSucio(true)
          setHechas((h) => h + 1)
        })
        .catch((e) => {
          fallo(e)
          refrescar(a.pagina)
        })
    },
    [refrescar, fallo],
  )

  const aplicarAnotacion = useCallback(
    async (a: Anotacion, cambios: { contenido?: string; color?: string; tamano?: number }) => {
      const [rojo, verde, azul] = rgb(cambios.color ?? hex(a.rojo, a.verde, a.azul))
      const args: Record<string, unknown> = { pagina: a.pagina, xref: a.xref, rojo, verde, azul }
      if (cambios.contenido !== undefined) args.contenido = cambios.contenido
      if (cambios.tamano !== undefined) args.tamano = cambios.tamano
      try {
        await invoke('editar_anotacion', args)
        setSeleccion(null)
        refrescar(a.pagina)
        setSucio(true)
        setHechas((h) => h + 1)
      } catch (e) {
        fallo(e)
      }
    },
    [refrescar, fallo],
  )

  const borrarAnotacion = useCallback(
    async (a: Anotacion) => {
      try {
        await invoke('borrar_anotacion', { pagina: a.pagina, xref: a.xref })
        setSeleccion(null)
        refrescar(a.pagina)
        setSucio(true)
        setHechas((h) => h + 1)
      } catch (e) {
        fallo(e)
      }
    },
    [refrescar, fallo],
  )

  const abrirFirma = () => {
    setSeleccion(null)
    setPadTrazos([])
    setPadAbierto(true)
  }

  const ponerFirma = () => {
    if (!padTrazos.length) return
    setFirmaTrazos(padTrazos.map((t) => t.map(([x, y]) => [x, y] as [number, number])))
    setPadAbierto(false)
    setPadTrazos([])
    setHerramienta('firma')
  }

  const colocarFirma = async (n: number, fx: number, fy: number) => {
    const trazos = firmaTrazos
    setFirmaTrazos(null)
    setHerramienta('mover')
    if (!trazos) return
    const ajustados = trazos.map((t) =>
      t.map(([x, y]) => [clamp01(fx + (x - 0.5) * 0.3), clamp01(fy + (y - 0.5) * 0.1)] as [number, number]),
    )
    const [rojo, verde, azul] = rgb(colorLapiz)
    try {
      await invoke('anotar_tinta', { pagina: n, rojo, verde, azul, grosor, trazos: ajustados })
      refrescar(n)
      setSucio(true)
      setHechas((h) => h + 1)
    } catch (e) {
      fallo(e)
    }
  }

  const guardarCopia = async () => {
    if (!doc || guardando) return
    const base = (nombreActual || doc.info.nombre).replace(/\.pdf$/i, '')
    const nombre = `${base || 'Documento'} (anotado).pdf`
    setGuardando(true)
    try {
      const puente = window.DakartaAndroid
      let destino: string | null
      if (puente?.guardar && puente.copiar) {
        destino = await esperarDestino(nombre)
        if (!destino) return
        const cache = await invoke<string>('guardar_en_cache')
        if (!puente.copiar(destino, cache)) throw new Error('No se pudo copiar la copia.')
        puente.persistir(destino)
      } else {
        destino = await invoke<string | null>('guardar_copia', { nombre })
        if (!destino) return
      }
      const guardada = destino
      const nombreDestino = puente?.nombre(guardada) || nombre
      setRutaActual(guardada)
      setNombreActual(nombreDestino)
      setRecientes((lista) => guardarReciente(lista, { ruta: guardada, nombre: nombreDestino }))
      setSucio(false)
      setHechas(0)
      setExito('Copia guardada')
    } catch (e) {
      console.error(e)
      setError('No se pudo guardar la copia. Inténtalo otra vez.')
    } finally {
      setGuardando(false)
    }
  }

  useEffect(() => {
    if (!exito) return
    const t = setTimeout(() => setExito(''), 3000)
    return () => clearTimeout(t)
  }, [exito])

  // Al editar, las anotaciones de las páginas que se ven (y sus vecinas) se cargan para poder tocarlas.
  useEffect(() => {
    if (!editando || !doc) return
    const [a, b] = visibles
    for (let n = Math.max(0, a - 1); n <= Math.min(doc.info.paginas - 1, b + 1); n++) {
      if (!(n in anots)) void cargarAnotaciones(n)
    }
  }, [editando, doc, visibles, anots, cargarAnotaciones])

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
      if (sucio) return !confirm('Hay cambios sin guardar. ¿Salir de Dakarta y perderlos?')
      return false
    }
    return () => {
      delete window.DakartaAtras
    }
  }, [verIndice, verBuscar, cerrarBuscar, sucio])

  // Windows: avisar antes de cerrar la ventana si hay cambios sin guardar.
  useEffect(() => {
    if (window.DakartaAndroid || !('__TAURI_INTERNALS__' in window)) return
    let des = () => {}
    import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) =>
        getCurrentWindow()
          .onCloseRequested((e) => {
            if (sucio && !confirm('Hay cambios sin guardar. ¿Cerrar Dakarta y perderlos?')) e.preventDefault()
          })
          .then((u) => {
            des = u
          }),
      )
      .catch(() => {})
    return () => des()
  }, [sucio])

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
              onClick={() => {
                if (sucio && !confirm('Hay cambios sin guardar. Se compartirá el archivo original, sin los cambios. ¿Continuar?')) return
                setVerIndice(false)
                // Android: menú de compartir del sistema. Windows: la carpeta con el PDF seleccionado.
                if (window.DakartaAndroid?.compartir) {
                  window.DakartaAndroid.compartir(rutaActual, nombreActual)
                } else {
                  invoke('mostrar_archivo', { ruta: rutaActual }).catch((e) => {
                    console.error(e)
                    setError('No se pudo abrir la carpeta del archivo.')
                  })
                }
              }}
              aria-label="Compartir"
              title="Compartir"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M8.8 10.6 15.2 6.4M8.8 13.4l6.4 4.2" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
                <circle cx="18" cy="5" r="3.2" fill="currentColor" />
                <circle cx="6" cy="12" r="3.2" fill="currentColor" />
                <circle cx="18" cy="19" r="3.2" fill="currentColor" />
              </svg>
            </button>
            <button
              onClick={() => (verBuscar ? cerrarBuscar() : setVerBuscar(true))}
              aria-pressed={verBuscar}
              aria-label="Buscar en el documento"
              title="Buscar en el documento"
            >
              🔍
            </button>
            <button
              onClick={() => (editando ? salirEditar() : entrarEditar())}
              aria-pressed={editando}
              aria-label="Editar PDF"
              title="Editar PDF"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M4 20h4L19 9l-4-4L4 16v4z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
              </svg>
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

      {editando && doc && (
        <div className="editar">
          <div className="fila-herramientas">
            <button onClick={deshacer} disabled={!hechas} aria-label="Deshacer" title="Deshacer">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M9 14 4 9l5-5" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
                <path d="M4 9h9.5a5.5 5.5 0 0 1 0 11H11" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" />
              </svg>
            </button>
            <button onClick={deshacerTodo} disabled={!hechas} aria-label="Deshacer todo" title="Deshacer todo">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="2" />
                <path d="M16.2 8.2a5.6 5.6 0 1 0 1.6 4.6" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
                <path d="M16.5 4.8v3.6h-3.6" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
            <button aria-pressed={herramienta === 'nota'} onClick={() => elegir('nota')}>
              Nota
            </button>
            <button aria-pressed={herramienta === 'texto'} onClick={() => elegir('texto')}>
              Texto
            </button>
            <button aria-pressed={herramienta === 'lapiz'} onClick={() => elegir('lapiz')}>
              Lápiz
            </button>
            <button aria-pressed={herramienta === 'resaltar'} onClick={() => elegir('resaltar')}>
              Resaltar
            </button>
            <button aria-pressed={herramienta === 'subrayar'} onClick={() => elegir('subrayar')}>
              Subrayar
            </button>
            <button aria-pressed={herramienta === 'tachar'} onClick={() => elegir('tachar')}>
              Tachar
            </button>
            <button aria-pressed={herramienta === 'firma'} onClick={abrirFirma}>
              Firma
            </button>
            <button
              aria-pressed={herramienta === 'sello'}
              onClick={() => {
                setSeleccion(null)
                setSelloAbierto(true)
              }}
            >
              Sello
            </button>
            <button aria-pressed={herramienta === 'formulario'} onClick={abrirFormulario}>
              Formulario
            </button>
          </div>
          {herramienta !== 'mover' && herramienta !== 'formulario' && (
            <div className="fila-paleta">
              {(herramienta === 'lapiz' || herramienta === 'firma') && (
                <Paleta colores={COLORES_FUERTES} valor={colorLapiz} onCambio={setColorLapiz} />
              )}
              {(herramienta === 'subrayar' || herramienta === 'tachar' || herramienta === 'texto') && (
                <Paleta colores={COLORES_FUERTES} valor={colorTexto} onCambio={setColorTexto} />
              )}
              {herramienta === 'resaltar' && (
                <Paleta colores={COLORES_RESALTADO} valor={colorResaltado} onCambio={setColorResaltado} />
              )}
              {herramienta === 'nota' && <Paleta colores={COLORES_NOTA} valor={colorNota} onCambio={setColorNota} />}
              {herramienta === 'sello' && <Paleta colores={COLORES_SELLO} valor={colorSello} onCambio={setColorSello} />}
              {(herramienta === 'lapiz' || herramienta === 'firma' || herramienta === 'subrayar' || herramienta === 'tachar') && (
                <Tamanos opciones={GROSORES} valor={grosor} onCambio={setGrosor} />
              )}
              {herramienta === 'texto' && <Tamanos opciones={TAMANOS_TEXTO} valor={tamanoTexto} onCambio={setTamanoTexto} />}
              {herramienta === 'sello' && <Tamanos opciones={TAMANOS_SELLO} valor={tamanoSello} onCambio={setTamanoSello} />}
              {herramienta === 'nota' && <Tamanos opciones={TAMANOS_NOTA} valor={tamanoNota} onCambio={setTamanoNota} />}
              {(herramienta === 'nota' || herramienta === 'texto' || herramienta === 'firma' || herramienta === 'sello') && (
                <span className="pista">Toca la página donde lo quieres</span>
              )}
            </div>
          )}
          {seleccion && herramienta === 'mover' && (
            <PanelAnotacion
              key={seleccion.xref}
              a={seleccion}
              onAplicar={(cambios) => void aplicarAnotacion(seleccion, cambios)}
              onBorrar={() => void borrarAnotacion(seleccion)}
              onCerrar={() => setSeleccion(null)}
            />
          )}
          <div className="fila-acciones">
            <button onClick={guardarCopia} disabled={guardando}>
              {guardando ? 'Guardando…' : 'Guardar copia'}
            </button>
            <button onClick={salirEditar}>Listo</button>
            <button onClick={abrirAyuda} title="¿Qué puedo editar?">
              ⓘ ¿Qué puedo editar?
            </button>
            {sucio && <span className="sin-guardar">Sin guardar</span>}
          </div>
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
                      <button onClick={() => abrirReciente(r)} disabled={abriendo} title={r.nombre}>
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
          onClick={() => {
            if (verIndice) setVerIndice(false)
          }}
          style={editando && herramienta !== 'mover' && herramienta !== 'formulario' ? { touchAction: 'none' } : undefined}
          onScroll={() => {
            medir()
            clearTimeout(relojVecinas.current)
            if (!reloj.current) reloj.current = window.setTimeout(seguir, 100)
          }}
        >
          <div
            className="contenido"
            ref={contenido}
            onPointerDown={(e) => {
              if (!editando || herramienta === 'mover' || herramienta === 'formulario' || !doc) return
              const page = (e.target as HTMLElement).closest('.pagina') as HTMLElement | null
              if (!page) return
              const n = Number(page.dataset.n)
              if (!Number.isInteger(n)) return
              e.preventDefault()
              e.currentTarget.setPointerCapture(e.pointerId)
              const r = page.getBoundingClientRect()
              const fx = clamp01((e.clientX - r.left) / r.width)
              const fy = clamp01((e.clientY - r.top) / r.height)
              const modo =
                herramienta === 'lapiz'
                  ? 'tinta'
                  : herramienta === 'nota' || herramienta === 'texto' || herramienta === 'firma' || herramienta === 'sello'
                    ? 'tap'
                    : 'texto'
              gesto.current = { n, page, modo, tipo: herramienta, puntos: [[fx, fy]] }
              if (modo !== 'tap') {
                setPreview({
                  left: page.offsetLeft,
                  top: page.offsetTop,
                  width: page.offsetWidth,
                  height: page.offsetHeight,
                  modo,
                  x0: fx,
                  y0: fy,
                  x1: fx,
                  y1: fy,
                  puntos: [[fx, fy]],
                })
              }
            }}
            onPointerMove={(e) => {
              const g = gesto.current
              if (!g) return
              e.preventDefault()
              const r = g.page.getBoundingClientRect()
              const fx = clamp01((e.clientX - r.left) / r.width)
              const fy = clamp01((e.clientY - r.top) / r.height)
              if (g.modo === 'tinta') {
                g.puntos.push([fx, fy])
                const puntos = [...g.puntos]
                setPreview((p) => (p ? { ...p, puntos, x1: fx, y1: fy } : p))
              } else {
                setPreview((p) => (p ? { ...p, x1: fx, y1: fy } : p))
              }
            }}
            onPointerUp={(e) => {
              const g = gesto.current
              gesto.current = null
              setPreview(null)
              if (!g) return
              const r = g.page.getBoundingClientRect()
              const fx = clamp01((e.clientX - r.left) / r.width)
              const fy = clamp01((e.clientY - r.top) / r.height)
              const listo = () => {
                refrescar(g.n)
                setSucio(true)
                setHechas((h) => h + 1)
              }
              if (g.modo === 'tap') {
                if (g.tipo === 'firma') {
                  colocarFirma(g.n, fx, fy)
                } else if (g.tipo === 'sello') {
                  colocarSello(g.n, fx, fy)
                } else {
                  setTextoPedido({ modo: g.tipo === 'nota' ? 'nota' : 'texto', n: g.n, x: g.puntos[0][0], y: g.puntos[0][1] })
                  setTextoEntrada('')
                }
                return
              }
              if (g.modo === 'tinta') {
                if (g.puntos.length < 2) return
                const [rojo, verde, azul] = rgb(colorLapiz)
                invoke('anotar_tinta', { pagina: g.n, rojo, verde, azul, grosor, trazos: [g.puntos] }).then(listo).catch(fallo)
              } else {
                const [rojo, verde, azul] = rgb(g.tipo === 'resaltar' ? colorResaltado : colorTexto)
                invoke('anotar_texto', {
                  pagina: g.n,
                  tipo: g.tipo,
                  rojo,
                  verde,
                  azul,
                  grosor,
                  x0: g.puntos[0][0],
                  y0: g.puntos[0][1],
                  x1: fx,
                  y1: fy,
                })
                  .then(listo)
                  .catch(fallo)
              }
            }}
            onPointerCancel={() => {
              gesto.current = null
              setPreview(null)
            }}
          >
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
                revisiones={revisiones}
                revisionTotal={revisionTotal}
                campos={herramienta === 'formulario' ? campos : undefined}
                onCampo={tocarCampo}
                anots={editando && herramienta === 'mover' ? anots : undefined}
                seleccion={seleccion?.xref ?? null}
                onSeleccionar={seleccionarAnotacion}
                onMover={moverAnotacion}
              />
            )}
            {preview && (
              <div
                className="preview"
                style={{ left: preview.left, top: preview.top, width: preview.width, height: preview.height }}
              >
                {preview.modo === 'texto' ? (
                  <div
                    className="preview-texto"
                    style={{
                      left: `${Math.min(preview.x0, preview.x1) * 100}%`,
                      top: `${Math.min(preview.y0, preview.y1) * 100}%`,
                      width: `${Math.abs(preview.x1 - preview.x0) * 100}%`,
                      height: `${Math.abs(preview.y1 - preview.y0) * 100}%`,
                    }}
                  />
                ) : (
                  <svg viewBox="0 0 100 100" preserveAspectRatio="none">
                    <polyline
                      points={preview.puntos.map(([x, y]) => `${x * 100},${y * 100}`).join(' ')}
                      fill="none"
                      stroke={colorLapiz}
                      strokeWidth={Math.max(1, grosor)}
                      vectorEffect="non-scaling-stroke"
                    />
                  </svg>
                )}
              </div>
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
                      <button onClick={() => abrirReciente(r)} disabled={abriendo} title={r.nombre}>
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
        {exito && (
          <div className="aviso-ok" role="status">
            {exito}
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

      {avisoEdicion && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-edicion">
          <div className="tarjeta-aviso">
            <h2 id="titulo-edicion">Edición básica de PDF</h2>
            <p>
              Puedes resaltar, subrayar, tachar, dibujar a mano, poner notas y cuadros de texto, firmar dibujando, rellenar
              formularios y poner sellos. Toca una anotación para cambiar su texto, color o tamaño, o arrástrala para moverla.
              No se puede cambiar el texto original, las imágenes ni el diseño de la página: el PDF está hecho para leerse, no
              para editarse. Al guardar se crea una copia; el archivo original no se toca.
            </p>
            <p>Si sales sin guardar, se pierden los cambios.</p>
            {!ayudaManual && (
              <label>
                <input type="checkbox" checked={recordarAviso} onChange={(e) => setRecordarAviso(e.currentTarget.checked)} />
                Entendido, no volver a mostrar
              </label>
            )}
            <button className="grande" onClick={cerrarAviso}>
              Entendido
            </button>
          </div>
        </div>
      )}

      {textoPedido && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-texto">
          <div className="tarjeta-aviso">
            <h2 id="titulo-texto">{textoPedido.modo === 'nota' ? 'Nueva nota' : 'Nuevo texto'}</h2>
            <textarea
              autoFocus
              rows={4}
              value={textoEntrada}
              onChange={(e) => setTextoEntrada(e.currentTarget.value)}
              placeholder={textoPedido.modo === 'nota' ? 'Escribe la nota' : 'Escribe el texto'}
            />
            <div className="fila-botones">
              <button onClick={() => setTextoPedido(null)}>Cancelar</button>
              <button className="grande" onClick={ponerTexto} disabled={!textoEntrada.trim()}>
                {textoPedido.modo === 'nota' ? 'Poner nota' : 'Poner texto'}
              </button>
            </div>
          </div>
        </div>
      )}

      {padAbierto && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-firma">
          <div className="tarjeta-aviso">
            <h2 id="titulo-firma">Firma con el dedo</h2>
            <div
              className="pad"
              onPointerDown={(e) => {
                e.preventDefault()
                e.currentTarget.setPointerCapture(e.pointerId)
                const r = e.currentTarget.getBoundingClientRect()
                const x = clamp01((e.clientX - r.left) / r.width)
                const y = clamp01((e.clientY - r.top) / r.height)
                padActual.current = [[x, y]]
                setPadTrazos((t) => [...t, [[x, y]]])
              }}
              onPointerMove={(e) => {
                const actual = padActual.current
                if (!actual) return
                e.preventDefault()
                const r = e.currentTarget.getBoundingClientRect()
                actual.push([clamp01((e.clientX - r.left) / r.width), clamp01((e.clientY - r.top) / r.height)])
                const copia = [...actual]
                setPadTrazos((t) => {
                  const nuevo = [...t]
                  nuevo[nuevo.length - 1] = copia
                  return nuevo
                })
              }}
              onPointerUp={() => {
                padActual.current = null
              }}
              onPointerCancel={() => {
                padActual.current = null
              }}
            >
              <svg viewBox="0 0 100 100" preserveAspectRatio="none">
                {padTrazos.map((t, i) => (
                  <polyline
                    key={i}
                    points={t.map(([x, y]) => `${x * 100},${y * 100}`).join(' ')}
                    fill="none"
                    stroke={colorLapiz}
                    strokeWidth="1.5"
                    vectorEffect="non-scaling-stroke"
                  />
                ))}
              </svg>
            </div>
            <p className="nota-ayuda">Después de ponerla, toca la página donde quieres la firma.</p>
            <div className="fila-botones">
              <button onClick={() => setPadTrazos([])} disabled={!padTrazos.length}>
                Borrar
              </button>
              <button
                onClick={() => {
                  setPadAbierto(false)
                  setPadTrazos([])
                }}
              >
                Cancelar
              </button>
              <button className="grande" onClick={ponerFirma} disabled={!padTrazos.length}>
                Poner firma
              </button>
            </div>
          </div>
        </div>
      )}

      {selloAbierto && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-sello">
          <div className="tarjeta-aviso">
            <h2 id="titulo-sello">Elige el sello</h2>
            <div className="sellos">
              {SELLOS.map(([texto, color]) => (
                <button key={texto} style={{ color, borderColor: color }} onClick={() => elegirSello(texto, color)}>
                  {texto}
                </button>
              ))}
            </div>
            <div className="fila-botones">
              <button onClick={() => setSelloAbierto(false)}>Cancelar</button>
            </div>
          </div>
        </div>
      )}

      {campoPedido && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-campo">
          <div className="tarjeta-aviso">
            <h2 id="titulo-campo">{campoPedido.campo.nombre || 'Campo del formulario'}</h2>
            <textarea
              autoFocus
              value={campoPedido.valor}
              onChange={(e) => setCampoPedido({ ...campoPedido, valor: e.currentTarget.value })}
              aria-label={campoPedido.campo.nombre || 'Valor del campo'}
            />
            <div className="fila-botones">
              <button onClick={() => setCampoPedido(null)}>Cancelar</button>
              <button className="grande" onClick={ponerCampo}>
                Guardar
              </button>
            </div>
          </div>
        </div>
      )}
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
  revisiones,
  revisionTotal,
  campos,
  onCampo,
  anots,
  seleccion,
  onSeleccionar,
  onMover,
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
  revisiones: Record<number, number>
  revisionTotal: number
  campos?: Campo[]
  onCampo: (c: Campo) => void
  anots?: Record<number, Anotacion[]>
  seleccion: number | null
  onSeleccionar: (a: Anotacion) => void
  onMover: (a: Anotacion, dx: number, dy: number) => void
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
  const porCampo = useMemo(() => {
    const m = new Map<number, Campo[]>()
    campos?.forEach((c) => {
      const lista = m.get(c.pagina)
      if (lista) lista.push(c)
      else m.set(c.pagina, [c])
    })
    return m
  }, [campos])
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
        revision={revisiones[n] ?? 0}
        revisionTotal={revisionTotal}
        campos={porCampo.get(n)}
        onCampo={onCampo}
        anots={anots?.[n]}
        seleccion={seleccion}
        onSeleccionar={onSeleccionar}
        onMover={onMover}
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
  revision,
  revisionTotal,
  campos,
  onCampo,
  anots,
  seleccion,
  onSeleccionar,
  onMover,
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
  revision: number
  revisionTotal: number
  campos?: Campo[]
  onCampo: (c: Campo) => void
  anots?: Anotacion[]
  seleccion: number | null
  onSeleccionar: (a: Anotacion) => void
  onMover: (a: Anotacion, dx: number, dy: number) => void
}) {
  const url = (w: number) => `${convertFileSrc(String(n), 'pagina')}?w=${w}&d=${id}&v=${revisionTotal}-${revision}`
  return (
    <div className="pagina" data-n={n} style={{ width: ancho, height: alto }}>
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
      {recorte && <Detalle r={recorte} id={id} revision={revision} revisionTotal={revisionTotal} />}
      {anots?.map((a) => (
        <CajaAnotacion
          key={a.xref}
          a={a}
          ancho={ancho}
          alto={alto}
          seleccionada={seleccion === a.xref}
          onSeleccionar={onSeleccionar}
          onMover={onMover}
        />
      ))}
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
      {campos?.map((c) => (
        <button
          key={c.xref}
          className={c.readonly ? 'campo protegido' : 'campo'}
          style={{
            left: `${c.x * 100}%`,
            top: `${c.y * 100}%`,
            width: `${c.w * 100}%`,
            height: `${c.h * 100}%`,
          }}
          title={c.nombre || 'Campo del formulario'}
          onClick={() => onCampo(c)}
        >
          {c.valor}
        </button>
      ))}
    </div>
  )
})

/**
 * El recorte con el detalle exacto. Se posiciona en % de la página: al cambiar el zoom sigue en su sitio, solo estirado.
 * El anterior se queda visible hasta que llega el nuevo; encima, el nuevo es transparente mientras no ha cargado.
 */
function Detalle({ r, id, revision, revisionTotal }: { r: Recorte; id: number; revision: number; revisionTotal: number }) {
  const direccion = (c: Recorte) =>
    `${convertFileSrc(String(c.n), 'pagina')}?w=${c.W}&h=${c.H}&x=${c.x}&y=${c.y}&cw=${c.cw}&ch=${c.ch}&d=${id}&v=${revisionTotal}-${revision}`
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

function Paleta({
  colores,
  valor,
  onCambio,
}: {
  colores: ReadonlyArray<readonly [string, string]>
  valor: string
  onCambio: (c: string) => void
}) {
  return (
    <span className="colores">
      {colores.map(([c, n]) => (
        <button
          key={c}
          type="button"
          className="color"
          style={{ background: c }}
          aria-label={n}
          title={n}
          aria-pressed={valor.toLowerCase() === c.toLowerCase()}
          onClick={() => onCambio(c)}
        />
      ))}
      <label className="color otro" title="Otro color">
        <input type="color" value={valor} onChange={(e) => onCambio(e.currentTarget.value)} aria-label="Otro color" />
      </label>
    </span>
  )
}

function Tamanos({
  opciones,
  valor,
  onCambio,
}: {
  opciones: ReadonlyArray<readonly [number, string]>
  valor: number
  onCambio: (n: number) => void
}) {
  return (
    <span className="grosores">
      {opciones.map(([v, n]) => (
        <button key={v} type="button" aria-pressed={valor === v} onClick={() => onCambio(v)}>
          {n}
        </button>
      ))}
    </span>
  )
}

const TITULOS_ANOTACION: Record<Anotacion['tipo'], string> = {
  nota: 'Nota',
  texto: 'Texto o sello',
  tinta: 'Trazo o firma',
  resaltar: 'Resaltado',
  subrayar: 'Subrayado',
  tachar: 'Tachado',
}

/** Caja invisible sobre una anotación: tocarla la elige y arrastrarla la mueve. */
function CajaAnotacion({
  a,
  ancho,
  alto,
  seleccionada,
  onSeleccionar,
  onMover,
}: {
  a: Anotacion
  ancho: number
  alto: number
  seleccionada: boolean
  onSeleccionar: (a: Anotacion) => void
  onMover: (a: Anotacion, dx: number, dy: number) => void
}) {
  const inicio = useRef<{ x: number; y: number } | null>(null)
  const [desp, setDesp] = useState<{ x: number; y: number } | null>(null)
  return (
    <div
      className={seleccionada ? 'anot seleccionada' : 'anot'}
      style={{
        left: `${a.x * 100}%`,
        top: `${a.y * 100}%`,
        width: `${a.w * 100}%`,
        height: `${a.h * 100}%`,
        transform: desp ? `translate(${desp.x}px, ${desp.y}px)` : undefined,
      }}
      title={TITULOS_ANOTACION[a.tipo]}
      onPointerDown={(e) => {
        e.stopPropagation()
        e.preventDefault()
        e.currentTarget.setPointerCapture(e.pointerId)
        inicio.current = { x: e.clientX, y: e.clientY }
        setDesp({ x: 0, y: 0 })
      }}
      onPointerMove={(e) => {
        const i = inicio.current
        if (!i) return
        e.preventDefault()
        setDesp({ x: e.clientX - i.x, y: e.clientY - i.y })
      }}
      onPointerUp={(e) => {
        const i = inicio.current
        inicio.current = null
        setDesp(null)
        if (!i) return
        const dx = e.clientX - i.x
        const dy = e.clientY - i.y
        if (Math.hypot(dx, dy) > 5) onMover(a, dx / ancho, dy / alto)
        else onSeleccionar(a)
      }}
      onPointerCancel={() => {
        inicio.current = null
        setDesp(null)
      }}
    />
  )
}

/** Panel para cambiar el texto, el color, el tamaño o borrar la anotación elegida. */
function PanelAnotacion({
  a,
  onAplicar,
  onBorrar,
  onCerrar,
}: {
  a: Anotacion
  onAplicar: (cambios: { contenido?: string; color?: string; tamano?: number }) => void
  onBorrar: () => void
  onCerrar: () => void
}) {
  const [texto, setTexto] = useState(a.contenido)
  const [color, setColor] = useState(hex(a.rojo, a.verde, a.azul))
  const [tamano, setTamano] = useState(a.tamano)
  const conTexto = a.tipo === 'nota' || a.tipo === 'texto'
  const colores =
    a.tipo === 'nota' ? COLORES_NOTA : a.tipo === 'resaltar' ? COLORES_RESALTADO : COLORES_FUERTES
  const tamanos =
    a.tipo === 'nota'
      ? TAMANOS_NOTA
      : a.tipo === 'texto'
        ? TAMANOS_TEXTO
        : a.tipo === 'tinta' || a.tipo === 'subrayar' || a.tipo === 'tachar'
          ? GROSORES
          : null
  return (
    <div className="fila-seleccion">
      <strong>{TITULOS_ANOTACION[a.tipo]}</strong>
      {conTexto && (
        <textarea
          value={texto}
          onChange={(e) => setTexto(e.currentTarget.value)}
          placeholder={a.tipo === 'nota' ? 'Texto de la nota' : 'Texto'}
          aria-label={a.tipo === 'nota' ? 'Texto de la nota' : 'Texto'}
        />
      )}
      <Paleta colores={colores} valor={color} onCambio={setColor} />
      {tamanos && <Tamanos opciones={tamanos} valor={tamano} onCambio={setTamano} />}
      <button
        type="button"
        disabled={conTexto && !texto.trim()}
        onClick={() => onAplicar({ contenido: conTexto ? texto : undefined, color, tamano: tamanos ? tamano : undefined })}
      >
        Aplicar
      </button>
      <button type="button" onClick={onBorrar}>
        Borrar
      </button>
      <button type="button" onClick={onCerrar}>
        Cerrar
      </button>
    </div>
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

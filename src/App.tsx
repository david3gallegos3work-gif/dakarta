import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import logo from '../assets/logo.jpg'
import { AcercaDe } from './AcercaDe.tsx'
import { CuadroClave, type PideClave } from './Clave.tsx'
import { CLAVE_MAL, esErrorClave } from './clave.ts'
import {
  clamp01,
  COLORES_FUERTES,
  COLORES_RESALTADO,
  COLORES_SELLO,
  GROSORES,
  hex,
  rgb,
  SELLOS,
  TAMANOS_SELLO,
  TAMANOS_TEXTO,
} from './opciones.ts'
import { base64De, jpegDe, miniaturaDe, nombreDeFotos, prepararFoto, type Diseno, type Recorte as RecorteFoto } from './fotos.ts'
import { Organizar, type Modo } from './Organizar.tsx'
import { ABIERTO, type Fuente } from './organizar.ts'
import { Paginas } from './Paginas.tsx'
import { Recortar } from './Recortar.tsx'
import { Indice, Paleta, PanelAnotacion, Tamanos } from './Paneles.tsx'
import {
  guardarReciente,
  leerRecientes,
  marcarAbriendo,
  nombreLegible,
  olvidarReciente,
  seCerroAbriendo,
  type Reciente,
} from './recientes.ts'
import type {
  Ancla,
  Anotacion,
  Busqueda,
  Campo,
  Coincidencia,
  Doc,
  Enlace,
  Herramienta,
  Hoja,
  Info,
  Palabra,
  Pregunta,
  Preview,
  Rango,
} from './tipos.ts'
import { buscarActualizacion, type Actualizacion } from './version.ts'
import { ANCHO_ENTERA_MAX, imanZoom, limitarZoom, recorte, type Recorte } from './zoom.ts'


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
/** Fija (no se crea en cada dibujo): así las hojas no se vuelven a dibujar por recibir una función nueva. */
const quitarMarcaAbriendo = () => marcarAbriendo(false)

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
  const [colorSello, setColorSello] = useState(COLORES_SELLO[0][0])
  const [grosor, setGrosor] = useState(3)
  const [tamanoTexto, setTamanoTexto] = useState(12)
  const [tamanoSello, setTamanoSello] = useState(18)
  /** La versión de cada página: al dibujar solo se vuelve a pedir esa, no todas. */
  const [revisiones, setRevisiones] = useState<Record<number, number>>({})
  /** Para deshacer todo, que afecta a varias páginas. */
  const [revisionTotal, setRevisionTotal] = useState(0)
  const [anots, setAnots] = useState<Record<number, Anotacion[]>>({})
  const [seleccion, setSeleccion] = useState<Anotacion | null>(null)
  const [seleccionTexto, setSeleccionTexto] = useState(false)
  const [palabras, setPalabras] = useState<Record<number, Palabra[]>>({})
  const [enlaces, setEnlaces] = useState<Record<number, Enlace[]>>({})
  const [campoPedido, setCampoPedido] = useState<{ campo: Campo; valor: string } | null>(null)
  const [sucio, setSucio] = useState(false)
  const [hechas, setHechas] = useState(0)
  const [avisoEdicion, setAvisoEdicion] = useState(false)
  const [pregunta, setPregunta] = useState<Pregunta | null>(null)
  const [pideClave, setPideClave] = useState<PideClave | null>(null)
  const [verAcerca, setVerAcerca] = useState(false)
  // Pantalla de organizar páginas (un PDF) o de unir PDFs, con los PDFs cuyas hojas muestra al abrirse.
  const [organizar, setOrganizar] = useState<{ modo: Modo; fuentes: Fuente[] } | null>(null)
  // Crear PDF desde fotos: cada foto elegida (por su número) y su miniatura.
  // `archivo`: la copia de trabajo de la foto (más liviana que la original).
  const fotos = useRef(new Map<number, { archivo: Blob; miniatura: string; recorte?: RecorteFoto }>())
  const [recortando, setRecortando] = useState<number | null>(null)
  const siguienteFoto = useRef(0)
  const [miniaturas, setMiniaturas] = useState<Record<number, string>>({})
  // Android: el selector devuelve el PDF elegido para unir por el mismo camino que «Abrir con».
  const esperandoAgregado = useRef<((ruta: string | null) => void) | null>(null)
  const salioParaElegir = useRef(false)
  /** Android: "Salir" del cuadro deja pasar el siguiente "atrás" sin volver a preguntar. */
  const saliendo = useRef(false)
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
  /** Último texto seleccionado: al tocar "Copiar" el navegador puede haber quitado la selección. */
  const textoSeleccionado = useRef('')
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
    setError(detalle.startsWith('MuPDF error') ? `No se pudo hacer ese cambio en el PDF. Detalle: ${detalle}` : detalle)
  }, [])

  const cargarAnotaciones = useCallback(async (n: number) => {
    try {
      const lista = await invoke<Anotacion[]>('anotaciones', { pagina: n })
      setAnots((m) => ({ ...m, [n]: lista }))
    } catch (e) {
      console.error(e)
    }
  }, [])

  const cargarPalabras = useCallback(async (n: number) => {
    try {
      const lista = await invoke<Palabra[]>('palabras', { pagina: n })
      setPalabras((m) => ({ ...m, [n]: lista }))
    } catch (e) {
      console.error(e)
    }
  }, [])

  const cargarEnlaces = useCallback(async (n: number) => {
    try {
      const lista = await invoke<Enlace[]>('enlaces', { pagina: n })
      setEnlaces((m) => ({ ...m, [n]: lista }))
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
      // Con una herramienta de edición, un dedo dibuja; dos dedos acercan o alejan para marcar con exactitud.
      if (editandoRef.current && herramientaRef.current !== 'mover' && e.touches.length !== 2) {
        toque = null
        return
      }
      if (editandoRef.current && herramientaRef.current !== 'mover') {
        // Al empezar a hacer zoom se cancela el trazo que hubiera empezado con el primer dedo.
        gesto.current = null
        setPreview(null)
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

  // Se guarda lo último seleccionado: al tocar el botón Copiar la selección puede desaparecer antes.
  useEffect(() => {
    const al = () => {
      const t = window.getSelection()?.toString() ?? ''
      if (t.trim()) textoSeleccionado.current = t
    }
    document.addEventListener('selectionchange', al)
    return () => document.removeEventListener('selectionchange', al)
  }, [])

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
    // Si seguía buscando en un PDF grande, el motor deja de hacerlo y vuelve a pintar páginas.
    invoke('cancelar_busqueda').catch(() => {})
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

  /**
   * Abre un PDF. `pedir` lo pide al motor con la contraseña (`undefined` la primera vez): si el PDF la
   * pide, sale el cuadro para escribirla y se vuelve a pedir con ella.
   */
  async function cargar(pedir: (clave?: string) => Promise<Info | null>, reciente?: Reciente, clave?: string) {
    setAbriendo(true)
    setError('')
    marcarAbriendo(true)
    let abierto = false
    try {
      const info = await pedir(clave)
      if (info) {
        abierto = true
        setDoc((d) => ({ info, id: (d?.id ?? 0) + 1 }))
        setRutaActual(info.ruta)
        setNombreActual(info.nombre)
        setEditando(false)
        setSucio(false)
        setHechas(0)
        setPreview(null)
        setSeleccion(null)
        setAnots({})
        setSeleccionTexto(false)
        setPalabras({})
        setEnlaces({})
        textoSeleccionado.current = ''
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
      if (esErrorClave(e)) {
        const nombre = reciente?.nombre
        setPideClave({
          titulo: 'Este PDF tiene contraseña',
          texto: nombre ? `Escribe la contraseña para abrir «${nombre}».` : 'Escribe la contraseña para abrirlo.',
          mal: String(e) === CLAVE_MAL,
          boton: 'Abrir',
          alListo: (c) => void cargar(pedir, reciente, c),
        })
        return
      }
      console.error(e)
      setError(reciente ? `No se pudo abrir «${reciente.nombre}». Ábrelo otra vez con «Abrir».` : 'No se pudo abrir el archivo. Inténtalo otra vez con «Abrir».')
      if (reciente) setRecientes((lista) => olvidarReciente(lista, reciente.ruta))
    } finally {
      setAbriendo(false)
      // Si abrió, la marca se quita al verse la primera hoja (o al salir de la app): si la app se cerrara
      // mientras se pinta, la marca quedaría puesta y la próxima vez no se reabre solo.
      if (!abierto) marcarAbriendo(false)
    }
  }

  const avisar = (texto: string, esError?: boolean) => (esError ? setError(texto) : setExito(texto))

  /** Con cambios sin guardar, pregunta antes de abrir otro PDF; si no hay, lo abre directo. */
  const siPuedeCambiar = (abrirOtro: () => void) => {
    if (!sucioRef.current) return abrirOtro()
    setPregunta({
      titulo: 'Hay cambios sin guardar',
      texto: 'Si abres otro PDF, se pierden los cambios de este.',
      si: 'Abrir el otro',
      no: 'Seguir aquí',
      alSi: abrirOtro,
    })
  }

  // En Android el selector lo abre la propia app: así el permiso queda guardado y el archivo se puede reabrir otro día.
  // En Android el aviso de cambios sin guardar sale cuando llega el archivo (igual que con "Abrir con").
  const abrir = () => {
    const puente = window.DakartaAndroid
    if (puente?.elegir) {
      puente.elegir()
      return
    }
    siPuedeCambiar(() => {
      invoke<string | null>('elegir_pdf')
        .then((ruta) => {
          if (ruta) void abrirEn(ruta)
        })
        .catch((e) => {
          console.error(e)
          setError('No se pudo abrir el archivo. Inténtalo otra vez con «Abrir».')
        })
    })
  }

  /** Abre el PDF de esa ruta (pidiendo la contraseña si tiene). */
  const abrirEn = (ruta: string, reciente?: Reciente, clave?: string) =>
    cargar((c) => invoke<Info>('abrir_ruta', { ruta, clave: c }), reciente, clave)

  const abrirReciente = (r: Reciente) => {
    siPuedeCambiar(() => abrirEn(r.ruta, r))
  }

  const entrarEditar = () => {
    setEditando(true)
    if (seleccionTexto) window.getSelection()?.removeAllRanges()
    textoSeleccionado.current = ''
    setSeleccionTexto(false)
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
      setSeleccion(null)
      refrescar(pagina)
      const quedan = Math.max(0, hechas - 1)
      setHechas(quedan)
      setSucio(quedan > 0)
    } catch (e) {
      fallo(e)
    }
  }

  const deshacerTodo = () =>
    setPregunta({
      titulo: 'Quitar todos los cambios',
      texto: 'Se quitan todos los cambios de esta sesión. Lo que ya guardaste no se toca.',
      si: 'Quitar todo',
      no: 'Cancelar',
      alSi: () => void quitarTodo(),
    })

  const quitarTodo = async () => {
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
    const [rojo, verde, azul] = rgb(colorTexto)
    try {
      // Nota y Texto ponen texto visible en la hoja; la nota lleva fondo amarillo, como un papelito pegado.
      await invoke('anotar_caja', {
        pagina: p.n,
        x: p.x,
        y: p.y,
        contenido: texto,
        rojo,
        verde,
        azul,
        tamano: tamanoTexto,
        postit: herramienta === 'nota',
      })
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
      if (c.tipo === 'Checkbox') {
        void ponerValorCampo(c, c.valor === c.encendido ? 'Off' : c.encendido)
        return
      }
      // Una opción se marca; ya marcada se queda así (se cambia eligiendo otra del mismo grupo).
      if (c.tipo === 'RadioButton') {
        if (c.valor !== c.encendido) void ponerValorCampo(c, c.encendido)
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

  const copiarSeleccion = async () => {
    const actual = window.getSelection()?.toString() ?? ''
    const texto = (actual.trim() ? actual : textoSeleccionado.current).trim()
    if (!texto) {
      setExito('Primero selecciona el texto que quieres copiar.')
      return
    }
    try {
      if (!navigator.clipboard) throw new Error('sin portapapeles')
      await navigator.clipboard.writeText(texto)
    } catch {
      const area = document.createElement('textarea')
      area.value = texto
      document.body.appendChild(area)
      area.select()
      document.execCommand('copy')
      area.remove()
    }
    setExito('Texto copiado')
  }

  const seleccionarAnotacion = useCallback((a: Anotacion) => setSeleccion(a), [])

  const moverAnotacion = useCallback(
    (a: Anotacion, dx: number, dy: number) => {
      // Al moverla se vuelve a crear con otro número: el panel ya no sirve para ella.
      setSeleccion((s) => (s?.xref === a.xref ? null : s))
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
    async (a: Anotacion, cambios: { contenido?: string; color?: string; tamano?: number; junto?: boolean }) => {
      const [rojo, verde, azul] = rgb(cambios.color ?? hex(a.rojo, a.verde, a.azul))
      const args: Record<string, unknown> = { pagina: a.pagina, xref: a.xref, rojo, verde, azul }
      if (cambios.contenido !== undefined) args.contenido = cambios.contenido
      if (cambios.tamano !== undefined) args.tamano = cambios.tamano
      if (cambios.junto) args.junto = true
      try {
        const nuevoPaso = await invoke<boolean>('editar_anotacion', args)
        // Sigue elegida: así los cambios de color y tamaño se ven y se pueden seguir tocando.
        setSeleccion((s) =>
          s && s.xref === a.xref
            ? { ...s, contenido: cambios.contenido ?? s.contenido, rojo, verde, azul, tamano: cambios.tamano ?? s.tamano }
            : s,
        )
        refrescar(a.pagina)
        setSucio(true)
        if (nuevoPaso) setHechas((h) => h + 1)
      } catch (e) {
        fallo(e)
      }
    },
    [refrescar, fallo],
  )

  const redimensionarAnotacion = useCallback(
    (a: Anotacion, w: number, h: number) => {
      // Se agranda en pantalla al instante; el motor confirma y repinta esa página.
      setAnots((m) => ({
        ...m,
        [a.pagina]: (m[a.pagina] ?? []).map((x) => (x.xref === a.xref ? { ...x, w, h } : x)),
      }))
      setSeleccion((s) => (s && s.xref === a.xref ? { ...s, w, h } : s))
      invoke('redimensionar_anotacion', { pagina: a.pagina, xref: a.xref, x: a.x, y: a.y, w, h })
        .then(() => {
          refrescar(a.pagina)
          setSucio(true)
          setHechas((n) => n + 1)
        })
        .catch((e) => {
          fallo(e)
          refrescar(a.pagina)
        })
    },
    [refrescar, fallo],
  )

  const girarAnotacion = useCallback(
    (a: Anotacion, grados: number) => {
      invoke('girar_anotacion', { pagina: a.pagina, xref: a.xref, grados })
        .then(async () => {
          refrescar(a.pagina)
          setSucio(true)
          setHechas((n) => n + 1)
          // Girada, la caja cambia de forma: sigue elegida con su caja nueva para poder seguir girándola.
          const lista = await invoke<Anotacion[]>('anotaciones', { pagina: a.pagina })
          setAnots((m) => ({ ...m, [a.pagina]: lista }))
          setSeleccion((s) => (s && s.xref === a.xref ? (lista.find((x) => x.xref === a.xref) ?? null) : s))
        })
        .catch((e) => {
          fallo(e)
          refrescar(a.pagina)
        })
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

  /** El PDF abierto como primera fuente de la pantalla de organizar o unir (con sus cambios sin guardar). */
  const fuenteAbierta = (d: Doc): Fuente => ({ archivo: ABIERTO, nombre: nombreActual || d.info.nombre, paginas: d.info.paginas })

  const cerrarOrganizar = useCallback(() => {
    setOrganizar(null)
    esperandoAgregado.current = null
    for (const f of fotos.current.values()) URL.revokeObjectURL(f.miniatura)
    fotos.current.clear()
    setMiniaturas({})
    setRecortando(null)
    // Los PDFs agregados para unir ya no hacen falta: el motor los suelta.
    invoke('cerrar_agregados').catch(() => {})
  }, [])

  /** Unir PDFs: abre en el motor un PDF elegido, pidiendo su contraseña si tiene. Sin él, `null`. */
  const abrirAgregado = async (ruta: string, nombre: string, clave?: string): Promise<Fuente | null> => {
    try {
      const f = await invoke<Fuente>('abrir_agregado', { ruta, clave })
      return { ...f, nombre: nombreLegible(nombre || f.nombre) }
    } catch (e) {
      if (!esErrorClave(e)) throw e
      const legible = nombreLegible(nombre || ruta.split(/[\\/]/).pop() || '')
      return new Promise((listo) =>
        setPideClave({
          titulo: 'Este PDF tiene contraseña',
          texto: legible ? `Escribe la contraseña de «${legible}» para unirlo.` : 'Escribe la contraseña para unirlo.',
          mal: String(e) === CLAVE_MAL,
          boton: 'Unir',
          alListo: (c) => void abrirAgregado(ruta, nombre, c).then(listo, () => listo(null)),
          alCancelar: () => listo(null),
        }),
      )
    }
  }

  /** Unir PDFs: pide uno o más PDFs y los abre en el motor junto al abierto. */
  const agregarPdf = async (): Promise<Fuente[]> => {
    try {
      const puente = window.DakartaAndroid
      let elegidos: { ruta: string; nombre: string }[]
      if (puente?.elegir) {
        const ruta = await new Promise<string | null>((listo) => {
          esperandoAgregado.current = listo
          salioParaElegir.current = false
          puente.elegir!()
        })
        if (!ruta) return []
        let nombre = ''
        try {
          nombre = puente.nombre(ruta) ?? ''
        } catch {
          // sin nombre de Android: queda el que se sacó de la ruta
        }
        elegidos = [{ ruta, nombre }]
      } else {
        elegidos = (await invoke<string[]>('elegir_pdfs')).map((ruta) => ({ ruta, nombre: '' }))
      }
      const lista: Fuente[] = []
      for (const { ruta, nombre } of elegidos) {
        const f = await abrirAgregado(ruta, nombre)
        if (f) lista.push(f)
      }
      return lista
    } catch (e) {
      console.error(e)
      setError('No se pudo abrir ese PDF para unirlo.')
      return []
    }
  }

  /** Crear PDF desde fotos: prepara la miniatura de cada foto elegida; las que no se pueden leer se avisan. */
  const prepararFotos = async (archivos: File[]): Promise<Fuente[]> => {
    const nuevas: Fuente[] = []
    let fallidas = 0
    for (const archivo of archivos) {
      try {
        const { trabajo, miniatura } = await prepararFoto(archivo)
        const id = siguienteFoto.current++
        fotos.current.set(id, { archivo: trabajo, miniatura })
        setMiniaturas((m) => ({ ...m, [id]: miniatura }))
        nuevas.push({ archivo: id, nombre: archivo.name, paginas: 1 })
      } catch (e) {
        console.error(e)
        fallidas++
      }
    }
    if (fallidas) {
      setError(`No se pudo leer ${fallidas === 1 ? 'una de las fotos' : `${fallidas} de las fotos`}. Prueba con fotos JPG o PNG.`)
    }
    return nuevas
  }

  /** Guarda el recorte de una foto y rehace su miniatura. */
  const recortarFoto = async (id: number, recorte: RecorteFoto) => {
    setRecortando(null)
    const foto = fotos.current.get(id)
    if (!foto) return
    try {
      const miniatura = await miniaturaDe(foto.archivo, recorte)
      URL.revokeObjectURL(foto.miniatura)
      fotos.current.set(id, { ...foto, miniatura, recorte })
      setMiniaturas((m) => ({ ...m, [id]: miniatura }))
    } catch (e) {
      console.error(e)
      setError('No se pudo recortar la foto.')
    }
  }

  /** Arma el PDF con las fotos en ese orden, giro y recorte, lo guarda donde se elija y lo abre. */
  const guardarFotos = async (hojas: Hoja[], diseno: Diseno) => {
    if (guardando || !hojas.length) return
    const nombre = nombreDeFotos()
    setGuardando(true)
    const puente = window.DakartaAndroid
    let elegido: string | null = null
    try {
      // Android pregunta dónde guardar antes de preparar las fotos (Windows, al final).
      if (puente?.guardar && puente.copiar) {
        elegido = await esperarDestino(nombre)
        if (!elegido) return
      }
      await invoke('fotos_vaciar')
      for (const [i, h] of hojas.entries()) {
        const foto = fotos.current.get(h.archivo)
        if (!foto) throw new Error('Falta una foto')
        setExito(`Preparando foto ${i + 1} de ${hojas.length}…`)
        const jpg = await jpegDe(foto.archivo, h.giro, diseno, foto.recorte)
        // Android: el IPC solo lleva texto; en base64 pesa la tercera parte que como lista de números.
        await invoke('fotos_agregar', puente ? { datos: await base64De(jpg) } : new Uint8Array(await jpg.arrayBuffer()))
      }
      setExito('')
      let destino: string | null
      if (puente?.copiar && elegido) {
        const cache = await invoke<string>('fotos_en_cache', { diseno })
        if (!puente.copiar(elegido, cache)) throw new Error('No se pudo guardar el PDF.')
        puente.persistir(elegido)
        destino = elegido
        elegido = null
      } else {
        destino = await invoke<string | null>('fotos_guardar_copia', { nombre, diseno })
        if (!destino) return
      }
      const guardado = destino
      const nombreDestino = puente?.nombre(guardado) || guardado.split(/[\\/]/).pop() || nombre
      setRecientes((lista) => guardarReciente(lista, { ruta: guardado, nombre: nombreDestino }))
      cerrarOrganizar()
      setExito('PDF creado')
      siPuedeCambiar(() => abrirEn(guardado, { ruta: guardado, nombre: nombreDestino }))
    } catch (e) {
      console.error(e)
      setExito('')
      setError('No se pudo crear el PDF. Inténtalo otra vez.')
    } finally {
      // Si falló después de elegir dónde guardar, el archivo vacío que se creó no se queda.
      if (elegido) puente?.borrar?.(elegido)
      setGuardando(false)
    }
  }

  /**
   * Guarda una copia. Con `organizar`, la copia lleva esas hojas en ese orden: al organizar se abre la
   * copia nueva; al extraer se sigue en el PDF de siempre. Con `proteccion` (una contraseña, o vacía para
   * quitarla) la copia sale con esa contraseña o sin ninguna, y se sigue en ella.
   */
  const guardarCopia = async (organizar?: { hojas: Hoja[]; extraer: boolean; unir: boolean }, proteccion?: string) => {
    if ((!doc && !organizar?.unir) || guardando) return
    const base = (doc ? nombreActual || doc.info.nombre : 'PDFs').replace(/\.pdf$/i, '')
    const sufijo =
      proteccion !== undefined ? (proteccion ? 'con contraseña' : 'sin contraseña') : !organizar ? 'anotado' : organizar.extraer ? 'páginas' : organizar.unir ? 'unido' : 'organizado'
    const nombre = `${base || 'Documento'} (${sufijo}).pdf`
    const hojas = organizar?.hojas
    setGuardando(true)
    try {
      const puente = window.DakartaAndroid
      let destino: string | null
      if (puente?.guardar && puente.copiar) {
        destino = await esperarDestino(nombre)
        if (!destino) return
        const elegido = destino
        // Si el motor no logra escribir la copia, el archivo vacío que se creó en el destino no se queda.
        const cache = await invoke<string>('guardar_en_cache', { hojas, proteccion }).catch((e) => {
          puente.borrar?.(elegido)
          throw e
        })
        if (!puente.copiar(destino, cache)) throw new Error('No se pudo guardar la copia.')
        puente.persistir(destino)
      } else {
        destino = await invoke<string | null>('guardar_copia', { nombre, hojas, proteccion })
        if (!destino) return
      }
      const guardada = destino
      // Windows: el nombre que eligió en la ventana de guardar, no el que se le sugirió.
      const nombreDestino = puente?.nombre(guardada) || guardada.split(/[\\/]/).pop() || nombre
      if (organizar) {
        if (organizar.extraer) {
          setRecientes((lista) => guardarReciente(lista, { ruta: guardada, nombre: nombreDestino }))
          setExito('Páginas extraídas en una copia aparte')
        } else {
          // Ningún PDF cambió: se abre el nuevo (organizado o unido) para seguir en él.
          cerrarOrganizar()
          abrirEn(guardada, { ruta: guardada, nombre: nombreDestino })
        }
        return
      }
      if (proteccion !== undefined) {
        // Se sigue en la copia nueva, abierta con su contraseña (o sin ninguna).
        abrirEn(guardada, { ruta: guardada, nombre: nombreDestino }, proteccion || undefined)
        setExito(proteccion ? 'Copia con contraseña guardada' : 'Copia sin contraseña guardada')
        return
      }
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

  // Leyendo (sin editar ni seleccionar texto), se piden los enlaces de las páginas que se ven.
  useEffect(() => {
    if (editando || seleccionTexto || !doc) return
    const [a, b] = visibles
    for (let n = Math.max(0, a - 1); n <= Math.min(doc.info.paginas - 1, b + 1); n++) {
      if (!(n in enlaces)) void cargarEnlaces(n)
    }
  }, [editando, seleccionTexto, doc, visibles, enlaces, cargarEnlaces])

  // Al seleccionar texto, se piden las palabras de las páginas que se ven.
  useEffect(() => {
    if (!seleccionTexto || !doc) return
    const [a, b] = visibles
    for (let n = Math.max(0, a - 1); n <= Math.min(doc.info.paginas - 1, b + 1); n++) {
      if (!(n in palabras)) void cargarPalabras(n)
    }
  }, [seleccionTexto, doc, visibles, palabras, cargarPalabras])

  // El botón o gesto "atrás" de Android cierra primero el panel o la búsqueda; si no hay nada abierto, sale.
  useEffect(() => {
    window.DakartaAtras = () => {
      if (saliendo.current) {
        saliendo.current = false
        return false
      }
      if (pregunta) {
        setPregunta(null)
        return true
      }
      if (recortando !== null) {
        setRecortando(null)
        return true
      }
      if (pideClave) {
        setPideClave(null)
        pideClave.alCancelar?.()
        return true
      }
      if (verAcerca) {
        setVerAcerca(false)
        return true
      }
      if (organizar) {
        cerrarOrganizar()
        return true
      }
      if (verIndice) {
        setVerIndice(false)
        return true
      }
      if (verBuscar) {
        cerrarBuscar()
        return true
      }
      const puente = window.DakartaAndroid
      if (!sucio || !puente?.salir) return false
      setPregunta({
        titulo: 'Hay cambios sin guardar',
        texto: 'Si sales de Dakarta, se pierden los cambios.',
        si: 'Salir',
        no: 'Seguir aquí',
        alSi: () => {
          saliendo.current = true
          puente.salir?.()
        },
      })
      return true
    }
    return () => {
      delete window.DakartaAtras
    }
  }, [verIndice, verBuscar, cerrarBuscar, sucio, pregunta, pideClave, recortando, verAcerca, organizar, cerrarOrganizar])

  // Windows: avisar antes de cerrar la ventana si hay cambios sin guardar.
  useEffect(() => {
    if (window.DakartaAndroid || !('__TAURI_INTERNALS__' in window)) return
    let des = () => {}
    import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) =>
        getCurrentWindow()
          .onCloseRequested((e) => {
            if (!sucio) return
            e.preventDefault()
            setPregunta({
              titulo: 'Hay cambios sin guardar',
              texto: 'Si cierras Dakarta, se pierden los cambios.',
              si: 'Cerrar',
              no: 'Seguir aquí',
              alSi: () => void getCurrentWindow().destroy(),
            })
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
      abrirEn(ruta, reciente)
    }
    const pendiente = () => {
      const ruta = window.DakartaAndroid?.pendiente()
      // Se estaba eligiendo un PDF para unir: va a la pantalla de unir, no se abre. Si al volver no llegó
      // ninguno, se canceló.
      const espera = esperandoAgregado.current
      if (espera) {
        if (ruta || (document.visibilityState === 'visible' && salioParaElegir.current)) {
          esperandoAgregado.current = null
          salioParaElegir.current = false
          espera(ruta ?? null)
        }
        return
      }
      if (!ruta) return
      siPuedeCambiar(() => abrirRuta(ruta))
    }
    pendiente()
    invoke<string | null>('archivo_inicial').then((ruta) => {
      if (ruta) abrirRuta(ruta)
      else if (!llegoArchivo) {
        const [ultimo] = leerRecientes()
        if (!ultimo) return
        // Si la vez anterior la app se cerró abriendo un PDF, no se reabre solo: se cerraría otra vez.
        if (seCerroAbriendo()) {
          marcarAbriendo(false)
          setError(`La última vez Dakarta se cerró al abrir un PDF. Si quieres, abre «${ultimo.nombre}» con «Abrir».`)
        } else abrirRuta(ultimo.ruta, ultimo)
      }
    })
    // Si se sale de la app, el PDF no la cerró: la marca de «abriendo» ya no hace falta.
    const alSalir = () => {
      if (document.visibilityState !== 'hidden') return
      marcarAbriendo(false)
      if (esperandoAgregado.current) salioParaElegir.current = true
    }
    document.addEventListener('visibilitychange', alSalir)
    document.addEventListener('visibilitychange', pendiente)
    window.addEventListener('focus', pendiente)
    buscarActualizacion().then(setNueva)
    return () => {
      document.removeEventListener('visibilitychange', alSalir)
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
                const compartir = () => {
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
                }
                if (!sucio) return compartir()
                setPregunta({
                  titulo: 'Hay cambios sin guardar',
                  texto: 'Se compartirá el archivo original, sin los cambios.',
                  si: 'Compartir',
                  no: 'Cancelar',
                  alSi: compartir,
                })
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
            {!editando && (
              <button
                onClick={() => {
                  cerrarBuscar()
                  setVerIndice(false)
                  if (seleccionTexto) {
                    window.getSelection()?.removeAllRanges()
                    textoSeleccionado.current = ''
                  }
                  setSeleccionTexto(!seleccionTexto)
                }}
                aria-pressed={seleccionTexto}
                aria-label="Seleccionar texto"
                title="Seleccionar texto para copiar"
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M5 6V4h14v2M12 4v16M8 20h8" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>
            )}
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

      {seleccionTexto && doc && !editando && (
        <div className="seleccion-texto">
          <span>Mantén presionado o arrastra sobre el texto. Luego toca Copiar. En escaneos no hay texto.</span>
          <button onClick={copiarSeleccion}>Copiar</button>
          <button
            onClick={() => {
              window.getSelection()?.removeAllRanges()
              textoSeleccionado.current = ''
              setSeleccionTexto(false)
            }}
          >
            Cerrar
          </button>
        </div>
      )}

      {editando && doc && !verIndice && (
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
            <button
              aria-pressed={herramienta === 'mover'}
              onClick={() => {
                setSeleccion(null)
                setHerramienta('mover')
              }}
            >
              Mover
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
              {(herramienta === 'subrayar' || herramienta === 'tachar' || herramienta === 'texto' || herramienta === 'nota') && (
                <Paleta colores={COLORES_FUERTES} valor={colorTexto} onCambio={setColorTexto} />
              )}
              {herramienta === 'resaltar' && (
                <Paleta colores={COLORES_RESALTADO} valor={colorResaltado} onCambio={setColorResaltado} />
              )}
              {herramienta === 'sello' && <Paleta colores={COLORES_SELLO} valor={colorSello} onCambio={setColorSello} />}
              {(herramienta === 'lapiz' || herramienta === 'firma') && (
                <Tamanos opciones={GROSORES} valor={grosor} onCambio={setGrosor} />
              )}
              {(herramienta === 'texto' || herramienta === 'nota') && (
                <Tamanos opciones={TAMANOS_TEXTO} valor={tamanoTexto} onCambio={setTamanoTexto} />
              )}
              {herramienta === 'sello' && <Tamanos opciones={TAMANOS_SELLO} valor={tamanoSello} onCambio={setTamanoSello} />}
              {(herramienta === 'nota' || herramienta === 'texto' || herramienta === 'firma' || herramienta === 'sello') && (
                <span className="pista">Toca la página donde lo quieres</span>
              )}
            </div>
          )}
          {!seleccion && herramienta === 'mover' && (
            <div className="fila-paleta">
              <span className="pista">Toca un dibujo, texto o sello para cambiarlo; arrástralo para moverlo</span>
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
            <button onClick={() => guardarCopia()} disabled={guardando}>
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
              <p>Este PDF no trae índice lateral. Si tiene una página de índice al inicio, toca sus líneas: si traen enlace, te llevan a esa página.</p>
            )}
            <button
              className="version-indice"
              onClick={() => {
                setVerIndice(false)
                setOrganizar({ modo: 'organizar', fuentes: [fuenteAbierta(doc)] })
              }}
            >
              Organizar páginas
            </button>
            <button
              className="version-indice"
              onClick={() => {
                setVerIndice(false)
                setOrganizar({ modo: 'unir', fuentes: [fuenteAbierta(doc)] })
              }}
            >
              Unir con otros PDFs
            </button>
            <button
              className="version-indice"
              onClick={() => {
                setVerIndice(false)
                setOrganizar({ modo: 'fotos', fuentes: [] })
              }}
            >
              Crear PDF desde fotos
            </button>
            <button
              className="version-indice"
              onClick={() => {
                setVerIndice(false)
                setPideClave({
                  titulo: doc.info.con_clave ? 'Cambiar la contraseña' : 'Proteger con contraseña',
                  texto: 'Se guarda una copia que pide esta contraseña para abrirse. El PDF de ahora no cambia.',
                  nueva: true,
                  boton: 'Guardar copia',
                  alListo: (c) => void guardarCopia(undefined, c),
                })
              }}
            >
              {doc.info.con_clave ? 'Cambiar la contraseña' : 'Proteger con contraseña'}
            </button>
            {doc.info.con_clave && (
              <button
                className="version-indice"
                onClick={() => {
                  setVerIndice(false)
                  setPregunta({
                    titulo: 'Quitar la contraseña',
                    texto: 'Se guarda una copia que se abre sin contraseña. El PDF de ahora no cambia.',
                    si: 'Guardar copia',
                    no: 'Cancelar',
                    alSi: () => void guardarCopia(undefined, ''),
                  })
                }}
              >
                Quitar la contraseña
              </button>
            )}
            <button className="version-indice" onClick={() => setVerAcerca(true)}>
              Dakarta · versión y descargas
            </button>
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
              if (!e.isPrimary || !editando || herramienta === 'mover' || herramienta === 'formulario' || !doc) return
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
                alVerse={quitarMarcaAbriendo}
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
                onRedimensionar={redimensionarAnotacion}
                onGirar={girarAnotacion}
                palabras={seleccionTexto ? palabras : undefined}
                enlaces={!editando && !seleccionTexto ? enlaces : undefined}
                onEnlace={ir}
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
            <button className="unir-inicio" onClick={() => setOrganizar({ modo: 'unir', fuentes: [] })} disabled={abriendo}>
              Unir PDFs
            </button>
            <button className="unir-inicio" onClick={() => setOrganizar({ modo: 'fotos', fuentes: [] })} disabled={abriendo}>
              Crear PDF desde fotos
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
            <details className="panel-acerca">
              <summary>Dakarta · versión y descargas</summary>
              <AcercaDe onAviso={avisar} />
            </details>
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

      {organizar && (
        <Organizar
          fuentes={organizar.fuentes}
          modo={organizar.modo}
          onAgregar={agregarPdf}
          onFotos={prepararFotos}
          onRecortar={setRecortando}
          miniaturas={miniaturas}
          guardando={guardando}
          onGuardar={(hojas, extraer, diseno) =>
            void (organizar.modo === 'fotos'
              ? guardarFotos(hojas, diseno)
              : guardarCopia({ hojas, extraer, unir: organizar.modo === 'unir' }))
          }
          onCerrar={cerrarOrganizar}
          aviso={exito}
          error={error}
          onAviso={setError}
        />
      )}
      {recortando !== null && fotos.current.get(recortando) && (
        <Recortar
          archivo={fotos.current.get(recortando)!.archivo}
          inicial={fotos.current.get(recortando)!.recorte}
          onListo={(r) => void recortarFoto(recortando, r)}
          onCancelar={() => setRecortando(null)}
        />
      )}

      {verAcerca && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-acerca" onClick={() => setVerAcerca(false)}>
          <div className="tarjeta-aviso" onClick={(e) => e.stopPropagation()}>
            <h2 id="titulo-acerca">Dakarta</h2>
            <AcercaDe onAviso={avisar} />
            <p className="nota-ayuda">
              Gratis, sin anuncios y sin rastreo. Motor PDF: MuPDF © Artifex Software, Inc. Licencia AGPL-3.0.
            </p>
            <div className="fila-botones">
              <button className="grande" onClick={() => setVerAcerca(false)}>
                Cerrar
              </button>
            </div>
          </div>
        </div>
      )}

      {pideClave && <CuadroClave key={String(pideClave.mal)} pedido={pideClave} onCerrar={() => setPideClave(null)} />}

      {pregunta && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-pregunta">
          <div className="tarjeta-aviso">
            <h2 id="titulo-pregunta">{pregunta.titulo}</h2>
            <p>{pregunta.texto}</p>
            <div className="fila-botones">
              <button onClick={() => setPregunta(null)}>{pregunta.no}</button>
              <button
                className="grande"
                onClick={() => {
                  setPregunta(null)
                  pregunta.alSi()
                }}
              >
                {pregunta.si}
              </button>
            </div>
          </div>
        </div>
      )}

      {avisoEdicion && (
        <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-edicion">
          <div className="tarjeta-aviso">
            <h2 id="titulo-edicion">Edición básica de PDF</h2>
            <p>
              Puedes resaltar, subrayar, tachar, dibujar a mano, poner notas y cuadros de texto, firmar dibujando, rellenar
              formularios y poner sellos. Toca una anotación para cambiar su texto, color o tamaño, arrástrala para moverla y
              arrastra su esquina para agrandarla.
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

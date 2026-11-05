import { convertFileSrc } from '@tauri-apps/api/core'
import { memo, useCallback, useMemo, useRef, useState } from 'react'
import { TITULOS_ANOTACION } from './opciones.ts'
import type { Anotacion, Busqueda, Campo, Coincidencia, Doc, Enlace, Palabra, Rango } from './tipos.ts'
import { anchoEntera, MINIATURA, type Recorte } from './zoom.ts'

/** Páginas a cada lado de la actual cuya miniatura ya se pide. */
const VENTANA = 15

/**
 * Todas las páginas reservan su espacio. Cada una muestra, de abajo arriba: su número, su miniatura (una vez pedida
 * se queda, por eso al volver a una hoja ya se ve), su imagen nítida (solo cerca de la pantalla) y, con mucho zoom,
 * el recorte con el detalle exacto de lo que se está mirando.
 */
export const Paginas = memo(function Paginas({
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
  onRedimensionar,
  onGirar,
  palabras,
  enlaces,
  onEnlace,
  alVerse,
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
  onRedimensionar: (a: Anotacion, w: number, h: number) => void
  onGirar: (a: Anotacion, grados: number) => void
  palabras?: Record<number, Palabra[]>
  enlaces?: Record<number, Enlace[]>
  onEnlace: (n: number) => void
  /** Se llama cuando la primera hoja ya se ve: el documento abrió bien. */
  alVerse: () => void
}) {
  // Alto/ancho real de las páginas ya pintadas que no miden como la primera.
  const [prop, setProp] = useState<Record<number, number>>({})
  const vistas = useRef(new Set<number>())
  const base = doc.info.alto / doc.info.ancho
  const medida = useCallback(
    (n: number, real: number) => {
      alVerse()
      setProp((x) => (Math.abs(real - (x[n] ?? base)) > 0.01 * base ? { ...x, [n]: real } : x))
    },
    [base, alVerse],
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
        onRedimensionar={onRedimensionar}
        onGirar={onGirar}
        palabras={palabras?.[n]}
        enlaces={enlaces?.[n]}
        onEnlace={onEnlace}
      />
    )
  })
})

export const Pagina = memo(function Pagina({
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
  onRedimensionar,
  onGirar,
  palabras,
  enlaces,
  onEnlace,
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
  onRedimensionar: (a: Anotacion, w: number, h: number) => void
  onGirar: (a: Anotacion, grados: number) => void
  palabras?: Palabra[]
  enlaces?: Enlace[]
  onEnlace: (n: number) => void
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
      {palabras?.map((p, i) => (
        <span
          key={i}
          className="texto-pdf"
          style={{
            left: `${p.x * 100}%`,
            top: `${p.y * 100}%`,
            width: `${p.w * 100}%`,
            height: `${p.h * 100}%`,
            fontSize: `${p.h * alto}px`,
          }}
        >
          {p.texto}
        </span>
      ))}
      {enlaces?.map((e, i) => (
        <button
          key={i}
          className="enlace"
          style={{
            left: `${e.x * 100}%`,
            top: `${e.y * 100}%`,
            width: `${e.w * 100}%`,
            height: `${e.h * 100}%`,
          }}
          aria-label={`Ir a la página ${e.pagina + 1}`}
          title={`Ir a la página ${e.pagina + 1}`}
          onClick={() => onEnlace(e.pagina)}
        />
      ))}
      {anots?.map((a) => (
        <CajaAnotacion
          key={a.xref}
          a={a}
          ancho={ancho}
          alto={alto}
          seleccionada={seleccion === a.xref}
          onSeleccionar={onSeleccionar}
          onMover={onMover}
          onRedimensionar={onRedimensionar}
          onGirar={onGirar}
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
      {/* Solo el marco para tocar: el valor ya lo dibuja la página; repetirlo aquí se veía doble. */}
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
          aria-label={`${c.nombre || 'Campo del formulario'}: ${c.valor || 'vacío'}`}
          onClick={() => onCampo(c)}
        />
      ))}
    </div>
  )
})

/**
 * El recorte con el detalle exacto. Se posiciona en % de la página: al cambiar el zoom sigue en su sitio, solo estirado.
 * El anterior se queda visible hasta que llega el nuevo; encima, el nuevo es transparente mientras no ha cargado.
 */
export function Detalle({ r, id, revision, revisionTotal }: { r: Recorte; id: number; revision: number; revisionTotal: number }) {
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

export function Imagen({ src, alCargar }: { src: string; alCargar: (proporcion: number) => void }) {
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

/** Caja invisible sobre una anotación: tocarla la elige y arrastrarla la mueve. */
export function CajaAnotacion({
  a,
  ancho,
  alto,
  seleccionada,
  onSeleccionar,
  onMover,
  onRedimensionar,
  onGirar,
}: {
  a: Anotacion
  ancho: number
  alto: number
  seleccionada: boolean
  onSeleccionar: (a: Anotacion) => void
  onMover: (a: Anotacion, dx: number, dy: number) => void
  onRedimensionar: (a: Anotacion, w: number, h: number) => void
  onGirar: (a: Anotacion, grados: number) => void
}) {
  const inicio = useRef<{ x: number; y: number } | null>(null)
  const [desp, setDesp] = useState<{ x: number; y: number } | null>(null)
  /** El tamaño mientras se arrastra la esquina: el estado pinta y la ref guarda el último valor. */
  const medida = useRef<{ w: number; h: number } | null>(null)
  const [vista, setVista] = useState<{ w: number; h: number } | null>(null)
  /** Giro mientras se arrastra el círculo de girar: el centro de la caja y el ángulo de partida. */
  const giro = useRef<{ cx: number; cy: number; desde: number; grados: number } | null>(null)
  const [giroVista, setGiroVista] = useState(0)
  // El lápiz y la firma giran a cualquier ángulo; el texto, la nota y el sello, en pasos de 90° (como el PDF).
  const giraLibre = a.tipo === 'tinta'
  const gira = giraLibre || a.tipo === 'texto'
  // La firma y el lápiz crecen en proporción; el texto y el sello, a lo que se arrastre.
  const enProporcion = a.tipo === 'tinta'
  const w = vista?.w ?? a.w
  const h = vista?.h ?? a.h
  return (
    <div
      className={seleccionada ? 'anot seleccionada' : 'anot'}
      style={{
        left: `${a.x * 100}%`,
        top: `${a.y * 100}%`,
        width: `${w * 100}%`,
        height: `${h * 100}%`,
        transform:
          [desp ? `translate(${desp.x}px, ${desp.y}px)` : '', giroVista ? `rotate(${giroVista}deg)` : ''].join(' ').trim() ||
          undefined,
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
    >
      {seleccionada && gira && (
        <span
          className="girador"
          title="Arrastra para girar"
          onPointerDown={(e) => {
            e.stopPropagation()
            e.preventDefault()
            e.currentTarget.setPointerCapture(e.pointerId)
            const caja = e.currentTarget.parentElement!.getBoundingClientRect()
            const cx = caja.left + caja.width / 2
            const cy = caja.top + caja.height / 2
            giro.current = { cx, cy, desde: Math.atan2(e.clientY - cy, e.clientX - cx), grados: 0 }
          }}
          onPointerMove={(e) => {
            const g = giro.current
            if (!g) return
            e.stopPropagation()
            e.preventDefault()
            let grados = ((Math.atan2(e.clientY - g.cy, e.clientX - g.cx) - g.desde) * 180) / Math.PI
            grados = ((grados + 540) % 360) - 180
            if (!giraLibre) grados = Math.round(grados / 90) * 90
            g.grados = grados
            setGiroVista(grados)
          }}
          onPointerUp={() => {
            const g = giro.current
            giro.current = null
            setGiroVista(0)
            if (g && Math.abs(g.grados) >= (giraLibre ? 2 : 45)) onGirar(a, g.grados)
          }}
          onPointerCancel={() => {
            giro.current = null
            setGiroVista(0)
          }}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M19 12a7 7 0 1 1-2.05-4.95" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
            <path d="M19.5 3.5v4.5H15" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </span>
      )}
      {seleccionada && (
        <span
          className="tirador"
          title="Arrastra para agrandar o achicar"
          onPointerDown={(e) => {
            e.stopPropagation()
            e.preventDefault()
            e.currentTarget.setPointerCapture(e.pointerId)
            inicio.current = { x: e.clientX, y: e.clientY }
            medida.current = { w: a.w, h: a.h }
            setVista(medida.current)
          }}
          onPointerMove={(e) => {
            const i = inicio.current
            if (!i) return
            e.stopPropagation()
            e.preventDefault()
            let nw = Math.min(1 - a.x, Math.max(0.02, a.w + (e.clientX - i.x) / ancho))
            let nh = Math.min(1 - a.y, Math.max(0.02, a.h + (e.clientY - i.y) / alto))
            if (enProporcion && a.w > 0 && a.h > 0) {
              const k = Math.min(nw / a.w, nh / a.h)
              nw = a.w * k
              nh = a.h * k
            }
            medida.current = { w: nw, h: nh }
            setVista(medida.current)
          }}
          onPointerUp={() => {
            const i = inicio.current
            inicio.current = null
            if (!i) return
            const m = medida.current
            medida.current = null
            setVista(null)
            if (m && (Math.abs(m.w - a.w) > 0.005 || Math.abs(m.h - a.h) > 0.005)) onRedimensionar(a, m.w, m.h)
          }}
          onPointerCancel={() => {
            inicio.current = null
            medida.current = null
            setVista(null)
          }}
        />
      )}
    </div>
  )
}

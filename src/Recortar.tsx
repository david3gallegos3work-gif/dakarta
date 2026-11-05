import { useEffect, useRef, useState } from 'react'
import { FOTO_ENTERA, arrastrarRecorte, type Agarre, type Recorte } from './fotos.ts'

const ESQUINAS: Agarre[] = ['ai', 'ad', 'bi', 'bd']

/** Recortar una foto al borde del documento: un recuadro con cuatro esquinas que se arrastran. */
export function Recortar({
  archivo,
  inicial,
  onListo,
  onCancelar,
}: {
  archivo: File
  inicial?: Recorte
  onListo: (r: Recorte) => void
  onCancelar: () => void
}) {
  const [url, setUrl] = useState('')
  const [r, setR] = useState<Recorte>(inicial ?? FOTO_ENTERA)
  const [proporcion, setProporcion] = useState(0)
  const [zona, setZona] = useState({ w: 0, h: 0 })
  const foto = useRef<HTMLDivElement>(null)
  const zonaRef = useRef<HTMLDivElement>(null)
  const agarre = useRef<{ que: Agarre; x: number; y: number; desde: Recorte } | null>(null)

  useEffect(() => {
    const u = URL.createObjectURL(archivo)
    setUrl(u)
    return () => URL.revokeObjectURL(u)
  }, [archivo])

  // La foto ocupa lo más que puede de la zona, sin deformarse.
  useEffect(() => {
    const el = zonaRef.current
    if (!el) return
    const medir = () => setZona({ w: el.clientWidth - 48, h: el.clientHeight - 48 })
    medir()
    const o = new ResizeObserver(medir)
    o.observe(el)
    return () => o.disconnect()
  }, [])
  const ancho = proporcion ? Math.max(0, Math.min(zona.w, zona.h * proporcion)) : 0
  const tamano = proporcion ? { width: ancho, height: ancho / proporcion } : undefined

  const empezar = (que: Agarre) => (e: React.PointerEvent) => {
    e.stopPropagation()
    e.currentTarget.setPointerCapture(e.pointerId)
    agarre.current = { que, x: e.clientX, y: e.clientY, desde: r }
  }
  const mover = (e: React.PointerEvent) => {
    const a = agarre.current
    const caja = foto.current?.getBoundingClientRect()
    if (!a || !caja?.width || !caja.height) return
    setR(arrastrarRecorte(a.desde, a.que, (e.clientX - a.x) / caja.width, (e.clientY - a.y) / caja.height))
  }
  const soltar = () => {
    agarre.current = null
  }
  const pct = (v: number) => `${v * 100}%`

  return (
    <div className="recortar" role="dialog" aria-modal="true" aria-label="Recortar foto">
      <p className="organizar-ayuda">Arrastra las esquinas hasta el borde del documento. Arrastra el recuadro para moverlo.</p>
      <div className="recortar-zona" ref={zonaRef}>
        <div
          ref={foto}
          className="recortar-foto"
          style={tamano}
          onPointerMove={mover}
          onPointerUp={soltar}
          onPointerCancel={soltar}
        >
          {url && (
            <img
              src={url}
              alt=""
              draggable={false}
              onLoad={(e) => setProporcion(e.currentTarget.naturalWidth / e.currentTarget.naturalHeight || 1)}
            />
          )}
          {proporcion > 0 && (
            <div
              className="recortar-cuadro"
              style={{ left: pct(r.x), top: pct(r.y), width: pct(r.w), height: pct(r.h) }}
              onPointerDown={empezar('todo')}
              onPointerMove={mover}
              onPointerUp={soltar}
              onPointerCancel={soltar}
            >
              {ESQUINAS.map((q) => (
                <span
                  key={q}
                  className={`recortar-esquina ${q}`}
                  onPointerDown={empezar(q)}
                  onPointerMove={mover}
                  onPointerUp={soltar}
                  onPointerCancel={soltar}
                />
              ))}
            </div>
          )}
        </div>
      </div>
      <div className="organizar-acciones">
        <button onClick={onCancelar}>Cancelar</button>
        <button onClick={() => setR(FOTO_ENTERA)}>Foto entera</button>
        <button className="grande" onClick={() => onListo(r)}>
          Listo
        </button>
      </div>
    </div>
  )
}

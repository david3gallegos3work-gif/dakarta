import { useEffect, useRef, useState } from 'react'
import { FOTO_ENTERA, arrastrarRecorte, type Agarre, type Recorte } from './fotos.ts'

/**
 * Recortar una foto al borde del documento: cuatro esquinas que se mueven libres. Si la foto se tomó
 * torcida o de medio lado, al guardarla el documento se endereza a un rectángulo.
 */
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

  // La foto ocupa lo más que puede de la zona, sin deformarse (con lugar para las esquinas).
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
  const figura = r.map((p) => `${p.x},${p.y}`).join(' ')
  const eventos = { onPointerMove: mover, onPointerUp: soltar, onPointerCancel: soltar }

  return (
    <div className="recortar" role="dialog" aria-modal="true" aria-label="Recortar foto">
      <p className="organizar-ayuda">
        Lleva cada esquina a la esquina del documento. Si salió torcido o de lado, se endereza solo.
      </p>
      <div className="recortar-zona" ref={zonaRef}>
        <div ref={foto} className="recortar-foto" style={tamano}>
          {url && (
            <img
              src={url}
              alt=""
              draggable={false}
              onLoad={(e) => setProporcion(e.currentTarget.naturalWidth / e.currentTarget.naturalHeight || 1)}
            />
          )}
          {proporcion > 0 && (
            <>
              <svg className="recortar-marco" viewBox="0 0 1 1" preserveAspectRatio="none">
                <path d={`M0,0H1V1H0Z M${figura.replaceAll(' ', ' L')}Z`} fillRule="evenodd" className="recortar-sombra" />
                <polygon points={figura} className="recortar-figura" onPointerDown={empezar('todo')} {...eventos} />
              </svg>
              {r.map((p, i) => (
                <span
                  key={i}
                  className="recortar-esquina"
                  style={{ left: `${p.x * 100}%`, top: `${p.y * 100}%` }}
                  onPointerDown={empezar(i as Agarre)}
                  {...eventos}
                />
              ))}
            </>
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

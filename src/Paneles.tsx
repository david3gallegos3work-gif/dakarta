import { useEffect, useRef, useState } from 'react'
import { COLORES_FUERTES, COLORES_RESALTADO, GROSORES, hex, TAMANOS_TEXTO, TITULOS_ANOTACION } from './opciones.ts'
import type { Anotacion, Entrada } from './tipos.ts'

export function Paleta({
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

export function Tamanos({
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


/** Panel para cambiar el texto, el color, el tamaño o borrar la anotación elegida. */
export function PanelAnotacion({
  a,
  onAplicar,
  onBorrar,
  onCerrar,
}: {
  a: Anotacion
  onAplicar: (cambios: { contenido?: string; color?: string; tamano?: number; junto?: boolean }) => void
  onBorrar: () => void
  onCerrar: () => void
}) {
  const [texto, setTexto] = useState(a.contenido)
  const [color, setColor] = useState(hex(a.rojo, a.verde, a.azul))
  const [tamano, setTamano] = useState(
    a.tamano || (a.tipo === 'nota' ? 40 : a.tipo === 'texto' ? 12 : 3),
  )
  const conTexto = a.tipo === 'nota' || a.tipo === 'texto'
  const colores = a.tipo === 'resaltar' ? COLORES_RESALTADO : COLORES_FUERTES
  const tamanos = a.tipo === 'texto' ? TAMANOS_TEXTO : a.tipo === 'tinta' ? GROSORES : null
  // El texto se va viendo en la hoja mientras se escribe: se guarda en cada pausa corta. El color y el
  // tamaño, al tocarlos. No hace falta "Aplicar". Todo lo escrito de un tirón es un solo paso de deshacer.
  const guardado = useRef(a.contenido)
  const enTanda = useRef(false)
  // El teclado del celular arma cada palabra antes de soltarla: guardar a mitad de palabra le hacía cerrarla
  // y aplicar su corrección (una N sola se volvía Ñ). Se espera a que termine la palabra.
  const componiendo = useRef(false)
  const reloj = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(reloj.current), [])
  const guardar = (valor: string) => {
    clearTimeout(reloj.current)
    const limpio = valor.trim()
    if (!conTexto || !limpio || limpio === guardado.current.trim()) return
    guardado.current = valor
    onAplicar({ contenido: valor, junto: enTanda.current })
    enTanda.current = true
  }
  const guardarTexto = () => {
    guardar(texto)
    enTanda.current = false
  }
  return (
    <div className="fila-seleccion">
      <strong>{TITULOS_ANOTACION[a.tipo]}</strong>
      {conTexto && (
        <textarea
          value={texto}
          autoCorrect="off"
          onCompositionStart={() => {
            componiendo.current = true
            clearTimeout(reloj.current)
          }}
          onCompositionEnd={(e) => {
            componiendo.current = false
            const valor = e.currentTarget.value
            clearTimeout(reloj.current)
            reloj.current = setTimeout(() => guardar(valor), 350)
          }}
          onChange={(e) => {
            const valor = e.currentTarget.value
            setTexto(valor)
            clearTimeout(reloj.current)
            if (!componiendo.current) reloj.current = setTimeout(() => guardar(valor), 350)
          }}
          onBlur={guardarTexto}
          placeholder={a.tipo === 'nota' ? 'Texto de la nota' : 'Texto'}
          aria-label={a.tipo === 'nota' ? 'Texto de la nota' : 'Texto'}
        />
      )}
      <Paleta
        colores={colores}
        valor={color}
        onCambio={(c) => {
          setColor(c)
          onAplicar({ color: c })
        }}
      />
      {tamanos && (
        <Tamanos
          opciones={tamanos}
          valor={tamano}
          onCambio={(n) => {
            setTamano(n)
            onAplicar({ tamano: n })
          }}
        />
      )}
      <button type="button" onClick={() => { guardarTexto(); onCerrar() }}>
        Cerrar
      </button>
      <button type="button" onClick={onBorrar}>
        Borrar
      </button>
    </div>
  )
}

export function Indice({ entradas, ir }: { entradas: Entrada[]; ir: (n: number) => void }) {
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

import { useState } from 'react'
import { problemaClave } from './clave.ts'

/** Lo que pide el cuadro: la contraseña de un PDF para abrirlo, o una nueva (escrita dos veces) para protegerlo. */
export type PideClave = {
  titulo: string
  texto: string
  /** La anterior no sirvió. */
  mal?: boolean
  nueva?: boolean
  boton: string
  alListo: (clave: string) => void
  alCancelar?: () => void
}

export function CuadroClave({ pedido, onCerrar }: { pedido: PideClave; onCerrar: () => void }) {
  const [clave, setClave] = useState('')
  const [otraVez, setOtraVez] = useState('')
  const [ver, setVer] = useState(false)
  const [intento, setIntento] = useState(false)
  const problema = pedido.nueva ? problemaClave(clave, otraVez) : clave ? '' : 'Escribe la contraseña.'
  const cancelar = () => {
    onCerrar()
    pedido.alCancelar?.()
  }
  const listo = () => {
    setIntento(true)
    if (problema) return
    onCerrar()
    pedido.alListo(clave)
  }
  const tipo = ver ? 'text' : 'password'
  return (
    <div className="fondo-aviso" role="dialog" aria-modal="true" aria-labelledby="titulo-clave">
      <form
        className="tarjeta-aviso"
        onSubmit={(e) => {
          e.preventDefault()
          listo()
        }}
      >
        <h2 id="titulo-clave">{pedido.titulo}</h2>
        <p>{pedido.texto}</p>
        {pedido.mal && <p className="clave-mal">Esa contraseña no es. Inténtalo otra vez.</p>}
        <input
          className="campo-clave"
          type={tipo}
          value={clave}
          onChange={(e) => setClave(e.target.value)}
          placeholder="Contraseña"
          aria-label="Contraseña"
          autoComplete={pedido.nueva ? 'new-password' : 'current-password'}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
          autoFocus
        />
        {pedido.nueva && (
          <input
            className="campo-clave"
            type={tipo}
            value={otraVez}
            onChange={(e) => setOtraVez(e.target.value)}
            placeholder="Escríbela otra vez"
            aria-label="Escríbela otra vez"
            autoComplete="new-password"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        )}
        <label>
          <input type="checkbox" checked={ver} onChange={(e) => setVer(e.target.checked)} />
          Mostrar la contraseña
        </label>
        {intento && problema && <p className="clave-mal">{problema}</p>}
        {pedido.nueva && (
          <p className="clave-nota">Anótala en un lugar seguro: si la olvidas, nadie podrá abrir la copia.</p>
        )}
        <div className="fila-botones">
          <button type="button" onClick={cancelar}>
            Cancelar
          </button>
          <button type="submit" className="grande">
            {pedido.boton}
          </button>
        </div>
      </form>
    </div>
  )
}

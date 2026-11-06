import { useEffect, useRef, useState } from 'react'
import { ErrorGemini, SinConexion, explicar, preguntar, revisarClave, tareaDocumento, traducirTexto, type Charla, type Pieza, type Tarea } from './gemini.ts'

type Mensaje = { pregunta?: string; texto: string }
/** Lo que se le pide: una tarea sobre las páginas, una pregunta, o explicar o traducir el texto elegido. */
type Pedido = { tarea: Tarea } | { pregunta: string } | { explicar: string } | { traducir: string }

const TAREAS: { tarea: Tarea; boton: string; titulo: string }[] = [
  { tarea: 'resumir', boton: 'Resumir', titulo: 'Resumen' },
  { tarea: 'puntos', boton: 'Puntos clave', titulo: 'Puntos clave' },
  { tarea: 'estudio', boton: 'Preguntas de estudio', titulo: 'Preguntas de estudio' },
  { tarea: 'traducir', boton: 'Traducir', titulo: 'Traducción' },
]
const corto = (t: string) => (t.length > 140 ? `${t.slice(0, 140)}…` : t)

const mensajeDe = (e: unknown) => (e instanceof SinConexion || e instanceof ErrorGemini ? e.message : e instanceof Error ? e.message : String(e))

/** Texto de Gemini en pantalla: párrafos, listas con «- » y **negritas** (sin HTML: nada se ejecuta). */
export function TextoGemini({ texto }: { texto: string }) {
  const negritas = (linea: string) =>
    linea.split(/(\*\*[^*]+\*\*)/g).map((t, i) => (t.startsWith('**') && t.endsWith('**') && t.length > 4 ? <strong key={i}>{t.slice(2, -2)}</strong> : t))
  const bloques: React.ReactNode[] = []
  let lista: string[] = []
  const cerrarLista = () => {
    if (lista.length) bloques.push(<ul key={bloques.length}>{lista.map((l, i) => <li key={i}>{negritas(l)}</li>)}</ul>)
    lista = []
  }
  for (const linea of texto.split('\n')) {
    const vineta = linea.match(/^\s*(?:[-*•]|\d+[.)])\s+(.*)$/)
    if (vineta) lista.push(vineta[1])
    else {
      cerrarLista()
      if (linea.trim()) bloques.push(<p key={bloques.length}>{negritas(linea.replace(/^#+\s*/, ''))}</p>)
    }
  }
  cerrarLista()
  return <div className="texto-gemini">{bloques}</div>
}

/**
 * Gemini: resumir el PDF y preguntarle. Sin clave, explica cómo vincularla (gratis, una sola vez). Solo esta
 * pantalla usa internet; lo demás de la app funciona sin conexión.
 */
export function Gemini({
  paginas,
  clave,
  elegido,
  onClave,
  cargarPiezas,
  onAbrirGoogle,
  onCerrar,
}: {
  paginas: number
  clave: string
  /** El texto seleccionado en el PDF, para explicarlo o traducirlo. */
  elegido?: string
  /** Guarda la clave ya revisada con Google (vacía: desvincular). */
  onClave: (clave: string) => Promise<void>
  /** Las páginas `desde`..=`hasta` (desde 0) como las recibe Gemini: texto o, si son escaneadas, imagen. */
  cargarPiezas: (desde: number, hasta: number, avance: (t: string) => void) => Promise<Pieza[]>
  onAbrirGoogle: () => void
  onCerrar: () => void
}) {
  const [borrador, setBorrador] = useState('')
  const [revisando, setRevisando] = useState(false)
  const [error, setError] = useState('')
  const [pensando, setPensando] = useState('')
  const [mensajes, setMensajes] = useState<Mensaje[]>([])
  const [pregunta, setPregunta] = useState('')
  const [todas, setTodas] = useState(true)
  const [desde, setDesde] = useState(1)
  const [hasta, setHasta] = useState(Math.min(paginas, 20))
  const [copiado, setCopiado] = useState(-1)
  const [menu, setMenu] = useState(false)
  // Las páginas ya leídas se guardan: una pregunta tras otra no las vuelve a sacar.
  const leidas = useRef<{ clave: string; piezas: Pieza[] } | null>(null)
  const fin = useRef<HTMLDivElement>(null)

  useEffect(() => {
    fin.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [mensajes, pensando])

  // Abierto desde «Explicar» con texto elegido: se explica enseguida (si ya está vinculado).
  const explicado = useRef(false)
  useEffect(() => {
    if (!clave || !elegido || explicado.current) return
    explicado.current = true
    void trabajar({ explicar: elegido })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clave, elegido])

  const vincular = async () => {
    const c = borrador.trim()
    if (!c) return
    setRevisando(true)
    setError('')
    try {
      await revisarClave(c)
      await onClave(c)
      setBorrador('')
    } catch (e) {
      setError(mensajeDe(e))
    } finally {
      setRevisando(false)
    }
  }

  const rangoValido = todas || (desde >= 1 && hasta >= desde && hasta <= paginas)
  const piezas = async () => {
    const [a, b] = todas ? [0, paginas - 1] : [desde - 1, hasta - 1]
    const k = `${a}-${b}`
    if (leidas.current?.clave !== k) leidas.current = { clave: k, piezas: await cargarPiezas(a, b, setPensando) }
    return leidas.current.piezas
  }

  const trabajar = async (pedido: Pedido) => {
    if (pensando || (!rangoValido && ('tarea' in pedido || 'pregunta' in pedido))) return
    setError('')
    try {
      if ('explicar' in pedido) {
        setPensando('Gemini está explicando el texto…')
        const texto = await explicar(pedido.explicar, clave)
        setMensajes((m) => [...m, { pregunta: `Explicar: «${corto(pedido.explicar)}»`, texto }])
        return
      }
      if ('traducir' in pedido) {
        setPensando('Gemini está traduciendo el texto…')
        const texto = await traducirTexto(pedido.traducir, clave)
        setMensajes((m) => [...m, { pregunta: `Traducir: «${corto(pedido.traducir)}»`, texto }])
        return
      }
      setPensando('Preparando las páginas…')
      const p = await piezas()
      if ('tarea' in pedido) {
        const texto = await tareaDocumento(pedido.tarea, p, clave, setPensando)
        const titulo = TAREAS.find((t) => t.tarea === pedido.tarea)!.titulo
        setMensajes((m) => [...m, { pregunta: todas ? `${titulo} del PDF` : `${titulo} de las páginas ${desde} a ${hasta}`, texto }])
      } else {
        const historial: Charla[] = mensajes.filter((m) => m.pregunta).map((m) => ({ pregunta: m.pregunta!, respuesta: m.texto }))
        const texto = await preguntar(pedido.pregunta, historial, p, clave, setPensando)
        setMensajes((m) => [...m, { pregunta: pedido.pregunta, texto }])
        setPregunta('')
      }
    } catch (e) {
      setError(mensajeDe(e))
    } finally {
      setPensando('')
    }
  }

  const copiar = (i: number, texto: string) => {
    navigator.clipboard?.writeText(texto.replace(/\*\*/g, '')).then(
      () => {
        setCopiado(i)
        setTimeout(() => setCopiado(-1), 1500)
      },
      () => setError('No se pudo copiar.'),
    )
  }

  return (
    <div className="gemini" role="dialog" aria-modal="true" aria-label="Gemini">
      <header className="barra organizar-barra">
        <button onClick={onCerrar} aria-label="Volver" title="Volver">
          ←
        </button>
        <strong>✨ Gemini</strong>
        {clave && (
          <button className="derecha" onClick={() => setMenu((x) => !x)} aria-label="Más opciones" aria-expanded={menu}>
            ⋮
          </button>
        )}
      </header>
      {menu && (
        <div className="gemini-menu">
          <button
            onClick={() => {
              setMenu(false)
              void onClave('')
              setMensajes([])
            }}
          >
            Desvincular Gemini (borrar la clave de este aparato)
          </button>
        </div>
      )}
      {!clave ? (
        <div className="gemini-cuerpo">
          <p>
            Vincula tu Gemini <strong>gratis</strong> una sola vez para resumir tus PDFs y hacerles preguntas.
          </p>
          <ol className="gemini-pasos">
            <li>
              <button className="grande" onClick={onAbrirGoogle}>
                Abrir Google
              </button>{' '}
              y entra con tu cuenta.
            </li>
            <li>
              Toca <strong>Crear clave de API</strong> (Create API key) y cópiala.
            </li>
            <li>
              Vuelve aquí, pégala abajo y toca <strong>Vincular</strong>.
            </li>
          </ol>
          <div className="gemini-fila">
            <input
              value={borrador}
              onChange={(e) => setBorrador(e.target.value)}
              placeholder="Pega la clave aquí"
              aria-label="Clave de Gemini"
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
            />
            <button className="grande" disabled={!borrador.trim() || revisando} onClick={() => void vincular()}>
              {revisando ? 'Revisando…' : 'Vincular'}
            </button>
          </div>
          {error && <p className="gemini-error">{error}</p>}
          <p className="gemini-nota">
            La clave queda solo en este aparato. En el plan gratis, Google puede revisar lo que le envíes: no lo uses con documentos delicados
            (bancarios, médicos, contraseñas).
          </p>
        </div>
      ) : (
        <>
          <div className="gemini-cuerpo">
            <p className="gemini-nota">
              Se envía a Google el texto de las páginas (o su imagen si son escaneadas). En el plan gratis puede revisarlo: no lo uses con
              documentos delicados.
            </p>
            <div className="gemini-rango">
              <label>
                <input type="radio" checked={todas} onChange={() => setTodas(true)} /> Todo el PDF ({paginas} {paginas === 1 ? 'página' : 'páginas'})
              </label>
              {paginas > 1 && (
                <label>
                  <input type="radio" checked={!todas} onChange={() => setTodas(false)} /> Páginas
                  <input
                    type="number"
                    min={1}
                    max={paginas}
                    value={desde}
                    onFocus={() => setTodas(false)}
                    onChange={(e) => setDesde(Number(e.target.value))}
                    aria-label="Desde la página"
                  />
                  a
                  <input
                    type="number"
                    min={1}
                    max={paginas}
                    value={hasta}
                    onFocus={() => setTodas(false)}
                    onChange={(e) => setHasta(Number(e.target.value))}
                    aria-label="Hasta la página"
                  />
                </label>
              )}
            </div>
            {elegido && (
              <div className="gemini-elegido">
                <p>
                  <strong>Texto elegido:</strong> «{corto(elegido)}»
                </p>
                <div className="gemini-tareas">
                  <button disabled={!!pensando} onClick={() => void trabajar({ explicar: elegido })}>
                    Explicar
                  </button>
                  <button disabled={!!pensando} onClick={() => void trabajar({ traducir: elegido })}>
                    Traducir
                  </button>
                </div>
              </div>
            )}
            <div className="gemini-tareas">
              {TAREAS.map((t) => (
                <button key={t.tarea} disabled={!!pensando || !rangoValido} onClick={() => void trabajar({ tarea: t.tarea })}>
                  {t.boton}
                </button>
              ))}
            </div>
            {mensajes.map((m, i) => (
              <div key={i} className="gemini-mensaje">
                {m.pregunta && <p className="gemini-pregunta">{m.pregunta}</p>}
                <div className="gemini-respuesta">
                  <TextoGemini texto={m.texto} />
                  <button className="gemini-copiar" onClick={() => copiar(i, m.texto)}>
                    {copiado === i ? 'Copiado ✓' : 'Copiar'}
                  </button>
                </div>
              </div>
            ))}
            {pensando && (
              <p className="gemini-pensando" role="status">
                <span className="ruedita" aria-hidden="true" /> {pensando}
              </p>
            )}
            {error && <p className="gemini-error">{error}</p>}
            <div ref={fin} />
          </div>
          <form
            className="gemini-preguntar"
            onSubmit={(e) => {
              e.preventDefault()
              if (pregunta.trim()) void trabajar({ pregunta: pregunta.trim() })
            }}
          >
            <input
              value={pregunta}
              onChange={(e) => setPregunta(e.target.value)}
              placeholder="Pregúntale al PDF…"
              aria-label="Pregunta para el PDF"
              enterKeyHint="send"
            />
            <button type="submit" className="grande" disabled={!pregunta.trim() || !!pensando || !rangoValido}>
              Preguntar
            </button>
          </form>
        </>
      )}
    </div>
  )
}

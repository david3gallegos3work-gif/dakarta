// Gemini (opcional, apagado hasta vincularlo): resumir el PDF y preguntarle, con la clave gratis de cada persona
// sacada de Google AI Studio. Costo cero: la app llama directo a Google, sin servidor propio. Basado en Datheke.
// Aviso: en el plan gratis Google puede usar y revisar lo enviado para mejorar sus productos.
const API = 'https://generativelanguage.googleapis.com/v1beta/models'
// Alias que Google mueve al modelo más nuevo: Flash-Lite primero (el que más usos gratis da por día), luego Flash.
export const MODELOS = ['gemini-flash-lite-latest', 'gemini-flash-latest']
const CLAVE_MODELO = 'dakarta-gemini-modelo'

const modeloGuardado = () => {
  try {
    return localStorage.getItem(CLAVE_MODELO) ?? ''
  } catch {
    return ''
  }
}
const guardarModelo = (m: string) => {
  try {
    localStorage.setItem(CLAVE_MODELO, m)
  } catch {
    // se vuelve a buscar la próxima vez
  }
}

const REGLAS =
  'Eres el asistente de Dakarta, un lector de PDF. Respondes en español sencillo (salvo que te pidan otro idioma), ' +
  'en texto simple: párrafos cortos y listas con "- ". Puedes marcar lo importante con **negritas**. ' +
  'Sin HTML, sin tablas, sin saludos ni despedidas. Usa solo lo que dice el documento; si algo no está, dilo. ' +
  'Cuando ayude, indica la página entre paréntesis, por ejemplo (pág. 4).'

export class ErrorGemini extends Error {}
export class SinConexion extends Error {}

/** Una página del PDF para Gemini: su texto o, si es escaneada, su imagen (JPEG en base64). */
export type Pieza = { pagina: number; texto: string } | { pagina: number; imagen: string }
type Parte = { text: string } | { inlineData: { mimeType: string; data: string } }
export type Charla = { pregunta: string; respuesta: string }

const pedir = (url: string, clave: string, cuerpo?: unknown) =>
  fetch(url, {
    method: cuerpo ? 'POST' : 'GET',
    headers: { 'Content-Type': 'application/json', 'x-goog-api-key': clave },
    body: cuerpo ? JSON.stringify(cuerpo) : undefined,
  }).catch(() => {
    throw new SinConexion('Sin conexión: Gemini necesita internet.')
  })

async function falla(res: Response): Promise<ErrorGemini> {
  const msg = await res.json().then(
    (d) => String(d?.error?.message ?? ''),
    () => '',
  )
  if (res.status === 429)
    return new ErrorGemini('Se acabó el uso gratis de Gemini por ahora (Google pone un límite por minuto y por día). Espera un rato o intenta mañana.')
  if ((res.status === 400 && /api key/i.test(msg)) || res.status === 401 || res.status === 403)
    return new ErrorGemini('Google no aceptó la clave de Gemini. Revísala o crea otra.')
  if (/location|region|country/i.test(msg)) return new ErrorGemini('Gemini gratis no está disponible en tu país.')
  if (res.status === 413 || /too large|exceeds/i.test(msg)) return new ErrorGemini('Es demasiado para Gemini de una vez. Elige menos páginas.')
  if (res.status >= 500) return new ErrorGemini('Gemini está ocupado. Intenta en un momento.')
  return new ErrorGemini(`Gemini respondió ${res.status}${msg ? `: ${msg}` : ''}`)
}

/** Antes de guardar la clave: Google dice si sirve (y de paso si hay internet). */
export async function revisarClave(clave: string) {
  const res = await pedir(`${API}?pageSize=1`, clave.trim())
  if (!res.ok) throw await falla(res)
}

async function generar(partes: Parte[], clave: string): Promise<string> {
  if (!clave) throw new ErrorGemini('Falta vincular Gemini.')
  const guardado = modeloGuardado()
  const modelos = guardado ? [guardado, ...MODELOS.filter((m) => m !== guardado)] : MODELOS
  for (const modelo of modelos) {
    const res = await pedir(`${API}/${modelo}:generateContent`, clave, {
      systemInstruction: { parts: [{ text: REGLAS }] },
      contents: [{ role: 'user', parts: partes }],
      generationConfig: { temperature: 0.3 },
    })
    // Modelo que ya no existe o no está en el plan gratis: se prueba el siguiente.
    if (res.status === 404) continue
    if (!res.ok) throw await falla(res)
    guardarModelo(modelo)
    const datos = await res.json()
    if (datos.promptFeedback?.blockReason) throw new ErrorGemini('Gemini no quiso responder sobre este documento.')
    const texto: string = (datos.candidates?.[0]?.content?.parts ?? []).map((p: { text?: string }) => p.text ?? '').join('')
    if (!texto.trim()) throw new ErrorGemini('Gemini no devolvió nada. Intenta de nuevo.')
    return texto.trim()
  }
  throw new ErrorGemini('No encontré un modelo de Gemini disponible con tu clave.')
}

/** Cuánto va en cada envío: así un PDF largo no agota el límite gratis de una vez (unas 25 000 palabras). */
export const LETRAS_POR_PARTE = 150_000
export const IMAGENES_POR_PARTE = 12
/** Más partes que esto ya es demasiado para el plan gratis (muchos envíos seguidos). */
export const PARTES_MAXIMAS = 8

/** Divide las páginas en partes que entran en un envío, sin cortar una página. */
export function dividir(piezas: Pieza[], letras = LETRAS_POR_PARTE, imagenes = IMAGENES_POR_PARTE): Pieza[][] {
  const partes: Pieza[][] = []
  let actual: Pieza[] = []
  let l = 0
  let i = 0
  for (const p of piezas) {
    const largo = 'texto' in p ? p.texto.length : 0
    const img = 'imagen' in p ? 1 : 0
    if (actual.length && (l + largo > letras || i + img > imagenes)) {
      partes.push(actual)
      actual = []
      l = 0
      i = 0
    }
    actual.push(p)
    l += largo
    i += img
  }
  if (actual.length) partes.push(actual)
  return partes
}

const rango = (parte: Pieza[]) => {
  const a = parte[0].pagina + 1
  const b = parte[parte.length - 1].pagina + 1
  return a === b ? `página ${a}` : `páginas ${a} a ${b}`
}

/** El documento (o una parte) como lo recibe Gemini: cada página marcada con su número. */
export function partesDe(parte: Pieza[]): Parte[] {
  return parte.map((p) =>
    'texto' in p ? { text: `[Página ${p.pagina + 1}]\n${p.texto}` } : { inlineData: { mimeType: 'image/jpeg', data: p.imagen } },
  )
}

const marcaImagenes = (parte: Pieza[]): Parte[] =>
  parte.some((p) => 'imagen' in p)
    ? [{ text: `Las imágenes son las páginas escaneadas, en orden: ${parte.filter((p) => 'imagen' in p).map((p) => p.pagina + 1).join(', ')}.` }]
    : []

type Avance = (texto: string) => void

/** Resume el documento. Si es largo, resume cada parte y luego une los resúmenes. */
export async function resumir(piezas: Pieza[], clave: string, avance: Avance = () => {}): Promise<string> {
  const partes = dividir(piezas)
  if (partes.length > PARTES_MAXIMAS) throw new ErrorGemini('El PDF es muy largo para Gemini gratis de una vez. Elige menos páginas.')
  const pedido = 'Resume este documento: primero una frase con de qué trata y luego los puntos más importantes en una lista.'
  if (partes.length === 1) {
    avance('Gemini está leyendo el PDF…')
    return generar([{ text: pedido }, ...marcaImagenes(partes[0]), ...partesDe(partes[0])], clave)
  }
  const resumenes: string[] = []
  for (const [i, parte] of partes.entries()) {
    avance(`Gemini está leyendo la parte ${i + 1} de ${partes.length}…`)
    resumenes.push(
      `Resumen de las ${rango(parte)}:\n` +
        (await generar([{ text: `Resume esta parte del documento (${rango(parte)}) en una lista con lo más importante.` }, ...marcaImagenes(parte), ...partesDe(parte)], clave)),
    )
  }
  avance('Gemini está uniendo el resumen…')
  return generar([{ text: `${pedido}\nSon los resúmenes de cada parte, en orden:\n\n${resumenes.join('\n\n')}` }], clave)
}

const conversacion = (historial: Charla[]) =>
  historial.length
    ? `Lo que ya se habló (para entender la pregunta nueva):\n${historial.map((c) => `Pregunta: ${c.pregunta}\nRespuesta: ${c.respuesta}`).join('\n\n')}\n\n`
    : ''

/** Responde una pregunta sobre el documento. Si es largo, busca lo que sirve en cada parte y luego responde. */
export async function preguntar(pregunta: string, historial: Charla[], piezas: Pieza[], clave: string, avance: Avance = () => {}): Promise<string> {
  const partes = dividir(piezas)
  if (partes.length > PARTES_MAXIMAS) throw new ErrorGemini('El PDF es muy largo para Gemini gratis de una vez. Elige menos páginas.')
  const previo = conversacion(historial.slice(-4))
  if (partes.length === 1) {
    avance('Gemini está buscando la respuesta…')
    return generar([{ text: `${previo}Responde esta pregunta sobre el documento: ${pregunta}` }, ...marcaImagenes(partes[0]), ...partesDe(partes[0])], clave)
  }
  const notas: string[] = []
  for (const [i, parte] of partes.entries()) {
    avance(`Gemini está buscando en la parte ${i + 1} de ${partes.length}…`)
    const r = await generar(
      [
        { text: `${previo}De esta parte del documento (${rango(parte)}), copia o resume solo lo que sirva para responder: «${pregunta}». Si no hay nada útil, responde solo NADA.` },
        ...marcaImagenes(parte),
        ...partesDe(parte),
      ],
      clave,
    )
    if (!/^\s*NADA\.?\s*$/i.test(r)) notas.push(`De las ${rango(parte)}:\n${r}`)
  }
  avance('Gemini está escribiendo la respuesta…')
  if (!notas.length) return 'No encontré eso en el documento.'
  return generar([{ text: `${previo}Responde esta pregunta: ${pregunta}\nUsa solo estas notas sacadas del documento:\n\n${notas.join('\n\n')}` }], clave)
}

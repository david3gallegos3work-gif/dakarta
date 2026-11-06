import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ErrorGemini, MODELOS, SinConexion, dividir, partesDe, preguntar, resumir, revisarClave, type Pieza } from './gemini.ts'

function simular(...respuestas: (Response | Error)[]) {
  const llamadas: { url: string; init: RequestInit }[] = []
  globalThis.fetch = async (url, init) => {
    llamadas.push({ url: String(url), init: init ?? {} })
    const r = respuestas.shift()!
    if (r instanceof Error) throw r
    return r
  }
  return llamadas
}
const json = (datos: unknown, estado = 200) => new Response(JSON.stringify(datos), { status: estado })
const respuesta = (text: string) => json({ candidates: [{ content: { parts: [{ text }] } }] })
const paginas = (n: number, letras = 10): Pieza[] => Array.from({ length: n }, (_, i) => ({ pagina: i, texto: 'x'.repeat(letras) }))

test('resume un PDF corto en un solo envío, con la clave en la cabecera y cada página marcada', async () => {
  const llamadas = simular(respuesta('Trata de X.\n- uno'))
  assert.equal(await resumir([{ pagina: 0, texto: 'Hola' }, { pagina: 1, texto: 'Mundo' }], 'clave1'), 'Trata de X.\n- uno')
  assert.equal(llamadas.length, 1)
  assert.match(llamadas[0].url, new RegExp(`${MODELOS[0]}:generateContent$`))
  assert.equal((llamadas[0].init.headers as Record<string, string>)['x-goog-api-key'], 'clave1')
  assert.match(String(llamadas[0].init.body), /\[Página 1\]\\nHola[\s\S]*\[Página 2\]\\nMundo/)
})

test('un PDF largo se resume por partes y luego se unen', async () => {
  const llamadas = simular(respuesta('- a'), respuesta('- b'), respuesta('Final'))
  const avisos: string[] = []
  assert.equal(await resumir(paginas(3, 60_000), 'k', (t) => avisos.push(t)), 'Final')
  assert.equal(llamadas.length, 3)
  assert.match(String(llamadas[2].init.body), /páginas 1 a 2[\s\S]*- a[\s\S]*página 3[\s\S]*- b/)
  assert.deepEqual(avisos.slice(0, 2), ['Gemini está leyendo la parte 1 de 2…', 'Gemini está leyendo la parte 2 de 2…'])
})

test('al preguntar sobre un PDF largo se descartan las partes sin nada útil', async () => {
  const llamadas = simular(respuesta('NADA'), respuesta('El plazo es 30 días (pág. 3).'), respuesta('Son 30 días (pág. 3).'))
  const r = await preguntar('¿Cuál es el plazo?', [{ pregunta: 'hola', respuesta: 'hola' }], paginas(3, 60_000), 'k')
  assert.equal(r, 'Son 30 días (pág. 3).')
  const final = String(llamadas[2].init.body)
  assert.match(final, /plazo es 30/)
  assert.doesNotMatch(final, /NADA/)
  assert.match(final, /Lo que ya se habló/)
})

test('si ninguna parte sirve, no inventa', async () => {
  simular(respuesta('NADA'), respuesta('nada.'))
  assert.equal(await preguntar('¿x?', [], paginas(2, 100_000), 'k'), 'No encontré eso en el documento.')
})

test('las páginas escaneadas van como imagen y se dice qué páginas son', () => {
  const p = partesDe([{ pagina: 4, imagen: 'AAA' }])
  assert.deepEqual(p, [{ inlineData: { mimeType: 'image/jpeg', data: 'AAA' } }])
  assert.equal(dividir(Array.from({ length: 30 }, (_, i) => ({ pagina: i, imagen: 'A' }))).length, 3)
})

test('dividir no corta páginas y no deja partes vacías', () => {
  assert.deepEqual(dividir([]), [])
  assert.equal(dividir(paginas(10, 10)).length, 1)
  // Una página más larga que el límite va sola.
  assert.equal(dividir([{ pagina: 0, texto: 'x'.repeat(200_000) }, { pagina: 1, texto: 'y' }]).length, 2)
})

test('si un modelo ya no existe prueba el siguiente', async () => {
  const llamadas = simular(json({}, 404), respuesta('ok'))
  assert.equal(await resumir(paginas(1), 'k'), 'ok')
  assert.match(llamadas[1].url, new RegExp(MODELOS[1]))
})

test('errores en palabras simples: límite gratis, clave mala, sin internet, demasiado largo', async () => {
  simular(json({ error: { message: 'Quota exceeded' } }, 429))
  await assert.rejects(resumir(paginas(1), 'k'), (e: Error) => e instanceof ErrorGemini && /límite/.test(e.message))
  simular(json({ error: { message: 'API key not valid' } }, 400))
  await assert.rejects(revisarClave('mala'), (e: Error) => /no aceptó la clave/.test(e.message))
  simular(new TypeError('Failed to fetch'))
  await assert.rejects(revisarClave('k'), (e: Error) => e instanceof SinConexion)
  await assert.rejects(resumir(paginas(9, 150_000), 'k'), (e: Error) => /muy largo/.test(e.message))
})

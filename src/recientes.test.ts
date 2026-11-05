import assert from 'node:assert/strict'
import { test } from 'node:test'
import { agregarReciente, marcarAbriendo, MAX_RECIENTES, nombreLegible, olvidarReciente, seCerroAbriendo } from './recientes.ts'

test('el último archivo queda primero y no se repite', () => {
  const a = { ruta: '/a.pdf', nombre: 'a.pdf' }
  const b = { ruta: '/b.pdf', nombre: 'b.pdf' }
  assert.deepEqual(agregarReciente([a, b], b), [b, a])
})

test('un archivo que ya no se puede abrir se quita de la lista', () => {
  const a = { ruta: '/a.pdf', nombre: 'a.pdf' }
  const b = { ruta: '/b.pdf', nombre: 'b.pdf' }
  assert.deepEqual(olvidarReciente([a, b], a.ruta), [b])
})

test('la lista guarda como máximo los últimos archivos', () => {
  let lista: { ruta: string; nombre: string }[] = []
  for (let i = 0; i < MAX_RECIENTES + 3; i++) {
    lista = agregarReciente(lista, { ruta: `/${i}.pdf`, nombre: `${i}.pdf` })
  }
  assert.equal(lista.length, MAX_RECIENTES)
  assert.equal(lista[0].ruta, `/${MAX_RECIENTES + 2}.pdf`)
})

test('si la app se cerró abriendo un PDF, al arrancar no se reabre solo', () => {
  const datos = new Map<string, string>()
  Object.assign(globalThis, {
    localStorage: {
      getItem: (k: string) => datos.get(k) ?? null,
      setItem: (k: string, v: string) => void datos.set(k, v),
      removeItem: (k: string) => void datos.delete(k),
    },
  })
  assert.equal(seCerroAbriendo(), false)
  marcarAbriendo(true)
  assert.equal(seCerroAbriendo(), true)
  marcarAbriendo(false)
  assert.equal(seCerroAbriendo(), false)
})

test('los nombres sacados del enlace se ven limpios', () => {
  assert.equal(nombreLegible('primary%3ADownload%2FFormulario-de-prueba.pdf'), 'Formulario-de-prueba.pdf')
  assert.equal(nombreLegible('document%3A9741 (anotado).pdf'), '9741 (anotado).pdf')
  assert.equal(nombreLegible('Tesis 100% final.pdf'), 'Tesis 100% final.pdf')
  assert.equal(nombreLegible('Informe final.pdf'), 'Informe final.pdf')
  assert.equal(nombreLegible('Reunión: acta.pdf'), 'Reunión: acta.pdf')
})

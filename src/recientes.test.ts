import assert from 'node:assert/strict'
import { test } from 'node:test'
import { agregarReciente, MAX_RECIENTES, olvidarReciente } from './recientes.ts'

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

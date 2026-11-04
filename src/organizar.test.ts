import assert from 'node:assert/strict'
import { test } from 'node:test'
import { moverMarcadas, type Ficha } from './organizar.ts'

const hojas = (n: number): Ficha[] => Array.from({ length: n }, (_, i) => ({ id: i, pagina: i, giro: 0 }))
const orden = (l: Ficha[]) => l.map((f) => f.id)

test('mover una hoja marcada antes y después', () => {
  assert.deepEqual(orden(moverMarcadas(hojas(4), new Set([2]), -1)), [0, 2, 1, 3])
  assert.deepEqual(orden(moverMarcadas(hojas(4), new Set([1]), 1)), [0, 2, 1, 3])
})

test('las marcadas juntas se mueven como bloque', () => {
  assert.deepEqual(orden(moverMarcadas(hojas(5), new Set([2, 3]), -1)), [0, 2, 3, 1, 4])
  assert.deepEqual(orden(moverMarcadas(hojas(5), new Set([1, 2]), 1)), [0, 3, 1, 2, 4])
})

test('al borde no se mueve ni se pierde ninguna hoja', () => {
  assert.deepEqual(orden(moverMarcadas(hojas(3), new Set([0]), -1)), [0, 1, 2])
  assert.deepEqual(orden(moverMarcadas(hojas(3), new Set([1, 2]), 1)), [0, 1, 2])
  // La 2 ya está al final; la 0 sí puede avanzar.
  assert.deepEqual(orden(moverMarcadas(hojas(3), new Set([0, 2]), 1)), [1, 0, 2])
})

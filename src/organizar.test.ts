import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ABIERTO, fichasDe, moverMarcadas, ordenarPorArchivo, type Ficha } from './organizar.ts'

const hojas = (n: number): Ficha[] => Array.from({ length: n }, (_, i) => ({ id: i, archivo: ABIERTO, pagina: i, giro: 0 }))
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

test('ordenar por archivo deja juntas las hojas de cada PDF sin cambiar su orden interno', () => {
  const a = { archivo: ABIERTO, nombre: 'a.pdf', paginas: 2 }
  const b = { archivo: 0, nombre: 'b.pdf', paginas: 2 }
  const [a1, a2] = fichasDe(a, 0)
  const [b1, b2] = fichasDe(b, 10)
  const mezcla = [b2, a2, b1, a1]
  assert.deepEqual(
    ordenarPorArchivo(mezcla, [a, b]).map((f) => `${f.archivo}:${f.pagina}`),
    ['-1:1', '-1:0', '0:1', '0:0'],
  )
})

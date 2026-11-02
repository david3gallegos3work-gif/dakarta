import assert from 'node:assert/strict'
import { test } from 'node:test'
import { esMasNueva } from './version.ts'

test('compara número por número, no como texto', () => {
  assert.equal(esMasNueva('0.10.0', '0.9.0'), true)
  assert.equal(esMasNueva('0.9.0', '0.10.0'), false)
  assert.equal(esMasNueva('1.0.0', '0.99.99'), true)
})

test('la misma versión no es más nueva', () => {
  assert.equal(esMasNueva('0.1.1', '0.1.1'), false)
  assert.equal(esMasNueva('0.1', '0.1.0'), false)
})

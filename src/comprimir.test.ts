import assert from 'node:assert/strict'
import { test } from 'node:test'
import { mensajeComprimido, pesoLegible } from './comprimir.ts'

test('peso legible con coma decimal', () => {
  assert.equal(pesoLegible(850 * 1024), '850 KB')
  assert.equal(pesoLegible(3.14 * 1024 * 1024), '3,1 MB')
  assert.equal(pesoLegible(25.6 * 1024 * 1024), '26 MB')
  assert.equal(pesoLegible(10), '1 KB')
})

test('mensaje al comprimir', () => {
  const mb = 1024 * 1024
  assert.equal(mensajeComprimido(12 * mb, 3 * mb), 'Copia comprimida guardada: de 12 MB a 3 MB (75 % menos).')
  assert.equal(mensajeComprimido(2 * mb, 1.98 * mb), 'Este PDF ya estaba comprimido: la copia pesa casi lo mismo (2 MB).')
  assert.equal(mensajeComprimido(0, 500 * 1024), 'Copia comprimida guardada (500 KB).')
})

import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cubeta, limitarZoom } from './zoom.ts'

test('cubeta redondea hacia arriba en escalones de 256 y respeta los topes', () => {
  assert.equal(cubeta(1080), 1280)
  assert.equal(cubeta(1024), 1024)
  assert.equal(cubeta(10), 256)
  assert.equal(cubeta(99999), 6144)
})

test('limitarZoom no sale del rango', () => {
  assert.equal(limitarZoom(0.01), 0.25)
  assert.equal(limitarZoom(100), 6)
  assert.equal(limitarZoom(1.5), 1.5)
})

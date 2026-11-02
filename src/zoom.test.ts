import assert from 'node:assert/strict'
import { test } from 'node:test'
import { anchoEntera, imanZoom, limitarZoom, recorte } from './zoom.ts'

test('imanZoom se queda en 100% solo cuando está muy cerca', () => {
  assert.equal(imanZoom(1.03), 1)
  assert.equal(imanZoom(0.96), 1)
  assert.equal(imanZoom(1.25), 1.25)
})

test('anchoEntera es exacto dentro de sus topes y nunca el de la miniatura', () => {
  assert.equal(anchoEntera(1080.4), 1080)
  assert.equal(anchoEntera(384), 512)
  assert.equal(anchoEntera(99999), 2048)
})

test('limitarZoom no sale del rango', () => {
  assert.equal(limitarZoom(0.01), 0.25)
  assert.equal(limitarZoom(100), 6)
  assert.equal(limitarZoom(1.5), 1.5)
})

const pagina = { x: 0, y: 0, w: 3000, h: 4000 }
const vista = { x: 1000, y: 1000, w: 360, h: 640 }

test('recorte pide lo visible más la holgura, en píxeles reales', () => {
  assert.deepEqual(recorte(7, pagina, vista, 3), { n: 7, W: 9000, H: 12000, x: 2730, y: 2520, cw: 1620, ch: 2880 })
})

test('recorte no se sale de la página', () => {
  assert.deepEqual(recorte(0, pagina, { x: 0, y: 0, w: 360, h: 640 }, 1), { n: 0, W: 3000, H: 4000, x: 0, y: 0, cw: 450, ch: 800 })
  const r = recorte(0, pagina, { x: 2800, y: 3500, w: 360, h: 640 }, 1)!
  assert.equal(r.x + r.cw, 3000)
  assert.equal(r.y + r.ch, 4000)
})

test('recorte es nulo si la página no se ve', () => {
  assert.equal(recorte(0, pagina, { x: 0, y: 5000, w: 360, h: 640 }, 3), null)
})

test('recorte reutiliza el anterior mientras cubra lo visible', () => {
  const r = recorte(7, pagina, vista, 3)!
  assert.equal(recorte(7, pagina, { ...vista, y: vista.y + 50 }, 3, r), r)
  assert.notEqual(recorte(7, pagina, { ...vista, y: vista.y + 400 }, 3, r), r)
})

test('recorte no reutiliza el anterior si cambió el zoom', () => {
  const r = recorte(7, pagina, vista, 3)!
  assert.notEqual(recorte(7, { ...pagina, w: 3100 }, vista, 3, r), r)
})

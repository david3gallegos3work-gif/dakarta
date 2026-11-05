import assert from 'node:assert/strict'
import { test } from 'node:test'
import { FOTO_ENTERA, achicar, arrastrarRecorte, girada, nombreDeFotos } from './fotos.ts'

test('la foto se achica hasta el lado máximo, sin agrandarse ni deformarse', () => {
  assert.deepEqual(achicar(4000, 3000, 2400), { ancho: 2400, alto: 1800 })
  assert.deepEqual(achicar(3000, 4000, 2400), { ancho: 1800, alto: 2400 })
  assert.deepEqual(achicar(800, 600, 2400), { ancho: 800, alto: 600 })
})

test('girar de lado intercambia ancho y alto', () => {
  assert.deepEqual(girada(400, 300, 90), { ancho: 300, alto: 400 })
  assert.deepEqual(girada(400, 300, 180), { ancho: 400, alto: 300 })
  assert.deepEqual(girada(400, 300, 270), { ancho: 300, alto: 400 })
})

test('nombre sugerido con la fecha', () => {
  assert.equal(nombreDeFotos(new Date(2026, 10, 5)), 'Fotos 2026-11-05.pdf')
})

test('recortar: las esquinas se mueven sin salirse de la foto ni cruzarse', () => {
  const r = arrastrarRecorte(FOTO_ENTERA, 'ai', 0.1, 0.2)
  assert.deepEqual(r, { x: 0.1, y: 0.2, w: 0.9, h: 0.8 })
  assert.deepEqual(arrastrarRecorte(r, 'bd', 0.5, 0.5), { x: 0.1, y: 0.2, w: 0.9, h: 0.8 })
  const chico = arrastrarRecorte(r, 'ai', 2, 2)
  assert.ok(Math.abs(chico.w - 0.05) < 1e-9 && Math.abs(chico.h - 0.05) < 1e-9)
  const movido = arrastrarRecorte({ x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, 'todo', 0.9, -0.9)
  assert.deepEqual(movido, { x: 0.5, y: 0, w: 0.5, h: 0.5 })
})

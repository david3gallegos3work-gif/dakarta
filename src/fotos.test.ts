import assert from 'node:assert/strict'
import { test } from 'node:test'
import { FOTO_ENTERA, achicar, arrastrarRecorte, deRectanguloA, esRectangulo, girada, medidaEnderezada, nombreDeFotos } from './fotos.ts'

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

test('recortar: cada esquina se mueve sola y sin salirse de la foto', () => {
  const r = arrastrarRecorte(FOTO_ENTERA, 0, 0.1, 0.2)
  assert.deepEqual(r[0], { x: 0.1, y: 0.2 })
  assert.deepEqual(r.slice(1), FOTO_ENTERA.slice(1))
  assert.deepEqual(arrastrarRecorte(r, 2, 0.5, 0.5)[2], { x: 1, y: 1 })
  const chico = [
    { x: 0.2, y: 0.2 },
    { x: 0.6, y: 0.2 },
    { x: 0.6, y: 0.6 },
    { x: 0.2, y: 0.6 },
  ] as typeof r
  const movido = arrastrarRecorte(chico, 'todo', 0.9, -0.9)
  assert.ok(Math.abs(movido[0].x - 0.6) < 1e-9 && movido[0].y === 0)
  assert.ok(movido.every((p) => p.x >= 0 && p.x <= 1 && p.y >= 0 && p.y <= 1))
  assert.equal(esRectangulo(FOTO_ENTERA), true)
  assert.equal(esRectangulo(r), false)
})

test('enderezar: las esquinas del documento van a las del rectángulo y el centro a su lugar', () => {
  // Un documento tomado de medio lado: más angosto arriba que abajo.
  const q = [
    { x: 0.3, y: 0.2 },
    { x: 0.7, y: 0.2 },
    { x: 0.9, y: 0.8 },
    { x: 0.1, y: 0.8 },
  ] as Parameters<typeof deRectanguloA>[0]
  const f = deRectanguloA(q)
  const cerca = (a: { x: number; y: number }, b: { x: number; y: number }) =>
    Math.abs(a.x - b.x) < 1e-9 && Math.abs(a.y - b.y) < 1e-9
  assert.ok(cerca(f(0, 0), q[0]) && cerca(f(1, 0), q[1]) && cerca(f(1, 1), q[2]) && cerca(f(0, 1), q[3]))
  // Simétrico: el centro del documento queda en el medio a lo ancho.
  assert.ok(Math.abs(f(0.5, 0.5).x - 0.5) < 1e-9)
  // Un rectángulo derecho se lleva tal cual.
  const g = deRectanguloA(FOTO_ENTERA)
  assert.ok(cerca(g(0.25, 0.75), { x: 0.25, y: 0.75 }))
  assert.deepEqual(medidaEnderezada(q, 1000, 1000), { ancho: 800, alto: 632 })
})

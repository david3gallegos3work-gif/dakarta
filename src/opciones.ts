/** Colores, tamaños y sellos que ofrecen las herramientas de edición. */

import type { Anotacion } from './tipos.ts'

export const COLORES_RESALTADO: ReadonlyArray<readonly [string, string]> = [
  ['#ffeb3b', 'Amarillo'],
  ['#a5d6a7', 'Verde'],
  ['#f8bbd0', 'Rosado'],
  ['#90caf9', 'Celeste'],
]
export const COLORES_FUERTES: ReadonlyArray<readonly [string, string]> = [
  ['#000000', 'Negro'],
  ['#d32f2f', 'Rojo'],
  ['#1565c0', 'Azul'],
  ['#2e7d32', 'Verde'],
  ['#6a1b9a', 'Morado'],
]
export const COLORES_SELLO: ReadonlyArray<readonly [string, string]> = [
  ['#2e7d32', 'Verde'],
  ['#1565c0', 'Azul'],
  ['#c62828', 'Rojo'],
  ['#6a1b9a', 'Morado'],
]
export const SELLOS: ReadonlyArray<readonly [string, string]> = [
  ['APROBADO', '#2e7d32'],
  ['RECIBIDO', '#1565c0'],
  ['PAGADO', '#2e7d32'],
  ['CONFIDENCIAL', '#c62828'],
]
export const GROSORES: ReadonlyArray<readonly [number, string]> = [
  [1.5, 'Fino'],
  [3, 'Medio'],
  [6, 'Grueso'],
]
export const TAMANOS_TEXTO: ReadonlyArray<readonly [number, string]> = [
  [10, 'Pequeño'],
  [12, 'Normal'],
  [18, 'Grande'],
  [24, 'Título'],
]
export const TAMANOS_SELLO: ReadonlyArray<readonly [number, string]> = [
  [18, 'Normal'],
  [24, 'Grande'],
  [32, 'Muy grande'],
]
export const clamp01 = (v: number) => Math.min(1, Math.max(0, v))
export const rgb = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16)
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}
export const hex = (r: number, g: number, b: number) =>
  '#' +
  [r, g, b]
    .map((v) =>
      Math.round(clamp01(v) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')

export const TITULOS_ANOTACION: Record<Anotacion['tipo'], string> = {
  nota: 'Nota',
  texto: 'Texto o nota',
  tinta: 'Trazo o firma',
  resaltar: 'Resaltado',
  subrayar: 'Subrayado',
  tachar: 'Tachado',
}

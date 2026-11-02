export const ZOOM_MIN = 0.25
export const ZOOM_MAX = 6
/** Ancho de las miniaturas que se ven mientras se desplaza (el mismo valor en motor.rs). */
export const MINIATURA = 384
/** Hasta este ancho se pide la hoja entera; más allá, solo el pedazo que se está mirando. */
export const ANCHO_ENTERA_MAX = 2048
/** Lo que se pide de más alrededor de lo visible (fracción de la pantalla): mover un poco no obliga a pintar de nuevo. */
const HOLGURA = 0.25

export type Caja = { x: number; y: number; w: number; h: number }
/** Pedazo de la página n en píxeles reales; W y H: tamaño de la página entera a ese zoom. */
export type Recorte = { n: number; W: number; H: number; x: number; y: number; cw: number; ch: number }

export const limitarZoom = (z: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z))

/** Cerca del 100% se queda justo en 100%: así es fácil volver a ver la página entera. */
export const imanZoom = (z: number) => (Math.abs(z - 1) < 0.05 ? 1 : z)

/** Ancho de la hoja entera que se pide al motor: exacto para que se vea nítida; nunca igual al de la miniatura. */
export const anchoEntera = (px: number) => Math.min(ANCHO_ENTERA_MAX, Math.max(512, Math.round(px)))

/** Pedazo de la página a pedir (nulo si no se ve). Si el anterior aún cubre lo visible, se devuelve ese mismo. */
export function recorte(n: number, pagina: Caja, vista: Caja, dpr: number, previo?: Recorte): Recorte | null {
  const W = Math.round(pagina.w * dpr)
  const H = Math.round(pagina.h * dpr)
  const x0 = Math.max(vista.x, pagina.x) - pagina.x
  const y0 = Math.max(vista.y, pagina.y) - pagina.y
  const x1 = Math.min(vista.x + vista.w, pagina.x + pagina.w) - pagina.x
  const y1 = Math.min(vista.y + vista.h, pagina.y + pagina.h) - pagina.y
  if (x1 <= x0 || y1 <= y0) return null
  if (
    previo &&
    previo.W === W &&
    previo.H === H &&
    previo.x <= x0 * dpr &&
    previo.y <= y0 * dpr &&
    previo.x + previo.cw >= x1 * dpr &&
    previo.y + previo.ch >= y1 * dpr
  ) {
    return previo
  }
  const x = Math.floor(Math.max(0, x0 - vista.w * HOLGURA) * dpr)
  const y = Math.floor(Math.max(0, y0 - vista.h * HOLGURA) * dpr)
  const cw = Math.min(W, Math.ceil((x1 + vista.w * HOLGURA) * dpr)) - x
  const ch = Math.min(H, Math.ceil((y1 + vista.h * HOLGURA) * dpr)) - y
  return { n, W, H, x, y, cw, ch }
}

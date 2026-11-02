export const ZOOM_MIN = 0.25
export const ZOOM_MAX = 6

export const limitarZoom = (z: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z))

/** Ancho en píxeles que se pide al motor: en escalones de 256 para que zooms parecidos reutilicen la misma imagen. */
export const cubeta = (px: number) => Math.min(6144, Math.max(256, Math.ceil(px / 256) * 256))

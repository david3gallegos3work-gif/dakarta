export type Reciente = { ruta: string; nombre: string }

const CLAVE = 'recientes'
export const MAX_RECIENTES = 10

/**
 * Nombre para mostrar. Cuando Android no da el nombre, se saca del enlace y queda como
 * «primary%3ADownload%2FInforme.pdf»: se descifra y se deja solo lo último («Informe.pdf»).
 */
export function nombreLegible(nombre: string): string {
  let n = nombre
  try {
    n = decodeURIComponent(n)
  } catch {
    // ya venía descifrado o trae un % suelto: se usa tal cual
  }
  // Los dos puntos solo separan carpetas en un enlace cifrado («primary:Download/…»); un nombre real puede tenerlos.
  const separadores = n === nombre ? /[/\\]/ : /[/\\:]/
  return n.split(separadores).filter(Boolean).pop() || nombre
}

/** El más nuevo primero; si ya estaba, se mueve al inicio. */
export function agregarReciente(lista: Reciente[], nuevo: Reciente): Reciente[] {
  const limpio = { ...nuevo, nombre: nombreLegible(nuevo.nombre) }
  return [limpio, ...lista.filter((r) => r.ruta !== nuevo.ruta)].slice(0, MAX_RECIENTES)
}

export function leerRecientes(): Reciente[] {
  try {
    const guardado: unknown = JSON.parse(localStorage.getItem(CLAVE) ?? '[]')
    if (!Array.isArray(guardado)) return []
    return guardado
      .filter((r): r is Reciente => typeof r?.ruta === 'string' && typeof r?.nombre === 'string')
      .map((r) => ({ ruta: r.ruta, nombre: nombreLegible(r.nombre) }))
      .slice(0, MAX_RECIENTES)
  } catch {
    return []
  }
}

function guardar(lista: Reciente[]): Reciente[] {
  try {
    localStorage.setItem(CLAVE, JSON.stringify(lista))
  } catch {
    // sin almacenamiento: la lista no se recuerda al volver a abrir
  }
  return lista
}

export function guardarReciente(lista: Reciente[], nuevo: Reciente): Reciente[] {
  return guardar(agregarReciente(lista, nuevo))
}

/** Un archivo que ya no se puede abrir se quita, para que no falle en cada arranque. */
export function olvidarReciente(lista: Reciente[], ruta: string): Reciente[] {
  return guardar(lista.filter((r) => r.ruta !== ruta))
}

const ABRIENDO = 'abriendo'

/** Se pone antes de abrir un PDF y se quita cuando ya se ve. Si sigue puesta al arrancar, ese PDF cerró la app. */
export function marcarAbriendo(si: boolean) {
  try {
    if (si) localStorage.setItem(ABRIENDO, '1')
    else localStorage.removeItem(ABRIENDO)
  } catch {
    // sin almacenamiento: se reabre el último como siempre
  }
}

export function seCerroAbriendo(): boolean {
  try {
    return localStorage.getItem(ABRIENDO) === '1'
  } catch {
    return false
  }
}

export type Reciente = { ruta: string; nombre: string }

const CLAVE = 'recientes'
export const MAX_RECIENTES = 10

/** El más nuevo primero; si ya estaba, se mueve al inicio. */
export function agregarReciente(lista: Reciente[], nuevo: Reciente): Reciente[] {
  return [nuevo, ...lista.filter((r) => r.ruta !== nuevo.ruta)].slice(0, MAX_RECIENTES)
}

export function leerRecientes(): Reciente[] {
  try {
    const guardado: unknown = JSON.parse(localStorage.getItem(CLAVE) ?? '[]')
    if (!Array.isArray(guardado)) return []
    return guardado
      .filter((r): r is Reciente => typeof r?.ruta === 'string' && typeof r?.nombre === 'string')
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

/** La hoja sale del PDF abierto (los demás números son PDFs agregados para unir). */
export const ABIERTO = -1

/** Un PDF cuyas hojas están en la pantalla: el abierto o uno agregado para unir. */
export type Fuente = { archivo: number; nombre: string; paginas: number }

/** Una hoja en la pantalla de organizar: `id` la distingue aunque sea copia de la misma página. */
export type Ficha = { id: number; archivo: number; pagina: number; giro: number }

/** Las hojas de un PDF en su orden, con ids desde `desde`. */
export function fichasDe(f: Fuente, desde: number): Ficha[] {
  return Array.from({ length: f.paginas }, (_, i) => ({ id: desde + i, archivo: f.archivo, pagina: i, giro: 0 }))
}

/** «Ordenar por archivo»: todas las del primer PDF, luego las del segundo…; dentro de cada uno, como estaban. */
export function ordenarPorArchivo(lista: Ficha[], fuentes: Fuente[]): Ficha[] {
  const lugar = (f: Ficha) => fuentes.findIndex((x) => x.archivo === f.archivo)
  return lista.slice().sort((a, b) => lugar(a) - lugar(b))
}

/**
 * Mueve las hojas marcadas un lugar antes (-1) o después (1). Las marcadas que van juntas se mueven
 * como bloque; una que ya está al borde, o pegada a otra marcada que no puede moverse, se queda.
 */
export function moverMarcadas(lista: Ficha[], marcadas: Set<number>, paso: -1 | 1): Ficha[] {
  const nueva = lista.slice()
  const indices = nueva.map((_, i) => i)
  if (paso === 1) indices.reverse()
  for (const i of indices) {
    const j = i + paso
    if (!marcadas.has(nueva[i].id) || j < 0 || j >= nueva.length || marcadas.has(nueva[j].id)) continue
    ;[nueva[i], nueva[j]] = [nueva[j], nueva[i]]
  }
  return nueva
}

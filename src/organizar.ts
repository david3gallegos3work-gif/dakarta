/** Una hoja en la pantalla de organizar: `id` la distingue aunque sea copia de la misma página. */
export type Ficha = { id: number; pagina: number; giro: number }

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

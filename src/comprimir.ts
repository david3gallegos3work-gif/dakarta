/** Cuánto se comprime: buena calidad (para imprimir) o menos peso (para enviar). */
export type Nivel = 'ligera' | 'fuerte'

/** Peso legible en español: «850 KB», «3,1 MB». */
export function pesoLegible(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`
  const mb = bytes / (1024 * 1024)
  return `${mb.toLocaleString('es', { maximumFractionDigits: mb < 10 ? 1 : 0 })} MB`
}

/** Lo que se le dice a la persona al terminar de comprimir. */
export function mensajeComprimido(antes: number, despues: number): string {
  // Sin el peso del original (no se pudo leer) solo se dice cuánto pesa la copia.
  if (!antes) return `Copia comprimida guardada (${pesoLegible(despues)}).`
  const menos = Math.round((1 - despues / antes) * 100)
  if (menos < 5) return `Este PDF ya estaba comprimido: la copia pesa casi lo mismo (${pesoLegible(despues)}).`
  return `Copia comprimida guardada: de ${pesoLegible(antes)} a ${pesoLegible(despues)} (${menos} % menos).`
}

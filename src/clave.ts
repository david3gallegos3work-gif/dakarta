/** El motor avisa así que el PDF pide contraseña (`CLAVE:PIDE`) o que la que se dio no sirve (`CLAVE:MAL`). */
export const PIDE_CLAVE = 'CLAVE:PIDE'
export const CLAVE_MAL = 'CLAVE:MAL'

export const esErrorClave = (e: unknown) => String(e) === PIDE_CLAVE || String(e) === CLAVE_MAL

/** MuPDF acepta contraseñas de hasta 127 bytes. */
export const LARGO_MAXIMO = 127

/** Qué le falta a una contraseña nueva (escrita dos veces); vacío si está bien. */
export function problemaClave(clave: string, otraVez: string): string {
  if (!clave) return 'Escribe la contraseña.'
  if (new TextEncoder().encode(clave).length > LARGO_MAXIMO) return 'La contraseña es demasiado larga.'
  if (clave !== otraVez) return 'Las dos contraseñas no son iguales.'
  return ''
}

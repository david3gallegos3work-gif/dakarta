/** Lado más largo de la foto en el PDF: nítida al imprimir en A4 (unos 280 ppp) sin pesar de más. */
export const LADO_MAXIMO = 2400
/** Lado más largo de la miniatura en pantalla. */
export const LADO_MINIATURA = 320

/** Medida de la imagen achicada (nunca agrandada) para que su lado más largo no pase de `maximo`. */
export function achicar(ancho: number, alto: number, maximo: number): { ancho: number; alto: number } {
  const escala = Math.min(1, maximo / Math.max(ancho, alto))
  return { ancho: Math.max(1, Math.round(ancho * escala)), alto: Math.max(1, Math.round(alto * escala)) }
}

/** Medida del lienzo tras girar la foto `giro` grados (múltiplo de 90). */
export function girada(ancho: number, alto: number, giro: number): { ancho: number; alto: number } {
  return giro % 180 === 0 ? { ancho, alto } : { ancho: alto, alto: ancho }
}

/** Pinta la foto achicada y girada en un lienzo y lo entrega como JPEG. El navegador ya la endereza (EXIF). */
async function pintarFoto(archivo: Blob, maximo: number, giro: number, calidad: number): Promise<Blob> {
  const imagen = await createImageBitmap(archivo)
  try {
    const { ancho, alto } = achicar(imagen.width, imagen.height, maximo)
    const lienzo = document.createElement('canvas')
    const final = girada(ancho, alto, giro)
    lienzo.width = final.ancho
    lienzo.height = final.alto
    const g = lienzo.getContext('2d')
    if (!g) throw new Error('Sin lienzo')
    g.fillStyle = '#fff'
    g.fillRect(0, 0, final.ancho, final.alto)
    g.translate(final.ancho / 2, final.alto / 2)
    g.rotate((giro * Math.PI) / 180)
    g.drawImage(imagen, -ancho / 2, -alto / 2, ancho, alto)
    const jpg = await new Promise<Blob | null>((listo) => lienzo.toBlob(listo, 'image/jpeg', calidad))
    if (!jpg) throw new Error('Sin JPEG')
    return jpg
  } finally {
    imagen.close()
  }
}

/** Miniatura para la pantalla (una dirección blob: que se suelta al cerrar). */
export async function miniaturaDe(archivo: Blob): Promise<string> {
  return URL.createObjectURL(await pintarFoto(archivo, LADO_MINIATURA, 0, 0.8))
}

/** La foto lista para el PDF: enderezada, girada, achicada y en JPEG. */
export async function jpegDe(archivo: Blob, giro: number): Promise<Uint8Array> {
  return new Uint8Array(await (await pintarFoto(archivo, LADO_MAXIMO, ((giro % 360) + 360) % 360, 0.85)).arrayBuffer())
}

/** Nombre sugerido: «Fotos 2026-11-05.pdf». */
export function nombreDeFotos(fecha = new Date()): string {
  const d = (n: number) => String(n).padStart(2, '0')
  return `Fotos ${fecha.getFullYear()}-${d(fecha.getMonth() + 1)}-${d(fecha.getDate())}.pdf`
}

/** Cómo van las fotos en el PDF: una por hoja A4, una por hoja con su forma, o documentos a tamaño real. */
export type Diseno = 'a4' | 'foto' | 'documentos'

/** Parte de la foto que se usa, en fracciones (0 a 1) de la foto derecha (antes de girarla). */
export type Recorte = { x: number; y: number; w: number; h: number }
export const FOTO_ENTERA: Recorte = { x: 0, y: 0, w: 1, h: 1 }
/** El recorte más chico: un 5 % de la foto por lado. */
const MINIMO = 0.05

/** Qué se arrastra en el recorte: una esquina (arriba/abajo, izquierda/derecha) o todo el recuadro. */
export type Agarre = 'ai' | 'ad' | 'bi' | 'bd' | 'todo'

/** Mueve una esquina (o todo el recuadro) `dx`, `dy` (en fracciones), sin salirse de la foto ni achicarse de más. */
export function arrastrarRecorte(r: Recorte, agarre: Agarre, dx: number, dy: number): Recorte {
  if (agarre === 'todo') {
    return { ...r, x: Math.min(Math.max(0, r.x + dx), 1 - r.w), y: Math.min(Math.max(0, r.y + dy), 1 - r.h) }
  }
  let [x0, y0, x1, y1] = [r.x, r.y, r.x + r.w, r.y + r.h]
  if (agarre[1] === 'i') x0 = Math.min(Math.max(0, x0 + dx), x1 - MINIMO)
  else x1 = Math.max(Math.min(1, x1 + dx), x0 + MINIMO)
  if (agarre[0] === 'a') y0 = Math.min(Math.max(0, y0 + dy), y1 - MINIMO)
  else y1 = Math.max(Math.min(1, y1 + dy), y0 + MINIMO)
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

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
async function pintarFoto(archivo: Blob, maximo: number, giro: number, calidad: number, r = FOTO_ENTERA): Promise<Blob> {
  const imagen = await createImageBitmap(archivo)
  try {
    const [sx, sy, sw, sh] = [r.x * imagen.width, r.y * imagen.height, r.w * imagen.width, r.h * imagen.height]
    const { ancho, alto } = achicar(sw, sh, maximo)
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
    g.drawImage(imagen, sx, sy, sw, sh, -ancho / 2, -alto / 2, ancho, alto)
    const jpg = await new Promise<Blob | null>((listo) => lienzo.toBlob(listo, 'image/jpeg', calidad))
    if (!jpg) throw new Error('Sin JPEG')
    return jpg
  } finally {
    imagen.close()
  }
}

/** Miniatura para la pantalla, ya recortada (una dirección blob: que se suelta al cerrar). */
export async function miniaturaDe(archivo: Blob, recorte?: Recorte): Promise<string> {
  return URL.createObjectURL(await pintarFoto(archivo, LADO_MINIATURA, 0, 0.8, recorte))
}

/** La foto lista para el PDF: enderezada, recortada, girada, achicada y en JPEG. */
export async function jpegDe(archivo: Blob, giro: number, recorte?: Recorte): Promise<Uint8Array> {
  const g = ((giro % 360) + 360) % 360
  return new Uint8Array(await (await pintarFoto(archivo, LADO_MAXIMO, g, 0.85, recorte)).arrayBuffer())
}

/** Nombre sugerido: «Fotos 2026-11-05.pdf». */
export function nombreDeFotos(fecha = new Date()): string {
  const d = (n: number) => String(n).padStart(2, '0')
  return `Fotos ${fecha.getFullYear()}-${d(fecha.getMonth() + 1)}-${d(fecha.getDate())}.pdf`
}

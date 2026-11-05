/** Cómo van las fotos en el PDF: una por hoja A4, una por hoja con su forma, o documentos a tamaño real. */
export type Diseno = 'a4' | 'foto' | 'documentos'

export type Punto = { x: number; y: number }
/**
 * Las cuatro esquinas del documento en la foto derecha (antes de girarla), en fracciones (0 a 1):
 * arriba a la izquierda, arriba a la derecha, abajo a la derecha y abajo a la izquierda. Si la foto se
 * tomó torcida o de medio lado, el documento se endereza a un rectángulo.
 */
export type Recorte = [Punto, Punto, Punto, Punto]
export const FOTO_ENTERA: Recorte = [
  { x: 0, y: 0 },
  { x: 1, y: 0 },
  { x: 1, y: 1 },
  { x: 0, y: 1 },
]

/** Qué se arrastra en el recorte: una esquina (su número) o todo el recuadro. */
export type Agarre = 0 | 1 | 2 | 3 | 'todo'

const entre = (v: number, a: number, b: number) => Math.min(Math.max(v, a), b)

/** Mueve una esquina (o todo el recuadro) `dx`, `dy` (en fracciones), sin salirse de la foto. */
export function arrastrarRecorte(r: Recorte, agarre: Agarre, dx: number, dy: number): Recorte {
  if (agarre === 'todo') {
    // Todo el recuadro: lo que pueda moverse sin que ninguna esquina salga de la foto.
    const xs = r.map((p) => p.x)
    const ys = r.map((p) => p.y)
    const mx = entre(dx, -Math.min(...xs), 1 - Math.max(...xs))
    const my = entre(dy, -Math.min(...ys), 1 - Math.max(...ys))
    return r.map((p) => ({ x: p.x + mx, y: p.y + my })) as Recorte
  }
  return r.map((p, i) => (i === agarre ? { x: entre(p.x + dx, 0, 1), y: entre(p.y + dy, 0, 1) } : p)) as Recorte
}

/** El recorte es un rectángulo derecho (no hace falta enderezar). */
export function esRectangulo(r: Recorte): boolean {
  const casi = (a: number, b: number) => Math.abs(a - b) < 1e-6
  return casi(r[0].y, r[1].y) && casi(r[2].y, r[3].y) && casi(r[0].x, r[3].x) && casi(r[1].x, r[2].x)
}

/**
 * Lleva un punto (u, v) del rectángulo final (0 a 1) a su lugar en el cuadrilátero `q` (perspectiva).
 * Así se sabe de qué punto de la foto sale cada punto del documento enderezado.
 */
export function deRectanguloA(q: Recorte): (u: number, v: number) => Punto {
  const [p0, p1, p2, p3] = q
  const dx1 = p1.x - p2.x
  const dx2 = p3.x - p2.x
  const dx3 = p0.x - p1.x + p2.x - p3.x
  const dy1 = p1.y - p2.y
  const dy2 = p3.y - p2.y
  const dy3 = p0.y - p1.y + p2.y - p3.y
  let g = 0
  let h = 0
  const det = dx1 * dy2 - dx2 * dy1
  if ((Math.abs(dx3) > 1e-12 || Math.abs(dy3) > 1e-12) && Math.abs(det) > 1e-12) {
    g = (dx3 * dy2 - dx2 * dy3) / det
    h = (dx1 * dy3 - dx3 * dy1) / det
  }
  const a = p1.x - p0.x + g * p1.x
  const b = p3.x - p0.x + h * p3.x
  const d = p1.y - p0.y + g * p1.y
  const e = p3.y - p0.y + h * p3.y
  return (u, v) => {
    const w = g * u + h * v + 1
    return { x: (a * u + b * v + p0.x) / w, y: (d * u + e * v + p0.y) / w }
  }
}

/** Ancho y alto (en píxeles de la foto) del documento enderezado: el lado más largo de cada par. */
export function medidaEnderezada(q: Recorte, ancho: number, alto: number): { ancho: number; alto: number } {
  const largo = (a: Punto, b: Punto) => Math.hypot((a.x - b.x) * ancho, (a.y - b.y) * alto)
  return {
    ancho: Math.max(1, Math.round(Math.max(largo(q[0], q[1]), largo(q[3], q[2])))),
    alto: Math.max(1, Math.round(Math.max(largo(q[0], q[3]), largo(q[1], q[2])))),
  }
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

function lienzo(ancho: number, alto: number): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement('canvas')
  c.width = ancho
  c.height = alto
  const g = c.getContext('2d')
  if (!g) throw new Error('Sin lienzo')
  return [c, g]
}

/** Endereza el documento: cada píxel del rectángulo final se toma de su lugar en la foto (con suavizado). */
function enderezar(imagen: ImageBitmap, q: Recorte, maximo: number): HTMLCanvasElement {
  // La foto se lee a un tamaño algo mayor que el final: alcanza para la nitidez y ahorra memoria.
  const fuente = achicar(imagen.width, imagen.height, Math.min(4000, Math.round(maximo * 1.6)))
  const [cf, gf] = lienzo(fuente.ancho, fuente.alto)
  gf.drawImage(imagen, 0, 0, fuente.ancho, fuente.alto)
  const origen = gf.getImageData(0, 0, fuente.ancho, fuente.alto).data
  cf.width = cf.height = 0
  const m = medidaEnderezada(q, fuente.ancho, fuente.alto)
  const { ancho, alto } = achicar(m.ancho, m.alto, maximo)
  const [cd, gd] = lienzo(ancho, alto)
  const salida = gd.createImageData(ancho, alto)
  const d = salida.data
  const llevar = deRectanguloA(q)
  const W = fuente.ancho
  const H = fuente.alto
  for (let j = 0; j < alto; j++) {
    for (let i = 0; i < ancho; i++) {
      const p = llevar((i + 0.5) / ancho, (j + 0.5) / alto)
      const x = entre(p.x * W - 0.5, 0, W - 1)
      const y = entre(p.y * H - 0.5, 0, H - 1)
      const x0 = Math.floor(x)
      const y0 = Math.floor(y)
      const x1 = Math.min(x0 + 1, W - 1)
      const y1 = Math.min(y0 + 1, H - 1)
      const fx = x - x0
      const fy = y - y0
      const a = (y0 * W + x0) * 4
      const b = (y0 * W + x1) * 4
      const c = (y1 * W + x0) * 4
      const e = (y1 * W + x1) * 4
      const k = (j * ancho + i) * 4
      for (let n = 0; n < 3; n++) {
        const arriba = origen[a + n] + (origen[b + n] - origen[a + n]) * fx
        const abajo = origen[c + n] + (origen[e + n] - origen[c + n]) * fx
        d[k + n] = arriba + (abajo - arriba) * fy
      }
      d[k + 3] = 255
    }
  }
  gd.putImageData(salida, 0, 0)
  return cd
}

/** Recorta (y endereza si hace falta), gira y achica la foto, y la entrega como JPEG. El navegador ya la endereza (EXIF). */
async function pintarFoto(archivo: Blob, maximo: number, giro: number, calidad: number, r = FOTO_ENTERA): Promise<Blob> {
  const imagen = await createImageBitmap(archivo)
  try {
    let pieza: CanvasImageSource = imagen
    let sx = 0
    let sy = 0
    let sw = imagen.width
    let sh = imagen.height
    if (esRectangulo(r)) {
      sx = r[0].x * imagen.width
      sy = r[0].y * imagen.height
      sw = (r[1].x - r[0].x) * imagen.width
      sh = (r[3].y - r[0].y) * imagen.height
    } else {
      const c = enderezar(imagen, r, maximo)
      pieza = c
      sw = c.width
      sh = c.height
    }
    const { ancho, alto } = achicar(sw, sh, maximo)
    const final = girada(ancho, alto, giro)
    const [c, g] = lienzo(final.ancho, final.alto)
    g.fillStyle = '#fff'
    g.fillRect(0, 0, final.ancho, final.alto)
    g.translate(final.ancho / 2, final.alto / 2)
    g.rotate((giro * Math.PI) / 180)
    g.drawImage(pieza, sx, sy, sw, sh, -ancho / 2, -alto / 2, ancho, alto)
    const jpg = await new Promise<Blob | null>((listo) => c.toBlob(listo, 'image/jpeg', calidad))
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

/** Tipos que comparten la app y sus piezas. */

declare global {
  interface Window {
    /** Solo en Android (scripts/parchar-android.py): el PDF que llegó por "Abrir con", su nombre y el selector. */
    DakartaAndroid?: {
      pendiente(): string | null
      nombre(uri: string): string | null
      persistir(uri: string): void
      elegir?(): void
      compartir?(uri: string, nombre: string): void
      guardar?(nombre: string): void
      destino?(): string | null
      copiar?(destino: string, origen: string): boolean
      borrar?(destino: string): void
      salir?(): void
    }
    /** Android: el botón o gesto "atrás" pregunta si la pantalla lo usó o si toca salir. */
    DakartaAtras?: () => boolean
  }
}

/** Pregunta con el cuadro de la app y no con confirm(): el aviso de Android congela la pantalla. */
export type Pregunta = { titulo: string; texto: string; si: string; no: string; alSi: () => void }

export type Entrada = { titulo: string; pagina: number | null; hijos: Entrada[] }
export type Info = {
  paginas: number
  ancho: number
  alto: number
  indice: Entrada[]
  ms: number
  /** Solo un PDF se puede anotar, organizar y guardar; Word, EPUB, TXT… son para leer. */
  pdf: boolean
  /** Para volver a abrirlo después: una ruta en Windows, un content:// en Android. */
  ruta: string
  nombre: string
}
export type Doc = { info: Info; id: number }
export type Coincidencia = { pagina: number; x: number; y: number; w: number; h: number }
export type Busqueda = { texto: string; resultados: Coincidencia[]; escaneado: boolean; truncada: boolean }
/** Punto de una página que debe quedar en el mismo lugar de la pantalla tras un zoom o un giro. */
export type Ancla = { i: number; fx: number; fy: number; vx: number; vy: number }
export type Rango = [number, number]
export type Herramienta =
  | 'mover'
  | 'resaltar'
  | 'subrayar'
  | 'tachar'
  | 'lapiz'
  | 'nota'
  | 'texto'
  | 'firma'
  | 'formulario'
  | 'sello'
export type Campo = {
  pagina: number
  xref: number
  tipo: string
  nombre: string
  valor: string
  readonly: boolean
  /** Casillas: el valor que las deja marcadas en este PDF ("Off" es desmarcada). */
  encendido: string
  x: number
  y: number
  w: number
  h: number
}
/** Palabra del PDF para la capa de texto que permite seleccionar y copiar. */
/** Enlace del PDF a otra de sus páginas (las líneas de un índice de Word): se puede tocar para saltar. */
export type Enlace = { pagina: number; x: number; y: number; w: number; h: number }
export type Palabra = {
  texto: string
  x: number
  y: number
  w: number
  h: number
}
/** Anotación propia que se puede tocar para mover, cambiar o borrar. */
export type Anotacion = {
  pagina: number
  xref: number
  tipo: 'nota' | 'texto' | 'tinta' | 'resaltar' | 'subrayar' | 'tachar'
  contenido: string
  rojo: number
  verde: number
  azul: number
  tamano: number
  x: number
  y: number
  w: number
  h: number
}
export type Preview = {
  left: number
  top: number
  width: number
  height: number
  modo: 'texto' | 'tinta'
  x0: number
  y0: number
  x1: number
  y1: number
  puntos: [number, number][]
}

/** Una hoja del PDF que se guarda al organizar: de qué página sale y cuánto se gira (múltiplo de 90). */
export type Hoja = { pagina: number; giro: number }

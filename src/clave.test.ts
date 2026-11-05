import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CLAVE_MAL, PIDE_CLAVE, esErrorClave, problemaClave } from './clave.ts'

test('reconoce los avisos de contraseña del motor', () => {
  assert.equal(esErrorClave(PIDE_CLAVE), true)
  assert.equal(esErrorClave(CLAVE_MAL), true)
  assert.equal(esErrorClave('No se pudo abrir este archivo'), false)
  assert.equal(esErrorClave(new Error('CLAVE:PIDE')), false)
})

test('una contraseña nueva debe estar escrita dos veces igual y no pasar de 127 bytes', () => {
  assert.equal(problemaClave('', ''), 'Escribe la contraseña.')
  assert.equal(problemaClave('niño', 'nino'), 'Las dos contraseñas no son iguales.')
  assert.equal(problemaClave('niño', 'niño'), '')
  assert.equal(problemaClave('a'.repeat(127), 'a'.repeat(127)), '')
  // 64 eñes son 128 bytes
  assert.equal(problemaClave('ñ'.repeat(64), 'ñ'.repeat(64)), 'La contraseña es demasiado larga.')
})

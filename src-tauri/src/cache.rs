use std::collections::VecDeque;

/// Últimas páginas pintadas, con tope de memoria: volver a una es instantáneo. Sale primero la que lleva más tiempo sin usarse.
pub struct Cache {
    tope: usize,
    total: usize,
    items: VecDeque<((i32, u32), Vec<u8>)>,
}

impl Cache {
    pub fn nuevo(tope: usize) -> Self {
        Cache { tope, total: 0, items: VecDeque::new() }
    }

    pub fn leer(&mut self, clave: (i32, u32)) -> Option<Vec<u8>> {
        let i = self.items.iter().position(|(c, _)| *c == clave)?;
        let item = self.items.remove(i)?;
        let datos = item.1.clone();
        self.items.push_back(item);
        Some(datos)
    }

    pub fn guardar(&mut self, clave: (i32, u32), datos: Vec<u8>) {
        self.total += datos.len();
        self.items.push_back((clave, datos));
        while self.total > self.tope {
            let Some((_, viejo)) = self.items.pop_front() else { break };
            self.total -= viejo.len();
        }
    }

    pub fn vaciar(&mut self) {
        self.items.clear();
        self.total = 0;
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn devuelve_lo_guardado() {
        let mut c = Cache::nuevo(10);
        c.guardar((1, 100), vec![7; 4]);
        assert_eq!(c.leer((1, 100)), Some(vec![7; 4]));
        assert_eq!(c.leer((1, 200)), None);
    }

    #[test]
    fn saca_la_menos_usada_al_pasar_el_tope() {
        let mut c = Cache::nuevo(10);
        c.guardar((1, 100), vec![0; 4]);
        c.guardar((2, 100), vec![0; 4]);
        c.leer((1, 100));
        c.guardar((3, 100), vec![0; 4]);
        assert!(c.leer((2, 100)).is_none());
        assert!(c.leer((1, 100)).is_some());
        assert!(c.leer((3, 100)).is_some());
    }

    #[test]
    fn una_imagen_mayor_que_el_tope_no_se_queda() {
        let mut c = Cache::nuevo(10);
        c.guardar((1, 100), vec![0; 11]);
        assert!(c.leer((1, 100)).is_none());
        assert_eq!(c.total, 0);
    }

    #[test]
    fn vaciar_deja_todo_en_cero() {
        let mut c = Cache::nuevo(10);
        c.guardar((1, 100), vec![0; 4]);
        c.vaciar();
        assert!(c.leer((1, 100)).is_none());
        assert_eq!(c.total, 0);
    }
}

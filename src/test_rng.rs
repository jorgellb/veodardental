//! Generador pseudoaleatorio con semilla para las pruebas de entradas al
//! azar (fuzzing acotado y reproducible).

/// Generador pseudoaleatorio con semilla fija: las pruebas de entradas
/// al azar se repiten igual en cada ejecución (xorshift64).
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
    /// Una entrada válida con cambios al azar: bytes cambiados, cortada,
    /// repetida o con trozos de otra.
    pub fn mutate(&mut self, base: &[u8]) -> Vec<u8> {
        let mut v = base.to_vec();
        for _ in 0..1 + self.below(4) {
            match self.below(5) {
                0 if !v.is_empty() => {
                    let i = self.below(v.len());
                    v[i] = self.next() as u8;
                }
                1 => v.truncate(self.below(v.len() + 1)),
                2 if !v.is_empty() => {
                    let i = self.below(v.len());
                    let n = self.below(40);
                    let extra = self.bytes(n);
                    v.splice(i..i, extra);
                }
                3 => v.extend_from_slice(&base[..self.below(base.len() + 1)]),
                _ => {
                    let i = self.below(v.len() + 1);
                    let k = self.below(5);
                    v.insert(i, b"{[\"\\9"[k]);
                }
            }
        }
        v
    }
}

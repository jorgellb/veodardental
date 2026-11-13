//! Importes en céntimos enteros (D-06). El código nuevo suma y compara
//! dinero así, sin arrastrar errores de coma flotante. Los datos guardados
//! siguen en euros (`f64` redondeado al céntimo) y se convierten al entrar y
//! al salir.

use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cents(pub i64);

impl Cents {
    /// De un importe en euros, redondeando al céntimo más cercano (las
    /// mitades, lejos del cero).
    pub fn from_eur(eur: f64) -> Cents {
        Cents((eur * 100.0).round() as i64)
    }

    pub fn eur(self) -> f64 {
        self.0 as f64 / 100.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Add for Cents {
    type Output = Cents;
    fn add(self, o: Cents) -> Cents {
        Cents(self.0 + o.0)
    }
}

impl Sub for Cents {
    type Output = Cents;
    fn sub(self, o: Cents) -> Cents {
        Cents(self.0 - o.0)
    }
}

impl Neg for Cents {
    type Output = Cents;
    fn neg(self) -> Cents {
        Cents(-self.0)
    }
}

impl Mul<i64> for Cents {
    type Output = Cents;
    fn mul(self, n: i64) -> Cents {
        Cents(self.0 * n)
    }
}

impl AddAssign for Cents {
    fn add_assign(&mut self, o: Cents) {
        self.0 += o.0;
    }
}

impl SubAssign for Cents {
    fn sub_assign(&mut self, o: Cents) {
        self.0 -= o.0;
    }
}

impl Sum for Cents {
    fn sum<I: Iterator<Item = Cents>>(it: I) -> Cents {
        Cents(it.map(|c| c.0).sum())
    }
}

impl std::fmt::Display for Cents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::model::money(self.eur()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sumar diez mil veces 0,10 € en coma flotante se desvía; en céntimos, no.
    #[test]
    fn sums_are_exact_to_the_cent() {
        let float: f64 = (0..10_000).map(|_| 0.10).sum();
        assert_ne!(float, 1000.0, "en coma flotante se desvía");
        let cents: Cents = (0..10_000).map(|_| Cents::from_eur(0.10)).sum();
        assert_eq!(cents, Cents(100_000));
        assert_eq!(cents.eur(), 1000.0);
        assert_eq!(Cents::from_eur(94.25) - Cents::from_eur(100.0), Cents(-575));
        assert_eq!(Cents::from_eur(-0.005), Cents(-1), "las mitades, lejos del cero");
        assert_eq!(-Cents(5), Cents(-5));
    }
}

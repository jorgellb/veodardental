//! Facturación en los territorios forales (T-F4-08; ver `docs/normativa.md`).
//! Arquitectura y, de TicketBAI, lo que la especificación oficial define con
//! exactitud («Especificaciones funcionales y técnicas del sistema TicketBAI
//! 1.0», 19/12/2019: apartados 4.3.2 y 4.3.3 y anexo 3):
//! - el identificativo TBAI;
//! - la URL del código QR de cada hacienda;
//! - el código de control CRC-8.
//!
//! Tres pasos separados:
//! 1. **Implementación:** este módulo. Falta el fichero TBAI (XML firmado con
//!    XAdES) y su envío o registro, que se hacen con el XSD y la política de
//!    firma de cada hacienda (B-12).
//! 2. **Homologación:** registrar el programa en el registro de software de la
//!    hacienda foral (B-12). Hasta entonces, [`HOMOLOGATED`] es falso.
//! 3. **Activación:** solo si la clínica tributa por la normativa foral, cosa
//!    que confirma su gestoría, y nunca a partir de una suposición. Con el
//!    territorio confirmado, VERI*FACTU deja de enviar (no aplica allí).
//!
//! Navarra tiene su propio sistema, que no se ha consultado: queda pendiente.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// El programa no está homologado en ninguna hacienda foral (B-12).
pub const HOMOLOGATED: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Territory {
    Araba,
    Bizkaia,
    Gipuzkoa,
    Navarra,
}

impl Territory {
    pub const ALL: [Territory; 4] = [Territory::Araba, Territory::Bizkaia, Territory::Gipuzkoa, Territory::Navarra];

    pub fn label(self) -> &'static str {
        match self {
            Territory::Araba => "Araba/Álava (TicketBAI)",
            Territory::Bizkaia => "Bizkaia (TicketBAI / Batuz)",
            Territory::Gipuzkoa => "Gipuzkoa (TicketBAI)",
            Territory::Navarra => "Navarra (sistema propio, sin consultar)",
        }
    }
}

/// Lo que se configura: el territorio y quién confirmó que la clínica tributa allí.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ForalConfig {
    #[serde(default)]
    pub territory: Option<Territory>,
    /// Confirmación de la gestoría (fecha y quién): sin ella no se activa nada.
    #[serde(default)]
    pub confirmed: Option<(NaiveDate, String)>,
}

/// Qué sistema de facturación aplica.
#[derive(Clone, Debug, PartialEq)]
pub enum Regime {
    /// Territorio común: VERI*FACTU (con su propia configuración).
    Verifactu,
    /// Territorio foral confirmado, con el programa aún sin homologar allí:
    /// VERI*FACTU no envía, y hace falta otro programa homologado hasta la
    /// homologación.
    ForalPending(Territory),
    /// Territorio foral confirmado y programa homologado.
    Foral(Territory),
}

pub fn regime(c: &ForalConfig) -> Regime {
    match (c.territory, &c.confirmed) {
        (Some(t), Some(_)) if HOMOLOGATED && t != Territory::Navarra => Regime::Foral(t),
        (Some(t), Some(_)) => Regime::ForalPending(t),
        _ => Regime::Verifactu,
    }
}

/// ¿Se envía a la AEAT por VERI*FACTU? No en un territorio foral confirmado.
pub fn verifactu_applies(c: &ForalConfig) -> bool {
    regime(c) == Regime::Verifactu
}

/// El territorio foral, si está confirmado.
pub fn foral_territory(c: &ForalConfig) -> Option<Territory> {
    match regime(c) {
        Regime::Verifactu => None,
        Regime::ForalPending(t) | Regime::Foral(t) => Some(t),
    }
}

/// Lo que falta en un territorio foral confirmado, para la puesta en marcha.
pub fn pending_text(t: Territory) -> &'static str {
    match t {
        Territory::Navarra => "Navarra tiene su propio sistema de facturación, que este programa aún no tiene: hasta entonces, facture con un programa que cumpla con la Hacienda Foral de Navarra.",
        _ if HOMOLOGATED => "TicketBAI preparado.",
        _ => "VERI*FACTU no aplica aquí y este programa aún no está homologado en TicketBAI (faltan el fichero firmado, el envío y el alta en el registro de software de la hacienda foral): hasta entonces, facture con un programa homologado.",
    }
}

/// TicketBAI según la especificación: identificativo, QR y CRC-8. Se usa
/// al generar el fichero TBAI firmado, que falta (B-12); hasta entonces solo
/// lo comprueban las pruebas, con los ejemplos de la especificación.
#[cfg_attr(not(test), allow(dead_code))]
pub mod tbai {
    use super::Territory;
    use chrono::NaiveDate;

    /// URL de la aplicación de lectura del QR (especificación, apartado 4.3.3).
    pub fn qr_base(t: Territory) -> Option<&'static str> {
        match t {
            Territory::Araba => Some("https://ticketbai.araba.eus/TBAI/QRTBAI"),
            Territory::Bizkaia => Some("https://batuz.eus/QRTBAI/"),
            Territory::Gipuzkoa => Some("https://tbai.egoitza.gipuzkoa.eus/qr/"),
            Territory::Navarra => None,
        }
    }

    /// Tabla del CRC-8 (polinomio x⁸+x²+x+1, inicio 0, sin reflejar, anexo 3).
    const fn table() -> [u8; 256] {
        let mut t = [0u8; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u8;
            let mut b = 0;
            while b < 8 {
                c = if c & 0x80 != 0 { (c << 1) ^ 0x07 } else { c << 1 };
                b += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    }

    const CRC8: [u8; 256] = table();

    /// CRC-8 de la especificación, en tres cifras decimales.
    pub fn crc8(s: &str) -> String {
        let c = s.bytes().fold(0u8, |c, b| CRC8[(c ^ b) as usize]);
        format!("{c:03}")
    }

    /// Identificativo TBAI (39 caracteres): «TBAI-NIF-DDMMAA-13 primeros
    /// caracteres de la firma-CRC».
    pub fn tbai_id(nif: &str, issued: NaiveDate, signature_value: &str) -> Result<String, String> {
        let nif = nif.trim().to_uppercase();
        if nif.chars().count() != 9 || !nif.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("El NIF tiene que tener 9 caracteres, sin separadores.".into());
        }
        let sig: String = signature_value.chars().take(13).collect();
        if sig.chars().count() < 13 {
            return Err("Falta la firma del fichero TBAI.".into());
        }
        let head = format!("TBAI-{nif}-{}-{sig}-", issued.format("%d%m%y"));
        Ok(format!("{head}{}", crc8(&head)))
    }

    fn url_encode(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    /// Contenido del QR TBAI: URL de la hacienda con id, serie, número, importe
    /// (con IVA, como en el fichero TBAI) y, al final, el CRC-8 de todo lo anterior.
    pub fn qr_url(t: Territory, id: &str, series: &str, number: &str, total: &str) -> Result<String, String> {
        let base = qr_base(t).ok_or("Navarra no usa TicketBAI: su sistema está pendiente.")?;
        let body = format!("{base}?id={}&s={}&nf={}&i={}", url_encode(id), url_encode(series), url_encode(number), url_encode(total));
        Ok(format!("{body}&cr={}", crc8(&body)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Los ejemplos de la propia especificación (apartados 4.3.2 y 4.3.3).
        #[test]
        fn the_examples_of_the_specification_come_out_exactly() {
            assert_eq!(crc8("TBAI-00000006Y-251019-btFpwP8dcLGAF-"), "237");
            let id = tbai_id("00000006Y", NaiveDate::from_ymd_opt(2019, 10, 25).unwrap(), "btFpwP8dcLGAFsomethingelse").unwrap();
            assert_eq!(id, "TBAI-00000006Y-251019-btFpwP8dcLGAF-237");
            assert_eq!(id.len(), 39);
            assert_eq!(qr_url(Territory::Bizkaia, &id, "T", "27174", "4.70").unwrap(), "https://batuz.eus/QRTBAI/?id=TBAI-00000006Y-251019-btFpwP8dcLGAF-237&s=T&nf=27174&i=4.70&cr=007");
            // la tabla es la del anexo 3
            assert_eq!((CRC8[1], CRC8[2], CRC8[255]), (0x07, 0x0E, 0xF3));
            assert!(tbai_id("123", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), "x".repeat(20).as_str()).is_err());
            assert!(tbai_id("00000006Y", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), "corta").is_err());
            assert!(qr_url(Territory::Navarra, &id, "T", "1", "1.00").is_err());
            assert!(qr_url(Territory::Gipuzkoa, &id, "A 1", "1", "1.00").unwrap().starts_with("https://tbai.egoitza.gipuzkoa.eus/qr/?id=") && qr_url(Territory::Gipuzkoa, &id, "A 1", "1", "1.00").unwrap().contains("s=A%201"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nada foral se activa sin confirmar; confirmado, VERI*FACTU deja de
    /// aplicar, y sin homologación TicketBAI tampoco se activa.
    #[test]
    fn nothing_is_activated_without_confirmation_and_homologation() {
        let mut c = ForalConfig::default();
        assert_eq!(regime(&c), Regime::Verifactu);
        c.territory = Some(Territory::Gipuzkoa);
        assert_eq!(regime(&c), Regime::Verifactu, "sin confirmar, nada cambia");
        assert!(verifactu_applies(&c));
        c.confirmed = Some((NaiveDate::from_ymd_opt(2026, 11, 14).unwrap(), "Gestoría".into()));
        assert_eq!(regime(&c), Regime::ForalPending(Territory::Gipuzkoa));
        assert!(!verifactu_applies(&c));
        const { assert!(!HOMOLOGATED) };
    }
}

//! Firma electrónica de las recetas con el certificado del prescriptor.
//!
//! El RD 1718/2010 (art. 3.2.c y 8.1) exige que el prescriptor firme
//! electrónicamente cada receta; el control FD02 de la homologación del
//! Sistema de Receta Electrónica Privada pide un certificado cualificado y un
//! formato de firma avanzada como XAdES, CAdES o PAdES.
//!
//! Aquí se firma en CAdES-BES separado (CMS `SignedData` sin contenido, con
//! los atributos firmados contentType, signingTime, messageDigest y
//! signingCertificateV2) con el certificado personal del dentista en un
//! fichero .p12/.pfx (FNMT, colegio…). La contraseña se pide al firmar y no
//! se guarda. Claves RSA (PKCS#1 v1.5 con SHA-256) y EC P-256 (ECDSA SHA-256).

use chrono::{DateTime, NaiveDateTime, Utc};
use ring::rand::SystemRandom;
use ring::signature::{self, EcdsaKeyPair, KeyPair, RsaKeyPair};
use sha2::{Digest, Sha256};

const OID_DATA: &str = "1.2.840.113549.1.7.1";
const OID_SIGNED_DATA: &str = "1.2.840.113549.1.7.2";
const OID_CONTENT_TYPE: &str = "1.2.840.113549.1.9.3";
const OID_MESSAGE_DIGEST: &str = "1.2.840.113549.1.9.4";
const OID_SIGNING_TIME: &str = "1.2.840.113549.1.9.5";
const OID_SIGNING_CERT_V2: &str = "1.2.840.113549.1.9.16.2.47";
const OID_SHA256: &str = "2.16.840.1.101.3.4.2.1";
const OID_RSA: &str = "1.2.840.113549.1.1.1";
const OID_EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";
const OID_ECDSA_SHA256: &str = "1.2.840.10045.4.3.2";
const OID_QC_STATEMENTS: &str = "1.3.6.1.5.5.7.1.3";
const OID_SERIAL_NUMBER: &str = "2.5.4.5";

// ───────────────────────────── DER mínimo ─────────────────────────────

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    let n = content.len();
    if n < 0x80 {
        v.push(n as u8);
    } else {
        let bytes: Vec<u8> = n.to_be_bytes().iter().copied().skip_while(|b| *b == 0).collect();
        v.push(0x80 | bytes.len() as u8);
        v.extend(bytes);
    }
    v.extend_from_slice(content);
    v
}

fn seq(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

/// SET OF en DER: los elementos ordenados por su codificación.
fn set_of(mut parts: Vec<Vec<u8>>) -> Vec<u8> {
    parts.sort();
    tlv(0x31, &parts.concat())
}

fn oid(dotted: &str) -> Vec<u8> {
    let n: Vec<u64> = dotted.split('.').filter_map(|x| x.parse().ok()).collect();
    let mut body = vec![(n[0] * 40 + n[1]) as u8];
    for &x in &n[2..] {
        let mut stack = vec![(x & 0x7f) as u8];
        let mut y = x >> 7;
        while y > 0 {
            stack.push(0x80 | (y & 0x7f) as u8);
            y >>= 7;
        }
        stack.reverse();
        body.extend(stack);
    }
    tlv(0x06, &body)
}

fn octets(b: &[u8]) -> Vec<u8> {
    tlv(0x04, b)
}

fn small_int(n: u8) -> Vec<u8> {
    tlv(0x02, &[n])
}

fn alg(o: &str, null_params: bool) -> Vec<u8> {
    if null_params { seq(&[&oid(o), &[0x05, 0x00]]) } else { seq(&[&oid(o)]) }
}

fn utc_time(t: DateTime<Utc>) -> Vec<u8> {
    tlv(0x17, t.format("%y%m%d%H%M%SZ").to_string().as_bytes())
}

/// Lector DER: (etiqueta, contenido, resto).
#[cfg(test)]
fn read(input: &[u8]) -> Result<(u8, &[u8], &[u8]), String> {
    let bad = || "firma con formato DER no válido".to_string();
    if input.len() < 2 {
        return Err(bad());
    }
    let tag = input[0];
    let (len, hdr) = if input[1] < 0x80 {
        (input[1] as usize, 2)
    } else {
        let k = (input[1] & 0x7f) as usize;
        if k == 0 || k > 4 || input.len() < 2 + k {
            return Err(bad());
        }
        (input[2..2 + k].iter().fold(0usize, |a, b| (a << 8) | *b as usize), 2 + k)
    };
    if input.len() < hdr + len {
        return Err(bad());
    }
    Ok((tag, &input[hdr..hdr + len], &input[hdr + len..]))
}

/// Un elemento DER: (etiqueta, contenido, codificación completa).
#[cfg(test)]
type Element<'a> = (u8, &'a [u8], &'a [u8]);

#[cfg(test)]
fn children(mut content: &[u8]) -> Result<Vec<Element<'_>>, String> {
    let mut out = vec![];
    while !content.is_empty() {
        let (t, c, rest) = read(content)?;
        let whole = &content[..content.len() - rest.len()];
        out.push((t, c, whole));
        content = rest;
    }
    Ok(out)
}

// ───────────────────────────── certificado ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CertInfo {
    pub subject: String,
    pub cn: String,
    /// NIF del titular (atributo serialNumber del sujeto, p. ej. «IDCES-12345678Z»).
    pub nif: String,
    pub issuer: String,
    pub not_before: Option<NaiveDateTime>,
    pub not_after: Option<NaiveDateTime>,
    /// Lleva la declaración de certificado cualificado (qcStatements).
    pub qualified: bool,
    pub key: String,
    pub sha256: String,
}

impl CertInfo {
    pub fn valid_at(&self, t: NaiveDateTime) -> bool {
        self.not_before.is_none_or(|a| t >= a) && self.not_after.is_none_or(|b| t <= b)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn cert_info(der: &[u8]) -> Result<CertInfo, String> {
    let (_, c) = x509_parser::parse_x509_certificate(der).map_err(|e| format!("El certificado no se puede leer: {e}"))?;
    let mut cn = String::new();
    let mut nif = String::new();
    for rdn in c.subject().iter() {
        for atv in rdn.iter() {
            let id = atv.attr_type().to_id_string();
            let val = atv.as_str().unwrap_or_default().to_string();
            if id == "2.5.4.3" && cn.is_empty() {
                cn = val;
            } else if id == OID_SERIAL_NUMBER {
                nif = val;
            }
        }
    }
    let ts = |t: i64| DateTime::<Utc>::from_timestamp(t, 0).map(|d| d.naive_utc());
    let key = match c.public_key().algorithm.algorithm.to_id_string().as_str() {
        OID_RSA => format!("RSA {} bits", c.public_key().parsed().map(|k| k.key_size()).unwrap_or(0)),
        OID_EC_PUBLIC_KEY => "EC (curva elíptica)".to_string(),
        o => o.to_string(),
    };
    Ok(CertInfo {
        subject: c.subject().to_string(),
        cn,
        nif,
        issuer: c.issuer().to_string(),
        not_before: ts(c.validity().not_before.timestamp()),
        not_after: ts(c.validity().not_after.timestamp()),
        qualified: c.extensions().iter().any(|e| e.oid.to_id_string() == OID_QC_STATEMENTS),
        key,
        sha256: hex(&Sha256::digest(der)),
    })
}

// ───────────────────────────── firmante ─────────────────────────────

enum Key {
    Rsa(RsaKeyPair),
    Ec(EcdsaKeyPair),
}

pub struct Signer {
    key: Key,
    pub cert: Vec<u8>,
    pub chain: Vec<Vec<u8>>,
    pub info: CertInfo,
}

/// Clave privada (PKCS#8), cadena de certificados (el del titular primero) y
/// nombre del titular de un certificado .p12/.pfx.
pub type P12Der = (Vec<u8>, Vec<Vec<u8>>, String);

/// Lo que hace falta de un .p12/.pfx para identificarse en conexiones seguras.
pub fn load_p12_der(bytes: &[u8], password: &str) -> Result<P12Der, String> {
    let s = load_p12(bytes, password)?;
    use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
    let ks = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Relaxed).map_err(|e| e.to_string())?;
    let (_, chain) = ks.private_key_chain().ok_or("El fichero no contiene la clave privada.")?;
    let key = chain.key().as_der().to_vec();
    let mut certs = vec![s.cert.clone()];
    certs.extend(s.chain.iter().cloned());
    let who = if s.info.cn.is_empty() { s.info.subject.clone() } else { s.info.cn.clone() };
    Ok((key, certs, who))
}

/// Abre un certificado personal .p12/.pfx con su contraseña.
pub fn load_p12(bytes: &[u8], password: &str) -> Result<Signer, String> {
    use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
    let ks = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Relaxed).map_err(|e| {
        let t = e.to_string().to_lowercase();
        if t.contains("mac") || t.contains("password") || t.contains("decrypt") || t.contains("unpad") { "La contraseña del certificado no es correcta.".to_string() } else { format!("No se puede abrir el certificado: {e}") }
    })?;
    let (_, chain) = ks.private_key_chain().ok_or("El fichero no contiene un certificado con su clave privada (exporte el certificado «con clave privada»).")?;
    let pk = chain.key().as_der().to_vec();
    let cert = chain.certs().first().map(|c| c.as_der().to_vec()).ok_or("El fichero no trae el certificado del titular.")?;
    let rest: Vec<Vec<u8>> = chain.certs().iter().skip(1).map(|c| c.as_der().to_vec()).collect();
    let rng = SystemRandom::new();
    let key = match chain.key().oid().to_string().as_str() {
        OID_RSA => Key::Rsa(RsaKeyPair::from_pkcs8(&pk).map_err(|e| format!("Clave RSA no válida: {e}"))?),
        OID_EC_PUBLIC_KEY => Key::Ec(EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_ASN1_SIGNING, &pk, &rng).map_err(|_| "Solo se admiten claves EC de la curva P-256.".to_string())?),
        o => return Err(format!("Tipo de clave no admitido ({o}).")),
    };
    let info = cert_info(&cert)?;
    // la clave privada debe corresponder al certificado
    let public = match &key {
        Key::Rsa(k) => k.public().as_ref().to_vec(),
        Key::Ec(k) => k.public_key().as_ref().to_vec(),
    };
    let (_, c) = x509_parser::parse_x509_certificate(&cert).map_err(|e| e.to_string())?;
    if c.public_key().subject_public_key.data.as_ref() != public.as_slice() {
        return Err("La clave privada no corresponde al certificado.".into());
    }
    Ok(Signer { key, cert, chain: rest, info })
}

impl Signer {
    fn raw_sign(&self, msg: &[u8]) -> Result<Vec<u8>, String> {
        let rng = SystemRandom::new();
        match &self.key {
            Key::Rsa(k) => {
                let mut sig = vec![0u8; k.public().modulus_len()];
                k.sign(&signature::RSA_PKCS1_SHA256, &rng, msg, &mut sig).map_err(|_| "No se pudo firmar con la clave RSA.".to_string())?;
                Ok(sig)
            }
            Key::Ec(k) => k.sign(&rng, msg).map(|s| s.as_ref().to_vec()).map_err(|_| "No se pudo firmar con la clave EC.".to_string()),
        }
    }

    /// Firma CAdES-BES separada (DER) del contenido.
    pub fn sign_cades(&self, content: &[u8], when: DateTime<Utc>) -> Result<Vec<u8>, String> {
        let (_, c) = x509_parser::parse_x509_certificate(&self.cert).map_err(|e| e.to_string())?;
        let issuer = c.tbs_certificate.issuer.as_raw().to_vec();
        let serial = tlv(0x02, c.tbs_certificate.raw_serial());
        let digest = Sha256::digest(content);
        let cert_hash = Sha256::digest(&self.cert);
        let ess = seq(&[&octets(&cert_hash), &seq(&[&seq(&[&tlv(0xA4, &issuer)]), &serial])]);
        let attrs = vec![
            seq(&[&oid(OID_CONTENT_TYPE), &set_of(vec![oid(OID_DATA)])]),
            seq(&[&oid(OID_SIGNING_TIME), &set_of(vec![utc_time(when)])]),
            seq(&[&oid(OID_MESSAGE_DIGEST), &set_of(vec![octets(&digest)])]),
            seq(&[&oid(OID_SIGNING_CERT_V2), &set_of(vec![seq(&[&seq(&[&ess])])])]),
        ];
        let signed_attrs = set_of(attrs); // 0x31 …: lo que se firma
        let sig = self.raw_sign(&signed_attrs)?;
        let sig_alg = match self.key {
            Key::Rsa(_) => alg(OID_RSA, true),
            Key::Ec(_) => alg(OID_ECDSA_SHA256, false),
        };
        let mut implicit_attrs = signed_attrs.clone();
        implicit_attrs[0] = 0xA0; // [0] IMPLICIT
        let signer_info = seq(&[&small_int(1), &seq(&[&issuer, &serial]), &alg(OID_SHA256, false), &implicit_attrs, &sig_alg, &octets(&sig)]);
        let mut certs = self.cert.clone();
        for x in &self.chain {
            certs.extend_from_slice(x);
        }
        let signed_data = seq(&[&small_int(1), &set_of(vec![alg(OID_SHA256, false)]), &seq(&[&oid(OID_DATA)]), &tlv(0xA0, &certs), &set_of(vec![signer_info])]);
        Ok(seq(&[&oid(OID_SIGNED_DATA), &tlv(0xA0, &signed_data)]))
    }
}

/// Comprueba una firma CAdES separada sobre un contenido: la huella del
/// contenido y la firma con la clave del certificado que lleva dentro.
/// Devuelve los datos del firmante.
#[cfg(test)]
pub fn verify_cades(cms: &[u8], content: &[u8]) -> Result<CertInfo, String> {
    let (t, ci, _) = read(cms)?;
    if t != 0x30 {
        return Err("No es una firma CMS.".into());
    }
    let ci = children(ci)?;
    if ci.len() < 2 || ci[0].2 != oid(OID_SIGNED_DATA).as_slice() {
        return Err("No es una firma CMS SignedData.".into());
    }
    let (_, sd, _) = read(ci[1].1)?;
    let sd = children(sd)?;
    let cert = sd.iter().find(|x| x.0 == 0xA0).ok_or("La firma no incluye el certificado.")?;
    let (_, _, first_cert_rest) = read(cert.1)?;
    let cert_der = &cert.1[..cert.1.len() - first_cert_rest.len()];
    let infos = sd.last().ok_or("Firma incompleta.")?;
    let (_, si, _) = read(infos.1)?;
    let si = children(si)?;
    let attrs = si.iter().find(|x| x.0 == 0xA0).ok_or("La firma no tiene atributos firmados.")?;
    let sig = si.iter().rev().find(|x| x.0 == 0x04).ok_or("La firma no tiene valor.")?;
    // messageDigest
    let mut digest_ok = false;
    for (_, a, _) in children(attrs.1)? {
        let parts = children(a)?;
        if parts.len() == 2 && parts[0].2 == oid(OID_MESSAGE_DIGEST).as_slice() {
            let (_, v, _) = read(parts[1].1)?;
            digest_ok = v == Sha256::digest(content).as_slice();
        }
    }
    if !digest_ok {
        return Err("El documento no coincide con el firmado (huella distinta).".into());
    }
    let mut signed = attrs.2.to_vec();
    signed[0] = 0x31;
    let (_, c) = x509_parser::parse_x509_certificate(cert_der).map_err(|e| e.to_string())?;
    let pk = c.public_key().subject_public_key.data.as_ref();
    let alg_ok = match c.public_key().algorithm.algorithm.to_id_string().as_str() {
        OID_RSA => signature::UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, pk).verify(&signed, sig.1).is_ok(),
        OID_EC_PUBLIC_KEY => signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, pk).verify(&signed, sig.1).is_ok(),
        _ => false,
    };
    if !alg_ok {
        return Err("La firma no es válida.".into());
    }
    cert_info(cert_der)
}

pub fn b64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
pub fn unb64(s: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = vec![];
    for ch in clean.chunks(4) {
        let pad = ch.iter().filter(|c| **c == b'=').count();
        let mut n = 0u32;
        for (i, c) in ch.iter().enumerate() {
            let v = if *c == b'=' { 0 } else { val(*c).ok_or("base64 no válido")? };
            n |= v << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad.min(2)]);
    }
    Ok(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Certificado de prueba EC P-256 empaquetado en .p12 con contraseña.
    pub fn test_p12(password: &str) -> Vec<u8> {
        use p12_keystore::{Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain};
        let mut params = rcgen::CertificateParams::new(vec!["prueba.invalid".to_string()]).unwrap();
        params.distinguished_name.push(rcgen::DnType::CommonName, "DRA. PRUEBA EJEMPLO - 00000000T");
        params.distinguished_name.push(rcgen::DnType::CustomDnType(vec![2, 5, 4, 5]), "IDCES-00000000T");
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        let chain = PrivateKeyChain::new(vec![1u8, 2, 3], PrivateKey::from_der(&key.serialize_der()).unwrap(), [Certificate::from_der(cert.der()).unwrap()]);
        let mut ks = KeyStore::new();
        ks.add_entry("firma", KeyStoreEntry::PrivateKeyChain(chain));
        ks.writer(password).write().unwrap()
    }

    #[test]
    fn der_basics() {
        assert_eq!(oid("1.2.840.113549.1.7.2"), vec![0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02]);
        assert_eq!(tlv(0x04, &[0u8; 200])[..3], [0x04, 0x81, 200]);
        assert_eq!(b64(b"Receta"), "UmVjZXRh");
        assert_eq!(unb64(&b64(b"firma digital!")).unwrap(), b"firma digital!");
        assert_eq!(unb64("YQ==").unwrap(), b"a");
    }

    #[test]
    fn signs_and_verifies_cades_with_p12() {
        let p12 = test_p12("clave-segura");
        assert!(load_p12(&p12, "otra").err().unwrap().contains("contraseña"));
        let s = load_p12(&p12, "clave-segura").unwrap();
        assert_eq!(s.info.nif, "IDCES-00000000T");
        assert!(s.info.cn.starts_with("DRA. PRUEBA"));
        assert!(!s.info.qualified, "un certificado de prueba no es cualificado");
        let doc = br#"{"receta":"ibuprofeno"}"#;
        let cms = s.sign_cades(doc, Utc::now()).unwrap();
        let who = verify_cades(&cms, doc).unwrap();
        assert_eq!(who.nif, "IDCES-00000000T");
        assert!(verify_cades(&cms, b"otro documento").unwrap_err().contains("no coincide"));
        // firma alterada
        let mut bad = cms.clone();
        let n = bad.len();
        bad[n - 5] ^= 0x55;
        assert!(verify_cades(&bad, doc).is_err());
        // OpenSSL, si está instalado, la reconoce como CMS válida
        if std::process::Command::new("openssl").arg("version").output().is_ok() {
            let dir = std::env::temp_dir().join(format!("ds_cades_{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("f.der"), &cms).unwrap();
            std::fs::write(dir.join("doc.json"), doc).unwrap();
            let out =
                std::process::Command::new("openssl").args(["cms", "-verify", "-binary", "-inform", "DER", "-noverify", "-in"]).arg(dir.join("f.der")).arg("-content").arg(dir.join("doc.json")).arg("-out").arg(dir.join("salida")).output().unwrap();
            let _ = std::fs::remove_dir_all(&dir);
            assert!(out.status.success(), "openssl: {}", String::from_utf8_lossy(&out.stderr));
        }
    }

    #[test]
    fn rsa_certificate_from_openssl() {
        // Comprueba la vía RSA (la habitual en los certificados de la FNMT) con un .p12 de OpenSSL.
        let dir = std::env::temp_dir().join(format!("ds_rsa_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| std::process::Command::new("openssl").args(args).current_dir(&dir).output();
        let Ok(o) = run(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "k.pem", "-out", "c.pem", "-days", "30", "-subj", "/CN=DR PRUEBA RSA/serialNumber=IDCES-11111111H"]) else {
            return; // sin OpenSSL no se puede crear el certificado de prueba
        };
        if !o.status.success() {
            return;
        }
        let o = run(&["pkcs12", "-export", "-inkey", "k.pem", "-in", "c.pem", "-out", "c.p12", "-passout", "pass:1234"]).unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let p12 = std::fs::read(dir.join("c.p12")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let s = load_p12(&p12, "1234").unwrap();
        assert!(s.info.key.starts_with("RSA 2048"), "{}", s.info.key);
        let cms = s.sign_cades(b"doc", Utc::now()).unwrap();
        assert_eq!(verify_cades(&cms, b"doc").unwrap().nif, "IDCES-11111111H");
    }
}

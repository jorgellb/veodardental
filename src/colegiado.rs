//! Comprobación de la habilitación del colegiado antes de recetar
//! («Diseño funcional e-receta privada v2.03», 2.3 y 2.12–2.14; control FT06).
//!
//! Cada Consejo General publica un servicio SOAP que dice si un número de
//! colegiado puede prescribir. Se comprueba al entrar a recetar y como mucho
//! cada 24 horas. Estos servicios solo atienden a los sistemas ya
//! certificados (desde aquí responden «403 Forbidden»): hasta entonces se
//! prueban contra un servicio simulado.

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Consejo {
    #[default]
    Dentistas,
    Medicos,
    Podologos,
}

impl Consejo {
    /// Dirección publicada en el diseño funcional v2.03.
    pub fn default_url(self) -> &'static str {
        match self {
            Consejo::Dentistas => "http://ws.guiadentistas.es/WScdentistas",
            Consejo::Medicos => "http://registrocgcom.vuds-omc.es:8001/WSRegistroCGCOM",
            Consejo::Podologos => "https://cgcop.es:9443/ColegiadoCGCOP",
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Petición SOAP de cada Consejo con el número de colegiado.
pub fn soap_request(c: Consejo, numero: &str) -> String {
    let n = xml_escape(numero.trim());
    match c {
        Consejo::Dentistas => format!(
            "<soapenv:Envelope xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\"><soapenv:Header/><soapenv:Body><Consulta_Habilitacion soapenv:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><numero_colegiado xsi:type=\"xsd:string\">{n}</numero_colegiado></Consulta_Habilitacion></soapenv:Body></soapenv:Envelope>"
        ),
        Consejo::Medicos => format!(
            "<soapenv:Envelope xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\" xmlns:urn=\"urn:cgcom.es:vuds.1.0\"><soapenv:Header/><soapenv:Body><urn:PeticionConsultaRegistroCGCOM><urn:NumeroColegiado>{n}</urn:NumeroColegiado></urn:PeticionConsultaRegistroCGCOM></soapenv:Body></soapenv:Envelope>"
        ),
        Consejo::Podologos => format!(
            "<soapenv:Envelope xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\" xmlns:urn=\"urn:cgcop.es:vuds.1.0\"><soapenv:Header/><soapenv:Body><urn:PeticionConsultaRegistroCGCOP><urn:NumeroColegiadoEstatal>{n}</urn:NumeroColegiadoEstatal></urn:PeticionConsultaRegistroCGCOP></soapenv:Body></soapenv:Envelope>"
        ),
    }
}

/// Texto de la primera etiqueta con ese nombre local (con o sin prefijo).
fn tag_text(xml: &str, local: &str) -> Option<String> {
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>')?;
        let tag = &rest[..end];
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let lname = name.rsplit(':').next().unwrap_or(name);
        if lname.eq_ignore_ascii_case(local) {
            if tag.ends_with('/') {
                return Some(String::new());
            }
            let body = &rest[end + 1..];
            let close = body.find("</")?;
            return Some(body[..close].trim().to_string());
        }
    }
    None
}

/// (habilitado, mensaje del Consejo).
pub fn parse_response(c: Consejo, xml: &str) -> Result<(bool, String), String> {
    let (flag, msg) = match c {
        Consejo::Dentistas => (tag_text(xml, "habilitacion"), tag_text(xml, "mensaje")),
        _ => (tag_text(xml, "Resultado"), tag_text(xml, "DescripcionError")),
    };
    let flag = flag.ok_or_else(|| {
        if let Some(f) = tag_text(xml, "faultstring") {
            format!("El servicio del Consejo devolvió un error: {f}")
        } else {
            "Respuesta del servicio del Consejo no reconocida.".to_string()
        }
    })?;
    Ok((flag.eq_ignore_ascii_case("true"), msg.unwrap_or_default()))
}

/// Pregunta al Consejo si el colegiado puede recetar.
pub fn consultar(url: &str, c: Consejo, numero: &str) -> Result<(bool, String), String> {
    if numero.trim().is_empty() || numero.trim() == "-" {
        return Err("Falta el número de colegiado.".into());
    }
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout(Duration::from_secs(20)).build();
    let resp = agent.post(url.trim()).set("Content-Type", "text/xml; charset=utf-8").set("SOAPAction", "\"\"").send_string(&soap_request(c, numero));
    let text = match resp {
        Ok(r) => r.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(403, _)) => return Err("El servicio del Consejo no admite consultas de este ordenador (403): solo atiende a sistemas de prescripción certificados.".into()),
        Err(ureq::Error::Status(500, r)) => r.into_string().unwrap_or_default(),
        Err(ureq::Error::Status(code, _)) => return Err(format!("El servicio del Consejo respondió con el error {code}.")),
        Err(ureq::Error::Transport(t)) => return Err(format!("No se pudo conectar con el servicio del Consejo: {t}")),
    };
    parse_response(c, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_follow_the_published_format() {
        let d = soap_request(Consejo::Dentistas, "28<001>");
        assert!(d.contains("<Consulta_Habilitacion") && d.contains("<numero_colegiado xsi:type=\"xsd:string\">28&lt;001&gt;</numero_colegiado>"));
        assert!(soap_request(Consejo::Medicos, "280123").contains("<urn:NumeroColegiado>280123</urn:NumeroColegiado>"));
        assert!(soap_request(Consejo::Podologos, "123").contains("NumeroColegiadoEstatal"));
    }

    #[test]
    fn parses_council_answers() {
        let ok = r#"<SOAP-ENV:Envelope><SOAP-ENV:Body><cdentistas:Consulta_HabilitacionRespuesta xmlns:cdentistas="x"><habilitacion xsi:type="xsd:string">true</habilitacion></cdentistas:Consulta_HabilitacionRespuesta></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;
        assert_eq!(parse_response(Consejo::Dentistas, ok).unwrap(), (true, String::new()));
        let no = r#"<Body><x:Consulta_HabilitacionRespuesta><habilitacion xsi:type="xsd:string">false</habilitacion><mensaje xsi:type="xsd:string">Colegiado no habilitado</mensaje></x:Consulta_HabilitacionRespuesta></Body>"#;
        assert_eq!(parse_response(Consejo::Dentistas, no).unwrap(), (false, "Colegiado no habilitado".into()));
        let med = r#"<soap:Envelope><soap:Body><cgcom:RespuestaConsultaRegistroCGCOM><cgcom:Resultado>false</cgcom:Resultado><cgcom:DescripcionError>Baja colegial</cgcom:DescripcionError></cgcom:RespuestaConsultaRegistroCGCOM></soap:Body></soap:Envelope>"#;
        assert_eq!(parse_response(Consejo::Medicos, med).unwrap(), (false, "Baja colegial".into()));
        assert!(parse_response(Consejo::Medicos, "<html>403</html>").is_err());
    }

    #[test]
    fn asks_a_stand_in_council_service() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let h = std::thread::spawn(move || {
            for _ in 0..2 {
                let mut req = server.recv().unwrap();
                let mut body = String::new();
                std::io::Read::read_to_string(req.as_reader(), &mut body).unwrap();
                let enabled = body.contains(">280001<");
                let xml = format!("<SOAP-ENV:Envelope><SOAP-ENV:Body><ns:Consulta_HabilitacionRespuesta><habilitacion>{enabled}</habilitacion>{}</ns:Consulta_HabilitacionRespuesta></SOAP-ENV:Body></SOAP-ENV:Envelope>", if enabled { "" } else { "<mensaje>No consta</mensaje>" });
                req.respond(tiny_http::Response::from_string(xml)).unwrap();
            }
        });
        let url = format!("http://127.0.0.1:{port}/WScdentistas");
        assert_eq!(consultar(&url, Consejo::Dentistas, "280001").unwrap(), (true, String::new()));
        assert_eq!(consultar(&url, Consejo::Dentistas, "999").unwrap(), (false, "No consta".into()));
        assert!(consultar(&url, Consejo::Dentistas, "-").is_err());
        h.join().unwrap();
    }
}

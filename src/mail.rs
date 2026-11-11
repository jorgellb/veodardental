//! Correo electrónico saliente (SMTP) con adjuntos: órdenes de trabajo al
//! laboratorio y, en general, cualquier envío del programa que necesite
//! adjuntar ficheros (un enlace «mailto:» no puede llevar adjuntos).

use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Seguridad de la conexión con el servidor de correo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Security {
    /// STARTTLS (normalmente el puerto 587).
    #[default]
    StartTls,
    /// TLS desde el principio (normalmente el puerto 465).
    Tls,
    /// Sin cifrar (sólo para un servidor dentro de la propia red).
    Ninguna,
}

impl Security {
    pub const ALL: [Security; 3] = [Security::StartTls, Security::Tls, Security::Ninguna];
    pub fn label(self) -> &'static str {
        match self {
            Security::StartTls => "STARTTLS (puerto 587)",
            Security::Tls => "SSL/TLS (puerto 465)",
            Security::Ninguna => "Sin cifrar (sólo red local)",
        }
    }
    pub fn default_port(self) -> u16 {
        match self {
            Security::StartTls => 587,
            Security::Tls => 465,
            Security::Ninguna => 25,
        }
    }
}

/// Proveedores habituales: servidor, puerto y seguridad.
pub const PRESETS: [(&str, &str, u16, Security); 5] = [
    ("Gmail", "smtp.gmail.com", 587, Security::StartTls),
    ("Outlook / 365", "smtp.office365.com", 587, Security::StartTls),
    ("IONOS", "smtp.ionos.es", 587, Security::StartTls),
    ("OVH", "ssl0.ovh.net", 465, Security::Tls),
    ("Yahoo", "smtp.mail.yahoo.com", 465, Security::Tls),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub user: String,
    /// Contraseña (o contraseña de aplicación). Se guarda con los datos de la clínica.
    #[serde(default)]
    pub password: String,
    pub from_name: String,
    pub from_email: String,
    /// Copia oculta a la propia clínica de todo lo que se envía.
    #[serde(default)]
    pub bcc_self: bool,
    /// Envío automático, cada día, de las comunicaciones por correo (recordatorios, revisiones…).
    #[serde(default)]
    pub auto_send: bool,
    /// Hora a partir de la que se hace el envío del día.
    #[serde(default)]
    pub auto_hour: u32,
    /// Ordenador que hace el envío automático (sólo uno, para no enviarlo dos veces).
    #[serde(default)]
    pub auto_station: String,
    /// Último día en que se hizo el envío automático.
    #[serde(default)]
    pub last_auto: Option<chrono::NaiveDate>,
}

impl Default for MailConfig {
    fn default() -> Self {
        MailConfig {
            enabled: false,
            host: String::new(),
            port: 587,
            security: Security::StartTls,
            user: String::new(),
            password: String::new(),
            from_name: String::new(),
            from_email: String::new(),
            bcc_self: false,
            auto_send: false,
            auto_hour: 10,
            auto_station: String::new(),
            last_auto: None,
        }
    }
}

impl MailConfig {
    pub fn ready(&self) -> bool {
        self.enabled && !self.host.trim().is_empty() && !self.from_email.trim().is_empty()
    }
}

/// Un fichero adjunto.
pub struct FileAttachment {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

fn mailbox(name: &str, email: &str) -> Result<Mailbox, String> {
    let email = email.trim();
    let addr = email.parse().map_err(|_| format!("La dirección «{email}» no es válida."))?;
    Ok(Mailbox::new(if name.trim().is_empty() { None } else { Some(name.trim().to_string()) }, addr))
}

/// Construye el mensaje (texto y adjuntos).
pub fn build(cfg: &MailConfig, to: &str, subject: &str, text: &str, files: &[FileAttachment]) -> Result<Message, String> {
    let mut b = Message::builder().from(mailbox(&cfg.from_name, &cfg.from_email)?).subject(subject);
    let mut any = false;
    for addr in to.split([',', ';']).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        b = b.to(mailbox("", addr)?);
        any = true;
    }
    if !any {
        return Err("Falta la dirección de correo del destinatario.".into());
    }
    if cfg.bcc_self {
        b = b.bcc(mailbox(&cfg.from_name, &cfg.from_email)?);
    }
    let mut mp = MultiPart::mixed().singlepart(SinglePart::plain(text.to_string()));
    for f in files {
        let ct = ContentType::parse(&f.mime).unwrap_or(ContentType::parse("application/octet-stream").map_err(|e| e.to_string())?);
        mp = mp.singlepart(Attachment::new(f.name.clone()).body(f.bytes.clone(), ct));
    }
    b.multipart(mp).map_err(|e| format!("No se pudo preparar el correo: {e}"))
}

fn transport(cfg: &MailConfig) -> Result<SmtpTransport, String> {
    let host = cfg.host.trim();
    if host.is_empty() {
        return Err("Indique el servidor de correo saliente (SMTP) en Configuración › Correo electrónico.".into());
    }
    let builder = match cfg.security {
        Security::StartTls => SmtpTransport::starttls_relay(host).map_err(|e| e.to_string())?,
        Security::Tls => SmtpTransport::relay(host).map_err(|e| e.to_string())?,
        Security::Ninguna => SmtpTransport::builder_dangerous(host),
    };
    let mut builder = builder.port(if cfg.port == 0 { cfg.security.default_port() } else { cfg.port }).timeout(Some(Duration::from_secs(40)));
    if !cfg.user.trim().is_empty() {
        builder = builder.credentials(Credentials::new(cfg.user.trim().to_string(), cfg.password.clone()));
    }
    Ok(builder.build())
}

/// Envía un correo con adjuntos.
pub fn send(cfg: &MailConfig, to: &str, subject: &str, text: &str, files: &[FileAttachment]) -> Result<(), String> {
    let msg = build(cfg, to, subject, text, files)?;
    let t = transport(cfg)?;
    t.send(&msg).map(|_| ()).map_err(|e| {
        let s = e.to_string();
        let l = s.to_lowercase();
        if l.contains("authentication") || l.contains("535") || l.contains("credentials") {
            "El servidor rechazó el usuario o la contraseña (en Gmail u Outlook hace falta una «contraseña de aplicación»).".into()
        } else if l.contains("connection") || l.contains("timed out") || l.contains("dns") || l.contains("resolve") {
            format!("No se puede conectar con el servidor de correo: {s}")
        } else {
            format!("El servidor de correo no aceptó el envío: {s}")
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    /// Atiende una conexión SMTP y devuelve las órdenes y el mensaje recibidos.
    fn serve_one(s: std::net::TcpStream) -> (Vec<String>, String) {
        let mut w = s.try_clone().unwrap();
        let mut r = BufReader::new(s);
        let mut cmds = vec![];
        let mut data = String::new();
        writeln!(w, "220 prueba ESMTP\r").unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let cmd = line.trim_end().to_string();
            let up = cmd.to_uppercase();
            cmds.push(cmd.clone());
            if up.starts_with("EHLO") {
                write!(w, "250-prueba\r\n250 8BITMIME\r\n").unwrap();
            } else if up.starts_with("DATA") {
                write!(w, "354 adelante\r\n").unwrap();
                loop {
                    line.clear();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == ".\r\n" {
                        break;
                    }
                    data.push_str(&line);
                }
                write!(w, "250 aceptado\r\n").unwrap();
            } else if up.starts_with("QUIT") {
                write!(w, "221 adios\r\n").unwrap();
                break;
            } else {
                write!(w, "250 ok\r\n").unwrap();
            }
        }
        (cmds, data)
    }

    /// Servidor SMTP mínimo de prueba: acepta un mensaje y devuelve lo recibido.
    pub(crate) fn fake_smtp() -> (u16, std::thread::JoinHandle<(Vec<String>, String)>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            serve_one(s)
        });
        (port, h)
    }

    /// Como `fake_smtp`, pero acepta `n` correos (uno por conexión).
    pub(crate) fn fake_smtp_many(n: usize) -> (u16, std::thread::JoinHandle<Vec<(Vec<String>, String)>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || (0..n).map(|_| serve_one(l.accept().unwrap().0)).collect());
        (port, h)
    }

    #[test]
    fn sends_mail_with_attachment() {
        let (port, h) = fake_smtp();
        let cfg = MailConfig { enabled: true, host: "127.0.0.1".into(), port, security: Security::Ninguna, from_name: "Clínica Dental".into(), from_email: "clinica@example.com".into(), ..Default::default() };
        let files = vec![FileAttachment { name: "orden.zip".into(), mime: "application/zip".into(), bytes: b"PK\x03\x04prueba".to_vec() }];
        send(&cfg, "lab@example.com", "Orden de trabajo", "Adjuntamos la orden.", &files).unwrap();
        let (cmds, data) = h.join().unwrap();
        assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("MAIL FROM:<CLINICA@EXAMPLE.COM>")));
        assert!(cmds.iter().any(|c| c.to_uppercase().starts_with("RCPT TO:<LAB@EXAMPLE.COM>")));
        assert!(data.contains("Subject: Orden de trabajo"));
        assert!(data.contains("orden.zip"), "lleva el adjunto");
        assert!(data.contains("Content-Type: application/zip"));
    }

    #[test]
    fn rejects_bad_addresses() {
        let cfg = MailConfig { enabled: true, host: "x".into(), from_email: "clinica@example.com".into(), ..Default::default() };
        assert!(build(&cfg, "", "s", "t", &[]).is_err());
        assert!(build(&cfg, "no-es-un-correo", "s", "t", &[]).is_err());
        assert!(build(&cfg, "a@b.com; c@d.com", "s", "t", &[]).is_ok());
    }
}

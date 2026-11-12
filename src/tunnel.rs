//! Publicar en internet la página de la clínica (cita online, «Mi cita»,
//! presupuestos para firmar) con Cloudflare Tunnel, sin abrir puertos en el
//! router: el programa `cloudflared` de Cloudflare abre una conexión cifrada
//! desde este ordenador y Cloudflare sirve la dirección https.
//!
//! Dos formas:
//! - Túnel con nombre (lo normal): la clínica lo crea gratis en su cuenta de
//!   Cloudflare con su dominio (p. ej. citas.suclinica.es) y pega aquí el
//!   código (token). La dirección no cambia nunca.
//! - Dirección temporal (para probar): `cloudflared` da una dirección
//!   aleatoria *.trycloudflare.com que cambia cada vez; no sirve para enlaces
//!   que se envían a los pacientes.
//!
//! El token se pasa por la variable de entorno TUNNEL_TOKEN (no aparece en la
//! lista de procesos) y nunca se escribe en el registro.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Túnel de la cuenta de Cloudflare de la clínica (con token).
    Named,
    /// Dirección temporal *.trycloudflare.com para probar.
    Quick,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    /// Conexiones activas con Cloudflare (suelen ser 4).
    pub connections: u32,
    /// Dirección temporal (modo de prueba).
    pub url: Option<String>,
    pub error: Option<String>,
    /// El programa terminó (con este motivo).
    pub exited: Option<String>,
    /// Últimas líneas del registro.
    pub log: Vec<String>,
}

pub struct Tunnel {
    pub mode: Mode,
    child: Child,
    pub state: Arc<Mutex<State>>,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Tunnel {
    pub fn snapshot(&mut self) -> State {
        if let Ok(Some(st)) = self.child.try_wait() && let Ok(mut g) = self.state.lock() && g.exited.is_none() {
            g.exited = Some(format!("cloudflared se ha cerrado ({st})."));
            g.connections = 0;
        }
        self.state.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" }
}

/// Busca `cloudflared`: la ruta indicada, el PATH y los sitios donde lo dejan sus instaladores.
pub fn find(custom: &str) -> Option<PathBuf> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        return p.is_file().then_some(p);
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if cfg!(windows) {
        for base in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(b) = std::env::var_os(base) {
                dirs.push(PathBuf::from(b).join("cloudflared"));
            }
        }
    } else {
        for d in ["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"] {
            dirs.push(PathBuf::from(d));
        }
    }
    dirs.into_iter().map(|d| d.join(exe_name())).find(|p| p.is_file())
}

/// Cómo instalarlo en este sistema.
pub fn install_hint() -> &'static str {
    if cfg!(windows) {
        "Instálelo con el instalador de Cloudflare (cloudflared para Windows) o, en una ventana de comandos, con: winget install --id Cloudflare.cloudflared"
    } else if cfg!(target_os = "macos") {
        "Instálelo con: brew install cloudflared (o el paquete de Cloudflare para macOS)."
    } else {
        "Instálelo con el paquete de Cloudflare para su distribución (cloudflared .deb o .rpm)."
    }
}

/// Lee una línea del registro de `cloudflared` y actualiza el estado.
pub fn parse_line(st: &mut State, line: &str, secret: &str) {
    let mut line = line.trim().to_string();
    if !secret.is_empty() && line.contains(secret) {
        line = line.replace(secret, "•••");
    }
    if line.is_empty() {
        return;
    }
    if line.contains("Registered tunnel connection") {
        st.connections = (st.connections + 1).min(8);
        st.error = None;
    } else if line.contains("Unregistered tunnel connection") || line.contains("Connection terminated") {
        st.connections = st.connections.saturating_sub(1);
    }
    if let Some(i) = line.find("https://") {
        let url: String = line[i..].chars().take_while(|c| !c.is_whitespace() && *c != '|').collect();
        if url.ends_with(".trycloudflare.com") {
            st.url = Some(url);
        }
    }
    if let Some(i) = line.find(" ERR ") {
        let msg = line[i + 5..].trim();
        st.error = Some(friendly_error(msg));
    }
    st.log.push(line);
    if st.log.len() > 60 {
        let n = st.log.len() - 60;
        st.log.drain(..n);
    }
}

fn friendly_error(msg: &str) -> String {
    let l = msg.to_lowercase();
    if l.contains("token") && (l.contains("invalid") || l.contains("not valid") || l.contains("malformed")) {
        "El código (token) del túnel no es válido: cópielo de nuevo desde Cloudflare.".into()
    } else if l.contains("unauthorized") || l.contains("authentication") {
        "Cloudflare no acepta ese túnel: compruebe el código o créelo de nuevo.".into()
    } else if l.contains("dial") || l.contains("timeout") || l.contains("no such host") || l.contains("network") {
        format!("No se puede llegar a Cloudflare (¿hay internet o un cortafuegos?): {msg}")
    } else {
        msg.to_string()
    }
}

/// Arranca `cloudflared`. En modo con nombre usa el token; en modo de prueba
/// sirve `http://127.0.0.1:{port}` en una dirección temporal.
pub fn start(exe: &Path, mode: Mode, port: u16, token: &str) -> Result<Tunnel, String> {
    let mut cmd = Command::new(exe);
    cmd.arg("tunnel").arg("--no-autoupdate");
    match mode {
        Mode::Named => {
            if token.trim().is_empty() {
                return Err("Falta el código (token) del túnel.".into());
            }
            cmd.arg("run").env("TUNNEL_TOKEN", token.trim());
        }
        Mode::Quick => {
            cmd.arg("--url").arg(format!("http://127.0.0.1:{port}"));
        }
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // sin ventana de consola
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("No se pudo arrancar cloudflared ({}): {e}", exe.display()))?;
    let state = Arc::new(Mutex::new(State::default()));
    let secret = token.trim().to_string();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let st = state.clone();
        let secret = secret.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if let Ok(mut g) = st.lock() {
                    parse_line(&mut g, &line, &secret);
                }
            }
        });
    }
    Ok(Tunnel { mode, child, state })
}

/// Comprueba desde fuera que la dirección pública llega a la página: pide los
/// datos de la clínica por la dirección https (ida y vuelta por internet).
pub fn check_public(base: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/');
    if !base.starts_with("https://") {
        return Err("La dirección para los pacientes debe empezar por https://".into());
    }
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)).build();
    let r = agent.get(&format!("{base}/api/datos")).call().map_err(|e| match e {
        ureq::Error::Status(c, _) => format!("La dirección responde, pero no con la página de la clínica (código {c}). Revise en Cloudflare que el «Public hostname» apunte a http://localhost con el puerto de la página."),
        e => format!("No se puede abrir {base} desde internet: {e}"),
    })?;
    let v: serde_json::Value = serde_json::from_str(&r.into_string().map_err(|e| e.to_string())?).map_err(|_| "La dirección responde, pero no es la página de VeodarDental.".to_string())?;
    match v["clinica"].as_str() {
        Some(c) => Ok(format!("Funciona: desde internet se ve la página de «{c}».")),
        None => Err("La dirección responde, pero no es la página de VeodarDental.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_lines_are_understood() {
        let mut st = State::default();
        parse_line(&mut st, "2026-11-08T10:00:00Z INF Starting tunnel tunnelID=abc", "SECRETO");
        parse_line(&mut st, "2026-11-08T10:00:01Z INF Registered tunnel connection connIndex=0 connection=x event=0 ip=198.41.200.13 location=mad01 protocol=quic", "SECRETO");
        parse_line(&mut st, "2026-11-08T10:00:01Z INF Registered tunnel connection connIndex=1 connection=y event=0 ip=198.41.192.7 location=mad05 protocol=quic", "SECRETO");
        assert_eq!(st.connections, 2);
        parse_line(&mut st, "2026-11-08T10:05:00Z WRN Connection terminated error=\"timeout\" connIndex=1", "SECRETO");
        assert_eq!(st.connections, 1);
        parse_line(&mut st, "2026-11-08T10:00:00Z INF |  https://brave-lion-sky-moon.trycloudflare.com                                     |", "");
        assert_eq!(st.url.as_deref(), Some("https://brave-lion-sky-moon.trycloudflare.com"));
        parse_line(&mut st, "2026-11-08T10:00:00Z ERR Provided Tunnel token is not valid. token=SECRETO", "SECRETO");
        assert!(st.error.as_deref().unwrap().contains("no es válido"));
        assert!(st.log.iter().all(|l| !l.contains("SECRETO")), "el token nunca va al registro");
    }

    /// Con un «cloudflared» de mentira: arranca, se lee su registro, el token
    /// llega por la variable de entorno y se cierra al soltar el túnel.
    #[cfg(unix)]
    #[test]
    fn runs_and_stops_cloudflared() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("veodardental-tunel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("cloudflared");
        std::fs::write(&exe, "#!/bin/sh\necho \"2026-11-08T10:00:00Z INF args: $*\" >&2\necho \"2026-11-08T10:00:00Z INF token=$TUNNEL_TOKEN\" >&2\necho \"2026-11-08T10:00:01Z INF Registered tunnel connection connIndex=0\" >&2\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(find(exe.to_str().unwrap()), Some(exe.clone()));
        let mut t = start(&exe, Mode::Named, 8791, "abc123secreto").unwrap();
        let mut st = State::default();
        for _ in 0..100 {
            st = t.snapshot();
            if st.connections > 0 && st.log.len() >= 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        assert_eq!(st.connections, 1, "{:?}", st.log);
        assert!(st.log.iter().any(|l| l.contains("args: tunnel --no-autoupdate run")));
        assert!(st.log.iter().any(|l| l.contains("token=•••")), "el token llega por el entorno y no se muestra: {:?}", st.log);
        assert!(st.exited.is_none());
        drop(t);
        assert!(start(&exe, Mode::Named, 8791, "").is_err(), "sin token no arranca");
        let q = start(&exe, Mode::Quick, 8791, "").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(q.state.lock().unwrap().log.iter().any(|l| l.contains("--url http://127.0.0.1:8791")));
        drop(q);
        let _ = std::fs::remove_file(&exe);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn public_check_needs_https() {
        assert!(check_public("http://citas.example.com").is_err());
        assert!(check_public("").is_err());
    }
}

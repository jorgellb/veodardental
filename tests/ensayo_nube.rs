//! T-F6-02. Ensayo en local del servidor tal como iría en la nube: el
//! programa de verdad como proceso aparte, sin ventana, con sus carpetas en
//! un directorio aislado (como el usuario del servicio) y la zona horaria de
//! la clínica. Nada sale de este ordenador: todo va por 127.0.0.1.
//!
//! - Sin datos de la clínica no arranca (salvo `--datos-de-ejemplo`).
//! - Con la salida redirigida (como servicio) no escribe la clave de la clínica.
//! - Se comprueba desde fuera con `--estado-servidor`: con la clave buena y la
//!   huella, sí; con otra clave o con otra huella, no.
//! - Se para con `--parar-servidor` y vuelve a arrancar con los mismos datos
//!   y la misma identidad; los ficheros quedan solo para su usuario.
//! - El aprovisionamiento (`deploy/nube/aprovisionar.sh --ensayo`) deja el
//!   programa, la unidad de systemd y las carpetas sin tocar el sistema, y
//!   el programa arranca con el entorno de esa unidad.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const EXE: &str = env!("CARGO_BIN_EXE_veodardental");

/// Dónde está el programa y con qué variables arranca.
struct Env {
    exe: PathBuf,
    vars: Vec<(String, PathBuf)>,
}

impl Env {
    /// Las carpetas del servicio debajo de `root`.
    fn isolated(root: &Path) -> Env {
        let vars = [("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "datos"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "estado"), ("VEODARDENTAL_RAIZ", "veodardental")];
        for (_, d) in vars {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        Env { exe: EXE.into(), vars: vars.iter().map(|(k, d)| (k.to_string(), root.join(d))).collect() }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.exe);
        c.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default()).env("TZ", "Europe/Madrid");
        // En Windows, sin estas variables (que un servicio siempre tiene) la red
        // no se inicia y el servidor no puede abrir su puerto.
        #[cfg(windows)]
        for k in ["SystemRoot", "SystemDrive", "windir", "TEMP", "TMP"] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        for (k, v) in &self.vars {
            c.env(k, v);
        }
        c
    }

    /// Arranca el servidor y espera a que diga que atiende a los puestos (lo
    /// de antes, como la hora, no lo garantiza: en Windows la comprobación del
    /// disco va después). Devuelve el proceso y lo que escribió hasta entonces.
    fn start(&self, port: u16, extra: &[&str]) -> (Child, Vec<String>) {
        let mut child = self.cmd().arg("--servidor").arg("--puerto").arg(port.to_string()).args(extra).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let out = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut lines = vec![];
        let until = Instant::now() + Duration::from_secs(60);
        let mut started = false;
        while Instant::now() < until && !started {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(l) => {
                    started = l.contains("Atendiendo a los puestos.");
                    lines.push(l);
                }
                Err(mpsc::RecvTimeoutError::Timeout) if matches!(child.try_wait(), Ok(Some(_))) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if !started {
            // si no arrancó, no queda un proceso suelto
            let _ = child.kill();
            let st = child.wait();
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut e, &mut err);
            }
            panic!("el servidor no arrancó ({st:?}): {lines:?} · {err}");
        }
        (child, lines)
    }

    /// `--estado-servidor` con la clave en la variable (nunca en la orden).
    fn status(&self, port: u16, key: &str, pin: &str) -> (bool, String) {
        let o = self.cmd().env("VEODARDENTAL_CLAVE", key).arg("--estado-servidor").arg(format!("127.0.0.1:{port}")).arg(pin).output().unwrap();
        (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
    }

    /// Lo mismo, con la clave por la entrada.
    fn status_stdin(&self, port: u16, key: &str) -> (bool, String) {
        use std::io::Write;
        let mut c = self.cmd().arg("--estado-servidor").arg(format!("127.0.0.1:{port}")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        writeln!(c.stdin.take().unwrap(), "{key}").unwrap();
        let o = c.wait_with_output().unwrap();
        (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
    }

    fn key(&self) -> String {
        String::from_utf8(self.cmd().arg("--clave-servidor").output().unwrap().stdout).unwrap().trim().to_string()
    }

    fn stop(&self, port: u16, child: &mut Child) {
        let o = self.cmd().arg("--parar-servidor").arg(format!("127.0.0.1:{port}")).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let until = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until, "el servidor no se paró");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn fresh(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vd_ensayo_nube_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Huella (como la escribe el servidor) de su línea «Huella del servidor: …».
fn printed_fingerprint(lines: &[String]) -> String {
    let l = lines.iter().find(|l| l.contains("Huella del servidor")).unwrap();
    l.split(": ").nth(1).unwrap().split(" (").next().unwrap().trim().to_string()
}

#[cfg(unix)]
fn mode(p: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_cloud_server_rehearsal_runs_locally() {
    // 1. un servidor nuevo, sin los datos de la clínica: no arranca
    let empty = fresh("vacio");
    let o = Env::isolated(&empty).cmd().arg("--servidor").arg("--puerto").arg(free_port().to_string()).output().unwrap();
    assert!(!o.status.success(), "sin datos no arranca");
    assert!(String::from_utf8_lossy(&o.stderr).contains("No hay datos de la clínica"), "{}", String::from_utf8_lossy(&o.stderr));
    let _ = std::fs::remove_dir_all(&empty);

    // 2. ensayo con datos de ejemplo, como servicio (salida redirigida)
    let root = fresh("servicio");
    let env = Env::isolated(&root);
    let port = free_port();
    let (mut server, lines) = env.start(port, &["--datos-de-ejemplo"]);
    let key = env.key();
    assert!(key.len() >= 8, "la clave se puede ver en el propio ordenador: {key:?}");
    assert!(!lines.iter().any(|l| l.contains(&key)), "la clave no va al registro: {lines:?}");
    assert!(lines.iter().any(|l| l.contains("TZ=Europe/Madrid")), "dice su zona horaria: {lines:?}");
    let fp = printed_fingerprint(&lines);

    // 3. comprobado desde fuera: con la clave buena, sí (y la misma huella)
    let (ok, out) = env.status(port, &key, "");
    assert!(ok, "{out}");
    assert!(out.contains(&fp), "la huella que ve el puesto es la del servidor: {out} / {fp}");
    // con otra clave, no; con otra huella, tampoco
    let (ok, out) = env.status(port, "CLAVE-QUE-NO-ES", "");
    assert!(!ok, "{out}");
    let (ok, out) = env.status(port, &key, &"ab".repeat(32));
    assert!(!ok, "con otra huella no conecta: {out}");
    // la clave también por la entrada; sin clave, no
    let (ok, out) = env.status_stdin(port, &key);
    assert!(ok, "{out}");
    let (ok, out) = env.status_stdin(port, "");
    assert!(!ok && out.contains("Falta la clave"), "{out}");

    // 4. se para y vuelve a arrancar con sus datos y su identidad
    env.stop(port, &mut server);
    let (mut again, lines2) = env.start(port, &[]);
    assert_eq!(printed_fingerprint(&lines2), fp, "la misma identidad");
    let (ok, out) = env.status(port, &key, "");
    assert!(ok, "{out}");

    // 5. los ficheros del servicio, solo para su usuario
    #[cfg(unix)]
    {
        let mut checked = 0;
        let mut stack = vec![root.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if ["clinica.json", "servidor.json", "datos_claves.json"].contains(&p.file_name().and_then(|n| n.to_str()).unwrap_or("")) {
                    assert_eq!(mode(&p), 0o600, "{}", p.display());
                    checked += 1;
                }
            }
        }
        assert!(checked >= 2, "están los datos y la configuración del servidor");
    }
    env.stop(port, &mut again);
    let _ = std::fs::remove_dir_all(&root);
}

/// El aprovisionamiento en ensayo: no toca el sistema, deja el programa, la
/// unidad (con el puerto, el usuario, la zona horaria y el endurecimiento) y
/// las carpetas solo para el servicio, y el programa arranca con el entorno
/// de esa unidad.
#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "el aprovisionamiento es para servidores Linux (systemd): en otros sistemas las carpetas del programa no siguen las variables de la unidad")]
fn the_provisioning_script_rehearses_without_touching_the_system() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/nube/aprovisionar.sh");
    let dir = fresh("aprovisionar");
    let port = free_port();
    let o = Command::new("sh").arg(&script).args(["--ensayo"]).arg(&dir).arg("--binario").arg(EXE).args(["--puerto", &port.to_string(), "--permitir", "203.0.113.10"]).output().unwrap();
    assert!(o.status.success(), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    // las órdenes del sistema solo se apuntan; el servicio no se arranca sin los datos
    let orders = std::fs::read_to_string(dir.join("ordenes.txt")).unwrap();
    for want in [
        "useradd --system",
        "/usr/sbin/nologin",
        "install -d -m 700 -o veodardental -g veodardental /var/lib/veodardental",
        "systemctl daemon-reload",
        "systemctl enable veodardental-servidor.service",
        "ufw allow from 203.0.113.10",
        &format!("ufw deny {port}/tcp"),
    ] {
        assert!(orders.contains(want), "falta «{want}»: {orders}");
    }
    assert!(!orders.contains("systemctl start"), "no arranca antes de llevar los datos");
    assert!(!orders.contains("chown -R") && !orders.contains("chmod -R"), "como administrador no recorre lo que es del servicio: {orders}");
    let unit = std::fs::read_to_string(dir.join("etc/systemd/system/veodardental-servidor.service")).unwrap();
    for want in [
        &format!("--servidor --puerto {port}"),
        "User=veodardental",
        "Environment=TZ=Europe/Madrid",
        "UMask=0077",
        "NoNewPrivileges=yes",
        "ProtectSystem=strict",
        "ReadWritePaths=/var/lib/veodardental",
        "StateDirectory=veodardental",
        "StateDirectoryMode=0700",
    ] {
        assert!(unit.contains(want), "falta «{want}» en la unidad");
    }
    assert!(!unit.contains("datos-de-ejemplo"), "el servicio nunca arranca con datos de ejemplo");
    #[cfg(unix)]
    {
        assert_eq!(mode(&dir.join("var/lib/veodardental")), 0o700);
        assert_eq!(mode(&dir.join("opt/veodardental/veodardental")) & 0o111, 0o111);
    }
    // el programa instalado arranca con las variables de la unidad (las rutas, dentro del ensayo)
    let vars: Vec<(String, PathBuf)> = unit.lines().filter_map(|l| l.strip_prefix("Environment=")).filter_map(|kv| kv.split_once('=')).filter(|(k, _)| *k != "TZ").map(|(k, v)| (k.to_string(), dir.join(v.trim_start_matches('/')))).collect();
    assert!(vars.iter().any(|(k, _)| k == "XDG_DATA_HOME"));
    for (_, p) in &vars {
        std::fs::create_dir_all(p).unwrap();
    }
    let env = Env { exe: dir.join("opt/veodardental/veodardental"), vars };
    let (mut server, lines) = env.start(port, &["--datos-de-ejemplo"]);
    assert!(lines.iter().any(|l| l.contains(&format!("{}", dir.join("var/lib/veodardental/datos").display()))), "los datos, en su carpeta: {lines:?}");
    let (ok, out) = env.status(port, &env.key(), "");
    assert!(ok, "{out}");
    env.stop(port, &mut server);
    // si la carpeta del servicio es un enlace, no se toca
    #[cfg(unix)]
    {
        let trap = fresh("enlace");
        let target = trap.join("otro_sitio");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(trap.join("var/lib")).unwrap();
        std::os::unix::fs::symlink(&target, trap.join("var/lib/veodardental")).unwrap();
        let o = Command::new("sh").arg(&script).arg("--ensayo").arg(&trap).arg("--binario").arg(EXE).output().unwrap();
        assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("es un enlace"), "{}", String::from_utf8_lossy(&o.stderr));
        let _ = std::fs::remove_dir_all(&trap);
    }
    // opciones que no valen
    for bad in [vec!["--ensayo", dir.to_str().unwrap(), "--binario", "/no/existe"], vec!["--ensayo", dir.to_str().unwrap(), "--binario", EXE, "--puerto", "ochenta"]] {
        let o = Command::new("sh").arg(&script).args(&bad).output().unwrap();
        assert_eq!(o.status.code(), Some(2), "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

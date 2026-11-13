//! Importador desde Microsoft SQL Server.
//!
//! Para pasar a VeodarDental desde GESDEN o Novaclinic (Infomed), cuya
//! estructura se conoce (tablas `TTos`, `Clientes`, `Pacientes`, `TGrupos`,
//! `TTipoOdg`…), o desde cualquier otro programa de gestión de clínicas que
//! guarde sus datos en SQL Server: se elige una tabla o se escribe una
//! consulta y sus columnas se asignan a los campos de VeodarDental con el mismo
//! asistente que los ficheros Excel/CSV.
//!
//! Solo se lee (consultas SELECT); en el programa anterior no se cambia nada.
//! Protocolo TDS 7.3 (SQL Server 2008 y posteriores), con instancias con
//! nombre (SERVIDOR\INSTANCIA, vía SQL Browser), usuario de SQL Server o de
//! Windows (NTLM) y cifrado TLS cuando el servidor lo admite.

use crate::extra::Sheet;
use crate::gesden::{GCatalogo, GTarifa, GTto};
use serde::{Deserialize, Serialize};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, FromSql};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SqlSource {
    /// «SERVIDOR\GESDEN», «192.168.1.10», «192.168.1.10,1433» o «host:puerto».
    pub server: String,
    pub database: String,
    pub user: String,
    /// No se guarda en ningún fichero.
    #[serde(skip)]
    pub password: String,
    /// Usuario de Windows (DOMINIO\usuario) en lugar de usuario de SQL Server.
    pub windows_auth: bool,
    /// 0 sin cifrado (servidores antiguos), 1 cifrar si el servidor lo admite, 2 cifrado obligatorio.
    pub encrypt: u8,
}

impl Default for SqlSource {
    fn default() -> Self {
        SqlSource { server: String::new(), database: "GELITE".into(), user: "sa".into(), password: String::new(), windows_auth: false, encrypt: 1 }
    }
}

/// (servidor, puerto, instancia).
pub fn parse_server(s: &str) -> Result<(String, Option<u16>, Option<String>), String> {
    let s = s.trim().trim_start_matches("tcp:");
    if s.is_empty() {
        return Err("Escriba el nombre o la dirección del servidor.".into());
    }
    if let Some((h, i)) = s.split_once('\\') {
        return Ok((if h == "." || h.eq_ignore_ascii_case("(local)") { "localhost".into() } else { h.to_string() }, None, Some(i.to_string())));
    }
    let split = s.rsplit_once(',').or_else(|| if s.matches(':').count() == 1 { s.rsplit_once(':') } else { None });
    if let Some((h, p)) = split {
        let port = p.trim().parse::<u16>().map_err(|_| format!("Puerto no válido: {p}"))?;
        return Ok((h.trim().to_string(), Some(port), None));
    }
    Ok((s.to_string(), None, None))
}

fn config(src: &SqlSource) -> Result<(Config, bool), String> {
    let (host, port, instance) = parse_server(&src.server)?;
    let mut c = Config::new();
    c.host(host);
    if let Some(p) = port {
        c.port(p);
    }
    let named = instance.is_some();
    if let Some(i) = instance {
        c.instance_name(i);
    }
    if !src.database.trim().is_empty() {
        c.database(src.database.trim());
    }
    c.application_name("VeodarDental (importación, solo lectura)");
    c.authentication(if src.windows_auth { windows_auth(src)? } else { AuthMethod::sql_server(src.user.trim(), &src.password) });
    c.encryption(encryption_level(src.encrypt));
    // SQL Server crea un certificado propio al instalarse (sin entidad que lo
    // firme) y así están casi todos los de las clínicas: si se exigiera uno
    // firmado no se podría importar de ninguno. La conexión es de solo lectura,
    // en la red de la clínica y para una importación puntual.
    c.trust_cert();
    Ok((c, named))
}

/// Opción de cifrado de la pantalla → nivel de tiberius. «Si el servidor lo
/// admite» cifra toda la sesión (no solo el inicio de sesión, que es `Off`).
fn encryption_level(option: u8) -> EncryptionLevel {
    match option {
        0 => EncryptionLevel::NotSupported,
        2 => EncryptionLevel::Required,
        _ => EncryptionLevel::On,
    }
}

/// Usuario de Windows: el indicado (DOMINIO\usuario) o, si se deja vacío, el
/// de la sesión actual. Solo en Windows.
#[cfg(windows)]
fn windows_auth(src: &SqlSource) -> Result<AuthMethod, String> {
    Ok(if src.user.trim().is_empty() { AuthMethod::Integrated } else { AuthMethod::windows(src.user.trim(), &src.password) })
}

#[cfg(not(windows))]
fn windows_auth(_: &SqlSource) -> Result<AuthMethod, String> {
    Err("La entrada con usuario de Windows solo está disponible en el programa para Windows; use un usuario de SQL Server.".into())
}

fn err_text(e: tiberius::error::Error) -> String {
    let t = e.to_string();
    let l = t.to_lowercase();
    if l.contains("18456") || l.contains("login failed") {
        "El servidor no acepta ese usuario o contraseña.".into()
    } else if l.contains("4060") || l.contains("cannot open database") {
        "Esa base de datos no existe en el servidor o el usuario no tiene acceso.".into()
    } else if l.contains("tls") || l.contains("handshake") || l.contains("certificate") {
        format!("Fallo al cifrar la conexión ({t}). Con servidores antiguos elija «Sin cifrado».")
    } else {
        format!("Error del servidor SQL: {t}")
    }
}

async fn connect(src: &SqlSource) -> Result<Client<Compat<TcpStream>>, String> {
    let (cfg, named) = config(src)?;
    let tcp = if named {
        use tiberius::SqlBrowser;
        TcpStream::connect_named(&cfg).await.map_err(|e| format!("No se encuentra la instancia «{}» (¿está activo el servicio SQL Browser?): {e}", src.server.trim()))?
    } else {
        TcpStream::connect(cfg.get_addr()).await.map_err(|e| format!("No se puede conectar con {}: {e}", src.server.trim()))?
    };
    let _ = tcp.set_nodelay(true);
    Client::connect(cfg, tcp.compat_write()).await.map_err(err_text)
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())
}

/// Valor de una celda como texto (fechas dd/mm/aaaa).
fn cell_text(cd: ColumnData<'static>) -> String {
    use chrono::{NaiveDate, NaiveDateTime, Timelike};
    match cd {
        ColumnData::U8(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::I16(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::I32(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::I64(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::F32(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::F64(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::Bit(v) => v.map(|b| if b { "1" } else { "0" }.to_string()).unwrap_or_default(),
        ColumnData::String(v) => v.map(|s| s.trim().to_string()).unwrap_or_default(),
        ColumnData::Guid(v) => v.map(|g| g.to_string()).unwrap_or_default(),
        ColumnData::Numeric(v) => v.map(|n| n.to_string()).unwrap_or_default(),
        ColumnData::Binary(v) => v.map(|b| format!("({} bytes)", b.len())).unwrap_or_default(),
        ColumnData::Xml(v) => v.map(|x| x.to_string()).unwrap_or_default(),
        ColumnData::Date(_) => NaiveDate::from_sql(&cd).ok().flatten().map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default(),
        other => match NaiveDateTime::from_sql(&other) {
            Ok(Some(d)) if d.hour() == 0 && d.minute() == 0 && d.second() == 0 => d.format("%d/%m/%Y").to_string(),
            Ok(Some(d)) => d.format("%d/%m/%Y %H:%M").to_string(),
            _ => String::new(),
        },
    }
}

async fn query(client: &mut Client<Compat<TcpStream>>, sql: &str) -> Result<Sheet, String> {
    let mut stream = client.simple_query(sql).await.map_err(err_text)?;
    let headers: Vec<String> = stream.columns().await.map_err(err_text)?.map(|c| c.iter().map(|x| x.name().to_string()).collect()).unwrap_or_default();
    let rows = stream.into_first_result().await.map_err(err_text)?;
    Ok(Sheet { headers, rows: rows.into_iter().map(|r| r.into_iter().map(cell_text).collect()).collect() })
}

/// Una consulta solo de lectura (SELECT / WITH), para no tocar el programa anterior.
pub fn read_only(sql: &str) -> bool {
    let s = crate::model::fold(sql.trim_start());
    let first = s.split_whitespace().next().unwrap_or("");
    let forbidden = ["insert ", "update ", "delete ", "drop ", "alter ", "create ", "truncate ", "exec", "merge ", "grant ", "into "];
    (first == "select" || first == "with") && !forbidden.iter().any(|w| s.contains(w)) && !s.contains(';')
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detected {
    pub version: String,
    pub database: String,
    /// (tabla, filas)
    pub tables: Vec<(String, i64)>,
    /// Programa reconocido por sus tablas.
    pub program: Option<&'static str>,
}

impl Detected {
    pub fn rows(&self, table: &str) -> i64 {
        self.tables.iter().find(|(t, _)| t.eq_ignore_ascii_case(table)).map(|x| x.1).unwrap_or(0)
    }
}

const GESDEN_TABLES: [&str; 5] = ["TTos", "Clientes", "Pacientes", "TGrupos", "TTipoOdg"];

/// Conecta, dice qué versión y base de datos hay y reconoce el programa.
pub fn detect(src: &SqlSource) -> Result<Detected, String> {
    runtime()?.block_on(async {
        let mut c = connect(src).await?;
        let v = query(&mut c, "SELECT CAST(SERVERPROPERTY('ProductVersion') AS NVARCHAR(50)) AS v, CAST(SERVERPROPERTY('Edition') AS NVARCHAR(100)) AS e, DB_NAME() AS d").await?;
        let row = v.rows.first().cloned().unwrap_or_default();
        let t = query(&mut c, "SELECT s.name + '.' + t.name AS tabla, CAST(SUM(p.rows) AS BIGINT) AS filas FROM sys.tables t JOIN sys.schemas s ON s.schema_id = t.schema_id JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0, 1) GROUP BY s.name, t.name ORDER BY s.name, t.name").await?;
        let tables: Vec<(String, i64)> = t.rows.iter().map(|r| (r[0].trim_start_matches("dbo.").to_string(), r.get(1).and_then(|x| x.parse().ok()).unwrap_or(0))).collect();
        let has = |n: &str| tables.iter().any(|(x, _)| x.eq_ignore_ascii_case(n));
        let program = GESDEN_TABLES.iter().all(|n| has(n)).then_some("GESDEN / Novaclinic (Infomed)");
        Ok(Detected { version: format!("SQL Server {} {}", row.first().cloned().unwrap_or_default(), row.get(1).cloned().unwrap_or_default()), database: row.get(2).cloned().unwrap_or_default(), tables, program })
    })
}

/// Lee una consulta de solo lectura (como mucho `max` filas).
pub fn read_query(src: &SqlSource, sql: &str, max: usize) -> Result<Sheet, String> {
    if !read_only(sql) {
        return Err("Solo se permiten consultas de lectura (SELECT).".into());
    }
    let mut s = runtime()?.block_on(async {
        let mut c = connect(src).await?;
        query(&mut c, sql).await
    })?;
    s.rows.truncate(max);
    Ok(s)
}

/// Lee una tabla entera (como mucho `max` filas).
pub fn read_table(src: &SqlSource, table: &str, max: usize) -> Result<Sheet, String> {
    let quoted: Vec<String> = table.split('.').map(|p| format!("[{}]", p.trim_matches(|c| c == '[' || c == ']').replace(']', "]]"))).collect();
    read_query(src, &format!("SELECT TOP {max} * FROM {}", quoted.join(".")), max)
}

const GESDEN_TARIFAS: &str = "SELECT C.IdCli, ISNULL(C.Nombre,''), ISNULL(C.Apellidos,''), CASE WHEN ISNULL(C.Inactivo,'N')='S' THEN 1 ELSE 0 END, ISNULL(C.Tel1,''), (SELECT COUNT(*) FROM PacCli PC WHERE PC.IdCli=C.IdCli) FROM Clientes C WHERE C.TipoCli='M' ORDER BY C.IdCli";

const GESDEN_TRATAMIENTOS: &str = "SELECT T.IdCli, ISNULL(T.Codigo,''), ISNULL(T.DescripMed,''), ISNULL(CAST(T.DescripPac AS NVARCHAR(250)),''), ISNULL(G.Descripcio,''), ISNULL(O.Descripcio,''), ISNULL(T.NumFases,0), CAST(ISNULL(T.Precio,0) AS FLOAT), ISNULL(T.Duracion,0), ISNULL(T.Piezas,2), CASE WHEN ISNULL(T.Inactivo,'N')='S' THEN 1 ELSE 0 END, CAST(ISNULL(I.IVA,0) AS FLOAT), CASE WHEN T.Autoriz='S' THEN 1 ELSE 0 END FROM TTos T LEFT JOIN TGrupos G ON G.IdGrupo=T.IdGrupo LEFT JOIN TTipoOdg O ON O.IdTipoOdg=T.IdTipoOdg LEFT JOIN TTipoIVA I ON I.IdTipoIVA=T.IdTipoIVA ORDER BY T.IdCli, T.Codigo";

/// Pacientes de GESDEN con los nombres de columna que reconoce el asistente.
pub const GESDEN_PACIENTES: &str = "SELECT P.NumPac AS [Nº historial], P.Nombre AS [Nombre], P.Apellidos AS [Apellidos], P.NIF AS [NIF], CONVERT(VARCHAR(10), P.FecNacim, 103) AS [Fecha nacimiento], P.Sexo AS [Sexo], P.TelMovil AS [Móvil], P.Tel1 AS [Teléfono], P.Email AS [Email], P.Direccion AS [Dirección], P.CP AS [C.P.], POB.Descripcio AS [Población], PRO.Descripcio AS [Provincia], M.Apellidos AS [Mutua / tarifa], CAST(P.Notas AS NVARCHAR(4000)) AS [Notas] FROM Pacientes P LEFT JOIN TPoblaci POB ON POB.IdPoblacio = P.IdPoblacio LEFT JOIN TProvin PRO ON PRO.IdProvin = POB.IdProvin LEFT JOIN Clientes M ON M.IdCli = P.IdMutDefec AND M.TipoCli = 'M' ORDER BY P.IdPac";

fn num<T: std::str::FromStr + Default>(s: &str) -> T {
    s.trim().replace(',', ".").parse().unwrap_or_default()
}

/// Tratamientos y tarifas de GESDEN, listos para [`crate::model::Db::import_gesden_catalog`].
pub fn gesden_catalog(src: &SqlSource) -> Result<GCatalogo, String> {
    let (tar, tto) = runtime()?.block_on(async {
        let mut c = connect(src).await?;
        let a = query(&mut c, GESDEN_TARIFAS).await?;
        let b = query(&mut c, GESDEN_TRATAMIENTOS).await?;
        Ok::<_, String>((a, b))
    })?;
    catalog_from_rows(&tar.rows, &tto.rows, &format!("{} · {}", src.server.trim(), src.database.trim()))
}

pub fn catalog_from_rows(tarifas: &[Vec<String>], tratamientos: &[Vec<String>], origen: &str) -> Result<GCatalogo, String> {
    let mut list: Vec<GTarifa> =
        tarifas.iter().filter(|r| r.len() >= 6).map(|r| GTarifa { id: num(&r[0]), codigo: r[1].clone(), nombre: r[2].clone(), inactiva: num(&r[3]), telefono: r[4].clone(), pacientes: num(&r[5]), tratamientos: vec![] }).collect();
    for r in tratamientos.iter().filter(|r| r.len() >= 13) {
        let id: i64 = num(&r[0]);
        let Some(t) = list.iter_mut().find(|t| t.id == id) else { continue };
        t.tratamientos.push(GTto { codigo: r[1].clone(), desc_medico: r[2].clone(), grupo: r[4].clone(), tipo_odg: r[5].clone(), precio: num(&r[7]), minutos: num(&r[8]), piezas: num(&r[9]), inactivo: num(&r[10]) });
    }
    if list.is_empty() {
        return Err("La base de datos no tiene tarifas de GESDEN.".into());
    }
    Ok(GCatalogo { origen: origen.to_string(), tarifas: list })
}

/// Pacientes de GESDEN para el asistente de importación (la tarifa general
/// se llama «Privado» en VeodarDental).
pub fn gesden_patients(src: &SqlSource, cat: Option<&GCatalogo>) -> Result<Sheet, String> {
    let mut s = read_query(src, GESDEN_PACIENTES, 1_000_000)?;
    let base = cat.and_then(crate::gesden::base_tariff).map(|t| t.nombre.trim().to_string());
    let col = s.headers.iter().position(|h| h == "Mutua / tarifa");
    if let Some(ci) = col {
        for row in s.rows.iter_mut() {
            if let Some(v) = row.get_mut(ci) {
                if base.as_deref() == Some(v.trim()) || v.trim().is_empty() {
                    *v = "Privado".into();
                } else {
                    *v = crate::gesden::pretty_text(v.trim().trim_matches('-').trim());
                }
            }
        }
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_option_encrypts_the_whole_session() {
        assert!(matches!(encryption_level(SqlSource::default().encrypt), EncryptionLevel::On));
        assert!(matches!(encryption_level(2), EncryptionLevel::Required));
        assert!(matches!(encryption_level(0), EncryptionLevel::NotSupported));
    }

    #[test]
    fn server_names() {
        assert_eq!(parse_server("SERVIDOR\\GESDEN").unwrap(), ("SERVIDOR".into(), None, Some("GESDEN".into())));
        assert_eq!(parse_server(".\\SQLEXPRESS").unwrap().0, "localhost");
        assert_eq!(parse_server("192.168.1.10,1433").unwrap(), ("192.168.1.10".into(), Some(1433), None));
        assert_eq!(parse_server("127.0.0.1:14333").unwrap(), ("127.0.0.1".into(), Some(14333), None));
        assert_eq!(parse_server("servidor").unwrap(), ("servidor".into(), None, None));
        assert!(parse_server("").is_err());
        assert!(parse_server("host,abc").is_err());
    }

    #[test]
    fn only_reading_queries() {
        assert!(read_only("SELECT * FROM Pacientes"));
        assert!(read_only("  with x as (select 1 a) select * from x"));
        assert!(!read_only("DELETE FROM Pacientes"));
        assert!(!read_only("SELECT * INTO copia FROM Pacientes"));
        assert!(!read_only("SELECT 1; DROP TABLE Pacientes"));
        assert!(!read_only("EXEC sp_who"));
    }

    #[test]
    fn catalog_from_gesden_rows() {
        let tar = vec![vec!["2227".into(), "".into(), "- TARIFA PRIVADA -".into(), "0".into(), "".into(), "50".into()], vec!["83".into(), "AV2".into(), "MUTUA 2013".into(), "1".into(), "".into(), "3".into()]];
        let tto = vec![vec!["2227".into(), "BLANQ".into(), "BLANQUEAMIENTO".into(), "".into(), "ESTETICA DENTAL".into(), "Genérico".into(), "0".into(), "240".into(), "0".into(), "2".into(), "0".into(), "0".into(), "0".into()]];
        let cat = catalog_from_rows(&tar, &tto, "prueba").unwrap();
        assert_eq!(cat.tarifas[0].tratamientos[0].precio, 240.0);
        assert_eq!(crate::gesden::base_tariff(&cat).unwrap().id, 2227);
        assert!(catalog_from_rows(&[], &[], "x").is_err());
    }

    /// Prueba con un SQL Server de verdad: DS_MSSQL="servidor;base;usuario;contraseña".
    #[test]
    #[ignore]
    fn real_server() {
        let Ok(v) = std::env::var("DS_MSSQL") else { return };
        let p: Vec<&str> = v.split(';').collect();
        let src = SqlSource { server: p[0].into(), database: p[1].into(), user: p[2].into(), password: p[3].into(), windows_auth: false, encrypt: 1 };
        let d = detect(&src).unwrap();
        println!("{} · {} · {} tablas · {:?}", d.version, d.database, d.tables.len(), d.program);
        assert!(d.program.is_some());
        let cat = gesden_catalog(&src).unwrap();
        let base = crate::gesden::base_tariff(&cat).unwrap();
        println!("tarifas {} · base «{}» {} tratamientos ({} activos)", cat.tarifas.len(), base.nombre, base.tratamientos.len(), base.active_treatments());
        let mut db = crate::model::Db::sample(crate::model::today());
        let r = db.import_gesden_catalog(&cat, Default::default()).unwrap();
        println!("{}", r.summary());
        let pats = gesden_patients(&src, Some(&cat)).unwrap();
        println!("pacientes: {} filas, columnas {:?}", pats.rows.len(), pats.headers);
        let map = crate::extra::guess_mapping(crate::extra::ImportKind::Pacientes, &pats.headers);
        assert!(map.iter().take(4).all(|m| m.is_some()), "{map:?}");
        let t = read_table(&src, "dbo.TGrupos", 5).unwrap();
        assert!(t.rows.len() <= 5 && !t.headers.is_empty());
    }
}

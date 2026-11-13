//! Libros registro de IVA e IRPF en el formato electrónico común de la AEAT
//! (T-F4-07; ver `docs/normativa.md`): «Formatos de los Libros Registro de IVA
//! e IRPF», versión del 18/02/2026, y los diseños y la plantilla publicados en
//! la sede (`LSI.xlsx`, actualizado el 01/01/2026, y
//! `PLANTILLA_LIBROS_UNIFICADOS.xlsx`, del 09/09/2026).
//!
//! - **Qué se genera:** un XLSX para la gestoría o para importar en el Pre303 y
//!   en Renta WEB.
//! - **Hojas:** si la clínica es de una persona física, los libros unificados
//!   de IVA e IRPF (`EXPEDIDAS_INGRESOS` y `RECIBIDAS_GASTOS`, fichero «T»); si
//!   es una sociedad, los del IVA (`EXPEDIDAS` y `RECIBIDAS`, fichero «C»), con
//!   las columnas del IRPF vacías (validación 17 del diseño).
//! - **Cabecera:** la de la plantilla oficial (filas 1 a 3). Los datos van
//!   desde la fila 4.
//! - **Facturas emitidas:** exentas por el artículo 20 de la Ley del IVA (clave
//!   E1), con concepto de ingreso I01.
//! - **Gastos:** con su concepto G, y la cuota de IVA soportado no deducible (la
//!   actividad está exenta).
//!
//! Lo que el formato de A3, ContaSol o Sage no tiene documentado de forma
//! oficial no se implementa (B-11). Que la gestoría lo acepte también es externo.

use crate::model::{Db, sheet_cell};
use chrono::{Datelike, NaiveDate};

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    S(String),
    N(f64),
    E,
}

/// Cabecera de la hoja de facturas expedidas (plantilla oficial, filas 1 a 3; columna, texto).
const EXP_HEAD: [&[(&str, &str)]; 3] = [
    &[
        ("A", "Autoliquidación"),
        ("C", "Actividad"),
        ("F", "Tipo de Factura"),
        ("G", "Concepto de\nIngreso"),
        ("H", "Ingreso Computable"),
        ("I", "Fecha\nExpedición"),
        ("J", "Fecha\nOperación"),
        ("K", "Identificación de la Factura"),
        ("N", "NIF Destinatario"),
        ("Q", "Nombre\nDestinatario"),
        ("R", "Clave de Operación"),
        ("S", "Calificación de la Operación"),
        ("T", "Operación\nExenta"),
        ("U", "Total\nFactura"),
        ("V", "Base\nImponible"),
        ("W", "Tipo\nde IVA"),
        ("X", "Cuota IVA\nRepercutida"),
        ("Y", "Tipo de\nRecargo Eq."),
        ("Z", "Cuota Recargo Eq."),
        ("AA", "Cobro (Operación Criterio de Caja de IVA y/o artículo 7.2.1º de Reglamento del IRPF)"),
        ("AE", "Tipo Retención\ndel IRPF"),
        ("AF", "Importe Retenido\ndel IRPF"),
        ("AG", "Registro Acuerdo Facturación"),
        ("AH", "Inmueble"),
        ("AJ", "Refrencia Externa"),
    ],
    &[
        ("A", "Ejercicio"),
        ("B", "Periodo"),
        ("C", "Código"),
        ("D", "Tipo"),
        ("E", "Grupo o Epígrafe del IAE"),
        ("K", "Serie"),
        ("L", "Número"),
        ("M", "Número-Final"),
        ("N", "Tipo"),
        ("O", "Código País"),
        ("P", "Identificación"),
        ("AA", "Fecha"),
        ("AB", "Importe"),
        ("AC", "Medio Utilizado"),
        ("AD", "Identificación Medio Utilizado"),
        ("AH", "Situación"),
        ("AI", "Referencia Catastral"),
    ],
    &[
        ("A", "Decimal (4,0)"),
        ("B", "Alfanumérico (2)"),
        ("C", "Alfanumérico (1)"),
        ("D", "Alfanumérico (2)"),
        ("E", "Alfanumérico (4)"),
        ("F", "Alfanumérico (2)"),
        ("G", "Alfanumérico (3)"),
        ("H", "Decimal(12,2)"),
        ("I", "Fecha(dd/mm/yyyy)"),
        ("J", "Fecha(dd/mm/yyyy)"),
        ("K", "Alfanumérico (20)"),
        ("L", "Alfanumérico (20)"),
        ("M", "Alfanumérico (20)"),
        ("N", "Alfanumérico (2)"),
        ("O", "Alfanumérico (2)"),
        ("P", "Alfanumérico (20)"),
        ("Q", "Alfanumérico (40)"),
        ("R", "Alfanumérico (2)"),
        ("S", "Alfanumérico (2)"),
        ("T", "Alfanumérico (2)"),
        ("U", "Decimal(12,2)"),
        ("V", "Decimal(12,2)"),
        ("W", "Decimal(4,2)"),
        ("X", "Decimal(12,2)"),
        ("Y", "Decimal(4,2)"),
        ("Z", "Decimal(12,2)"),
        ("AA", "Fecha(dd/mm/yyyy)"),
        ("AB", "Decimal(12,2)"),
        ("AC", "Alfanumérico (2)"),
        ("AD", "Alfanumérico (34)"),
        ("AE", "Decimal(4,2)"),
        ("AF", "Decimal(12,2)"),
        ("AG", "Alfanumérico (15)"),
        ("AH", "Alfanumérico (1)"),
        ("AI", "Alfanumérico (20)"),
        ("AJ", "Alfanumérico (40)"),
    ],
];

/// Cabecera de la hoja de facturas recibidas (plantilla oficial, filas 1 a 3).
const REC_HEAD: [&[(&str, &str)]; 3] = [
    &[
        ("A", "Autoliquidación"),
        ("C", "Actividad"),
        ("F", "Tipo de Factura"),
        ("G", "Concepto de\nGasto"),
        ("H", "Gasto Deducible"),
        ("I", "Fecha\nExpedición"),
        ("J", "Fecha\nOperación"),
        ("K", "Identificación Factura del Expedidor"),
        ("M", "Fecha\nRecepción"),
        ("N", "Número\nRecepción"),
        ("O", "Número\nRecepción Final"),
        ("P", "NIF Expedidor"),
        ("S", "Nombre\nExpedidor"),
        ("T", "Clave de\nOperación"),
        ("U", "Bien de Inversión"),
        ("V", "Inversión del Sujeto Pasivo"),
        ("W", "Deducible en Periodo Posterior"),
        ("X", "Periodo Deducción"),
        ("Z", "Total\nFactura"),
        ("AA", "Base\nImponible"),
        ("AB", "Tipo\nde IVA"),
        ("AC", "Cuota IVA\nSoportado"),
        ("AD", "Cuota Deducible"),
        ("AE", "Tipo de\nRecargo Eq."),
        ("AF", "Cuota\nRecargo Eq."),
        ("AG", "Pago (Operación Criterio de Caja de IVA y/o artículo 7.2.1º de Reglamento del IRPF)"),
        ("AK", "Tipo Retención\ndel IRPF"),
        ("AL", "Importe Retenido\ndel IRPF"),
        ("AM", "Registro Acuerdo Facturación"),
        ("AN", "Inmueble"),
        ("AP", "RefErencia Externa"),
    ],
    &[
        ("A", "Ejercicio"),
        ("B", "Periodo"),
        ("C", "Código"),
        ("D", "Tipo"),
        ("E", "Grupo o Epígrafe del IAE"),
        ("K", "(Serie-Número)"),
        ("L", "Número-Final"),
        ("P", "Tipo"),
        ("Q", "Código País"),
        ("R", "Identificación"),
        ("X", "Ejercicio"),
        ("Y", "Periodo"),
        ("AG", "Fecha"),
        ("AH", "Importe"),
        ("AI", "Medio Utilizado"),
        ("AJ", "Identificación Medio Utilizado"),
        ("AN", "Situación"),
        ("AO", "Referencia Catastral"),
    ],
    &[
        ("A", "Decimal (4,0)"),
        ("B", "Alfanumérico (2)"),
        ("C", "Alfanumérico (1)"),
        ("D", "Alfanumérico (2)"),
        ("E", "Alfanumérico (4)"),
        ("F", "Alfanumérico (2)"),
        ("G", "Alfanumérico (3)"),
        ("H", "Decimal(12,2)"),
        ("I", "Fecha(dd/mm/yyyy)"),
        ("J", "Fecha(dd/mm/yyyy)"),
        ("K", "Alfanumérico (40)"),
        ("L", "Alfanumérico (20)"),
        ("M", "Fecha(dd/mm/yyyy)"),
        ("N", "Alfanumérico (20)"),
        ("O", "Alfanumérico (20)"),
        ("P", "Alfanumérico (2)"),
        ("Q", "Alfanumérico (2)"),
        ("R", "Alfanumérico (20)"),
        ("S", "Alfanumérico (40)"),
        ("T", "Alfanumérico (2)"),
        ("U", "Alfanumérico (1)"),
        ("V", "Alfanumérico (1)"),
        ("W", "Alfanumérico (1)"),
        ("X", "Decimal (4,0)"),
        ("Y", "Alfanumérico (2)"),
        ("Z", "Decimal(12,2)"),
        ("AA", "Decimal(12,2)"),
        ("AB", "Decimal(4,2)"),
        ("AC", "Decimal(12,2)"),
        ("AD", "Decimal(12,2)"),
        ("AE", "Decimal(4,2)"),
        ("AF", "Decimal(12,2)"),
        ("AG", "Fecha(dd/mm/yyyy)"),
        ("AH", "Decimal(12,2)"),
        ("AI", "Alfanumérico (2)"),
        ("AJ", "Alfanumérico (34)"),
        ("AK", "Decimal(4,2)"),
        ("AL", "Decimal(12,2)"),
        ("AM", "Alfanumérico (15)"),
        ("AN", "Alfanumérico (1)"),
        ("AO", "Alfanumérico (20)"),
        ("AP", "Alfanumérico (40)"),
    ],
];

/// Concepto de gasto (lista oficial) de cada categoría de gasto del programa.
/// Es una propuesta que revisa la gestoría (B-11).
pub fn expense_concept(category: &str) -> &'static str {
    match category {
        "Material" => "G01",
        "Laboratorio" => "G03",
        "Alquiler" => "G12",
        "Nóminas" => "G04",
        "Suministros" => "GY4",
        "Seguros" => "G20",
        "Marketing" => "G22",
        "Mantenimiento" => "G13",
        "Impuestos y tasas" => "G26",
        _ => "G37",
    }
}

/// Índice de una columna («A» → 0, «AJ» → 35).
pub fn col(name: &str) -> usize {
    name.bytes().fold(0, |n, b| n * 26 + (b - b'A' + 1) as usize) - 1
}

fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

/// ¿Es de una persona física? (el NIF de una sociedad empieza por letra salvo K, L, M, X, Y, Z)
pub fn physical(nif: &str) -> bool {
    let n = nif.trim().to_uppercase();
    n.chars().next().is_none_or(|c| c.is_ascii_digit() || "KLMXYZ".contains(c))
}

fn quarter(d: NaiveDate) -> String {
    format!("{}T", (d.month() - 1) / 3 + 1)
}

fn date(d: NaiveDate) -> Cell {
    Cell::S(d.format("%d/%m/%Y").to_string())
}

fn text(s: &str, max: usize) -> Cell {
    let t: String = sheet_cell(s.trim(), '\t').chars().take(max).collect();
    if t.is_empty() { Cell::E } else { Cell::S(t) }
}

fn norm_nif(s: &str) -> String {
    s.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// Datos de la actividad (columnas C, D y E).
#[derive(Clone, Debug)]
pub struct Activity {
    pub kind: String,
    pub epigrafe: String,
}

impl Activity {
    pub fn of(db: &Db) -> Activity {
        let c = &db.config;
        let kind = if c.actividad_tipo.trim().is_empty() { if physical(&c.nif) { "05" } else { "03" }.to_string() } else { c.actividad_tipo.trim().to_string() };
        Activity { kind, epigrafe: c.iae_epigrafe.trim().to_string() }
    }
}

/// Filas del libro de facturas expedidas (y de ventas e ingresos) de un año.
pub fn issued(db: &Db, year: i32) -> Vec<Vec<Cell>> {
    let act = Activity::of(db);
    let irpf = physical(&db.config.nif);
    let mut list: Vec<&crate::model::Invoice> = db.invoices.iter().filter(|i| i.date.year() == year && !crate::verifactu::cancelled(db, i.id)).collect();
    list.sort_by_key(|i| (i.date, i.series.clone(), i.number));
    list.into_iter()
        .map(|inv| {
            let mut r = vec![Cell::E; col("AJ") + 1];
            let base = (inv.base() * 100.0).round() / 100.0;
            let nif = norm_nif(&inv.client_nif);
            let kind = if inv.tipo.trim().is_empty() { if nif.is_empty() { "F2" } else { "F1" } } else { inv.tipo.trim() };
            let name = if inv.client_name.trim().is_empty() { &inv.patient_name } else { &inv.client_name };
            r[col("A")] = Cell::N(year as f64);
            r[col("B")] = Cell::S(quarter(inv.date));
            r[col("C")] = Cell::S("A".into());
            r[col("D")] = Cell::S(act.kind.clone());
            r[col("E")] = text(&act.epigrafe, 4);
            r[col("F")] = Cell::S(kind.into());
            if irpf {
                r[col("G")] = Cell::S("I01".into());
                r[col("H")] = Cell::N(base);
            }
            r[col("I")] = date(inv.date);
            r[col("K")] = text(&inv.series, 20);
            r[col("L")] = Cell::S(inv.number.to_string());
            if kind != "F2" && kind != "R5" {
                r[col("P")] = text(&nif, 20);
                r[col("Q")] = text(name, 40);
            }
            r[col("R")] = Cell::S("01".into());
            r[col("T")] = Cell::S("E1".into());
            r[col("U")] = Cell::N(base);
            r[col("V")] = Cell::N(base);
            r[col("AJ")] = text(&inv.code(), 40);
            r
        })
        .collect()
}

/// Filas del libro de facturas recibidas (y de compras y gastos) de un año.
pub fn received(db: &Db, year: i32) -> Vec<Vec<Cell>> {
    let act = Activity::of(db);
    let irpf = physical(&db.config.nif);
    let mut list: Vec<&crate::finance::Expense> = db.expenses.iter().filter(|e| e.date.year() == year).collect();
    list.sort_by_key(|e| (e.date, e.id));
    list.into_iter()
        .enumerate()
        .map(|(n, e)| {
            let mut r = vec![Cell::E; col("AP") + 1];
            let r2 = |v: f64| (v * 100.0).round() / 100.0;
            let nif = db.suppliers.iter().find(|s| crate::model::fold(s.name.trim()) == crate::model::fold(e.supplier.trim())).map(|s| norm_nif(&s.nif)).unwrap_or_default();
            r[col("A")] = Cell::N(year as f64);
            r[col("B")] = Cell::S(quarter(e.date));
            r[col("C")] = Cell::S("A".into());
            r[col("D")] = Cell::S(act.kind.clone());
            r[col("E")] = text(&act.epigrafe, 4);
            r[col("F")] = Cell::S("F1".into());
            if irpf {
                r[col("G")] = Cell::S(expense_concept(&e.category).into());
                r[col("H")] = Cell::N(r2(e.base));
            }
            r[col("I")] = date(e.date);
            r[col("K")] = text(&e.invoice_no, 40);
            r[col("M")] = date(e.date);
            r[col("N")] = Cell::S((n + 1).to_string());
            r[col("R")] = text(&nif, 20);
            r[col("S")] = text(&e.supplier, 40);
            r[col("T")] = Cell::S("01".into());
            r[col("U")] = Cell::S("N".into());
            r[col("V")] = Cell::S("N".into());
            r[col("Z")] = Cell::N(r2(e.base + e.vat()));
            r[col("AA")] = Cell::N(r2(e.base));
            if e.vat_pct > 0.0 {
                r[col("AB")] = Cell::N(e.vat_pct);
            }
            r[col("AC")] = Cell::N(r2(e.vat()));
            // actividad exenta (art. 20): el IVA soportado no se deduce
            r[col("AD")] = Cell::N(0.0);
            r[col("AP")] = Cell::S(format!("Gasto {}", e.id));
            r
        })
        .collect()
}

fn as_text(c: &Cell) -> Option<&str> {
    if let Cell::S(s) = c { Some(s) } else { None }
}

fn as_num(c: &Cell) -> f64 {
    if let Cell::N(n) = c { *n } else { 0.0 }
}

/// Largo máximo de cada columna de texto (fila 3 de la cabecera).
fn max_len(head: &[(&str, &str)], c: usize) -> Option<usize> {
    let t = head.iter().find(|(n, _)| col(n) == c)?.1;
    t.strip_prefix("Alfanumérico (")?.strip_suffix(')')?.parse().ok()
}

fn nif_ok(n: &str) -> bool {
    let d = n.chars().filter(|c| c.is_ascii_digit()).count();
    let first_or_last_letter = n.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) || n.chars().last().is_some_and(|c| c.is_ascii_alphabetic());
    n.len() == 9 && n.chars().all(|c| c.is_ascii_alphanumeric()) && d >= 7 && first_or_last_letter
}

/// Validaciones del diseño oficial (con su número) sobre un libro. Vacío: válido.
pub fn validate(issued_book: bool, rows: &[Vec<Cell>]) -> Vec<String> {
    let head = if issued_book { EXP_HEAD[2] } else { REC_HEAD[2] };
    let mut v = vec![];
    for (i, r) in rows.iter().enumerate() {
        let row = i + 4;
        for (c, cell) in r.iter().enumerate() {
            if let (Some(s), Some(max)) = (as_text(cell), max_len(head, c))
                && s.chars().count() > max
            {
                v.push(format!("Fila {row}, columna {}: más de {max} caracteres (validación 4).", col_name(c)));
            }
        }
        let parse = |c: &Cell| as_text(c).and_then(|s| NaiveDate::parse_from_str(s, "%d/%m/%Y").ok());
        let Some(exp) = parse(&r[col("I")]) else {
            v.push(format!("Fila {row}: fecha de expedición no válida (validación 5)."));
            continue;
        };
        if as_num(&r[col("A")]) as i32 != exp.year() || as_text(&r[col("B")]) != Some(quarter(exp).as_str()) {
            v.push(format!("Fila {row}: ejercicio o periodo distinto del de la fecha (validación 11)."));
        }
        if issued_book {
            let kind = as_text(&r[col("F")]).unwrap_or("");
            if ["F1", "F3", "R1", "R2", "R3", "R4"].contains(&kind) && !as_text(&r[col("P")]).is_some_and(nif_ok) {
                v.push(format!("Fila {row}: factura {kind} sin un NIF de destinatario válido (validaciones 2 y 33)."));
            }
            if as_text(&r[col("T")]) == Some("E1") && (as_num(&r[col("U")]) - as_num(&r[col("V")])).abs() > 0.005 {
                v.push(format!("Fila {row}: en una operación exenta el total es la base (validación 37)."));
            }
        } else {
            let (total, base, quota, ded) = (as_num(&r[col("Z")]), as_num(&r[col("AA")]), as_num(&r[col("AC")]), as_num(&r[col("AD")]));
            if (total - base - quota).abs() > 0.011 {
                v.push(format!("Fila {row}: el total no es base más cuota (validación 32)."));
            }
            let rate = as_num(&r[col("AB")]);
            if rate != 0.0 && (base * rate / 100.0 - quota).abs() > 0.011 {
                v.push(format!("Fila {row}: la cuota no es base por tipo (validación 33)."));
            }
            if ded > quota + 0.005 {
                v.push(format!("Fila {row}: cuota deducible mayor que la soportada (validación 33)."));
            }
            if !as_text(&r[col("R")]).is_some_and(nif_ok) {
                v.push(format!("Fila {row}: falta el NIF del proveedor (validación 2); complételo en Almacén › Proveedores."));
            }
            if matches!(r[col("K")], Cell::E) {
                v.push(format!("Fila {row}: falta el número de la factura del proveedor."));
            }
        }
    }
    v
}

fn esc(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').collect::<String>().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn sheet_xml(head: &[&[(&str, &str)]; 3], rows: &[Vec<Cell>]) -> String {
    let mut x = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
    let cell = |c: &str, r: usize, v: &Cell| match v {
        Cell::S(s) => format!(r#"<c r="{c}{r}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, esc(s)),
        Cell::N(n) => format!(r#"<c r="{c}{r}"><v>{n}</v></c>"#),
        Cell::E => String::new(),
    };
    for (i, h) in head.iter().enumerate() {
        x += &format!(r#"<row r="{}">"#, i + 1);
        for (c, t) in h.iter() {
            x += &cell(c, i + 1, &Cell::S(t.to_string()));
        }
        x += "</row>";
    }
    for (i, r) in rows.iter().enumerate() {
        x += &format!(r#"<row r="{}">"#, i + 4);
        for (c, v) in r.iter().enumerate() {
            x += &cell(&col_name(c), i + 4, v);
        }
        x += "</row>";
    }
    x + "</sheetData></worksheet>"
}

/// El XLSX con las dos hojas.
pub fn xlsx(names: [&str; 2], issued_rows: &[Vec<Cell>], received_rows: &[Vec<Cell>]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let files = [
        ("[Content_Types].xml".to_string(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_string()),
        ("_rels/.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/><sheet name="{}" sheetId="2" r:id="rId2"/></sheets></workbook>"#, names[0], names[1])),
        ("xl/_rels/workbook.xml.rels".into(), r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#.into()),
        ("xl/worksheets/sheet1.xml".into(), sheet_xml(&EXP_HEAD, issued_rows)),
        ("xl/worksheets/sheet2.xml".into(), sheet_xml(&REC_HEAD, received_rows)),
    ];
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, body) in files.iter() {
            z.start_file(n.as_str(), opt).map_err(|e| e.to_string())?;
            z.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// Nombre del fichero: ejercicio, NIF, tipo («T» unificados o «C» del IVA) y nombre.
pub fn file_name(year: i32, nif: &str, name: &str) -> String {
    let kind = if physical(nif) { 'T' } else { 'C' };
    let clean: String = name.chars().filter(|c| !"/\\:*?\"<>|".contains(*c) && !c.is_control()).collect();
    format!("{year}{}{kind}{}.xlsx", norm_nif(nif), clean.trim())
}

/// Genera los libros del año en `dir`. Devuelve el fichero y lo que no pasa
/// las validaciones (para corregirlo antes de entregarlo).
pub fn export(db: &Db, year: i32, dir: &std::path::Path) -> Result<(std::path::PathBuf, Vec<String>), String> {
    let (iss, rec) = (issued(db, year), received(db, year));
    let mut problems: Vec<String> = validate(true, &iss).into_iter().map(|p| format!("Expedidas: {p}")).collect();
    problems.extend(validate(false, &rec).into_iter().map(|p| format!("Recibidas: {p}")));
    if db.config.iae_epigrafe.trim().is_empty() {
        problems.insert(0, "Falta el epígrafe del IAE de la clínica (Configuración › Facturación y cobros).".into());
    }
    let names = if physical(&db.config.nif) { ["EXPEDIDAS_INGRESOS", "RECIBIDAS_GASTOS"] } else { ["EXPEDIDAS", "RECIBIDAS"] };
    let bytes = xlsx(names, &iss, &rec)?;
    if bytes.len() > 4 << 20 {
        problems.push("El fichero pasa de 4 MB: la AEAT no lo importa así.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name(year, &db.config.nif, &db.config.name));
    crate::paths::write_private(&path, &bytes).map_err(|e| e.to_string())?;
    Ok((path, problems))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Reader};

    fn db() -> Db {
        let mut db = Db::sample(crate::model::today());
        db.config.nif = "12345678Z".into();
        db.config.iae_epigrafe = "836".into();
        for s in db.suppliers.iter_mut() {
            if s.nif.trim().is_empty() {
                s.nif = "B12345678".into();
            }
        }
        db
    }

    #[test]
    fn columns_and_codes() {
        assert_eq!((col("A"), col("Z"), col("AA"), col("AJ"), col("AP")), (0, 25, 26, 35, 41));
        assert_eq!((col_name(0), col_name(25), col_name(26), col_name(41)), ("A".to_string(), "Z".to_string(), "AA".to_string(), "AP".to_string()));
        assert!(physical("12345678Z") && physical("X1234567L") && !physical("B12345678"));
        assert_eq!(file_name(2026, "12345678-Z", "Clínica Dental: Pérez"), "202612345678ZTClínica Dental Pérez.xlsx");
        assert_eq!(file_name(2026, "B12345678", "Sonrisa SL"), "2026B12345678CSonrisa SL.xlsx");
        assert!(nif_ok("12345678Z") && nif_ok("X1234567L") && nif_ok("B12345678") && !nif_ok("1234") && !nif_ok("12-345678Z"));
    }

    /// El fichero se lee como el de la AEAT: hojas con su nombre, cabecera
    /// idéntica a la de la plantilla, datos desde la fila 4 y lo que exige el
    /// diseño (exenta E1, total = base, ejercicio y periodo de la fecha).
    #[test]
    fn the_books_follow_the_official_layout_and_validations() {
        let db = db();
        let year = db.invoices.iter().map(|i| i.date.year()).max().unwrap();
        let dir = std::env::temp_dir().join(format!("ds_libros_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (path, problems) = export(&db, year, &dir).unwrap();
        assert!(path.file_name().unwrap().to_string_lossy().starts_with(&format!("{year}12345678ZT")));
        let iss = issued(&db, year);
        assert!(!iss.is_empty());
        // solo pueden quedar avisos de datos de ejemplo incompletos, nunca de formato
        assert!(problems.iter().all(|p| p.contains("NIF") || p.contains("número de la factura")), "{problems:?}");
        let mut wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
        assert_eq!(wb.sheet_names(), vec!["EXPEDIDAS_INGRESOS".to_string(), "RECIBIDAS_GASTOS".to_string()]);
        let r = wb.worksheet_range("EXPEDIDAS_INGRESOS").unwrap();
        let at = |row: usize, c: &str| r.get_value((row as u32, col(c) as u32)).cloned().unwrap_or(Data::Empty);
        for (i, h) in EXP_HEAD.iter().enumerate() {
            for (c, t) in h.iter() {
                assert_eq!(at(i, c), Data::String(t.to_string()), "fila {} {c}", i + 1);
            }
        }
        assert_eq!(r.height(), 3 + iss.len());
        assert_eq!(at(3, "T"), Data::String("E1".into()));
        assert_eq!(at(3, "R"), Data::String("01".into()));
        assert_eq!(at(3, "G"), Data::String("I01".into()));
        assert_eq!(at(3, "U"), at(3, "V"));
        let rr = wb.worksheet_range("RECIBIDAS_GASTOS").unwrap();
        assert_eq!(rr.get_value((0, col("AP") as u32)), Some(&Data::String("RefErencia Externa".into())));
        // lo que no cumple el diseño, se dice
        let mut bad = iss.clone();
        bad[0][col("U")] = Cell::N(1.0);
        bad[0][col("B")] = Cell::S("9T".into());
        bad[0][col("Q")] = Cell::S("x".repeat(41));
        let v = validate(true, &bad);
        assert!(v.iter().any(|p| p.contains("validación 37")) && v.iter().any(|p| p.contains("validación 11")) && v.iter().any(|p| p.contains("validación 4")), "{v:?}");
        let mut rec = received(&db, year);
        if let Some(first) = rec.first_mut() {
            first[col("Z")] = Cell::N(0.5);
            assert!(validate(false, &rec).iter().any(|p| p.contains("validación 32")));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Una sociedad: libros del IVA (fichero «C») sin las columnas del IRPF.
    #[test]
    fn a_company_gets_the_vat_books_without_income_tax_columns() {
        let mut db = db();
        db.config.nif = "B87654321".into();
        let year = db.invoices.iter().map(|i| i.date.year()).max().unwrap();
        let iss = issued(&db, year);
        assert!(iss.iter().all(|r| r[col("G")] == Cell::E && r[col("H")] == Cell::E));
        assert_eq!(Activity::of(&db).kind, "03");
        let dir = std::env::temp_dir().join(format!("ds_libros_c_{}", std::process::id()));
        let (path, _) = export(&db, year, &dir).unwrap();
        let wb: calamine::Xlsx<_> = calamine::open_workbook(&path).unwrap();
        assert_eq!(wb.sheet_names(), vec!["EXPEDIDAS".to_string(), "RECIBIDAS".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }
}

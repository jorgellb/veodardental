//! Almacén (productos, movimientos, consumos por tratamiento, pedidos a
//! proveedores) y laboratorio protésico (trabajos enviados y su seguimiento).

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Supplier {
    pub id: u32,
    pub name: String,
    pub nif: String,
    pub phone: String,
    pub email: String,
    pub contact: String,
    pub is_lab: bool,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub id: u32,
    pub code: String,
    pub name: String,
    pub family: String,
    pub unit: String,
    pub stock: f64,
    pub min_stock: f64,
    pub cost: f64,
    pub supplier_id: u32,
    pub location: String,
    pub active: bool,
    /// Producto con lote (implantes, pilares, injertos…): se apunta el lote al
    /// recibirlo y el que lleva cada paciente al colocárselo.
    #[serde(default)]
    pub traced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum MoveKind {
    Entrada,
    Salida,
    Consumo,
    Ajuste,
}

impl MoveKind {
    pub fn label(self) -> &'static str {
        match self {
            MoveKind::Entrada => "Entrada",
            MoveKind::Salida => "Salida",
            MoveKind::Consumo => "Consumo",
            MoveKind::Ajuste => "Ajuste inventario",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StockMove {
    pub id: u32,
    pub product_id: u32,
    pub when: NaiveDateTime,
    pub qty: f64,
    pub kind: MoveKind,
    pub reason: String,
    pub patient_id: Option<u32>,
    pub lot: String,
    pub user: String,
    /// Tratamiento que gastó el material (en los consumos automáticos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<u32>,
    /// Caducidad del lote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<NaiveDate>,
}

/// Lote de un producto con lo que queda de él (según sus movimientos).
#[derive(Clone, Debug, PartialEq)]
pub struct Lot {
    pub lot: String,
    pub expiry: Option<NaiveDate>,
    pub qty: f64,
    /// Cuándo entró por primera vez.
    pub since: NaiveDateTime,
}

/// Caducidad tal como viene en la caja: «03/2029» (hasta fin de mes),
/// «2029-03», «15/03/2029» o «2029-03-15».
pub fn parse_expiry(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for f in ["%d/%m/%Y", "%Y-%m-%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    let (m, y) = match (s.split_once('/'), s.split_once('-')) {
        (Some((m, y)), _) => (m, y),
        (_, Some((y, m))) => (m, y),
        _ => return None,
    };
    let (m, mut y): (u32, i32) = (m.trim().parse().ok()?, y.trim().parse().ok()?);
    if y < 100 {
        y += 2000;
    }
    if !(1..=12).contains(&m) {
        return None;
    }
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    next?.pred_opt()
}

/// Caducidad para leer: «03/2029» si es fin de mes (como en la caja), la fecha si no.
pub fn fmt_expiry(d: NaiveDate) -> String {
    if d.succ_opt().is_some_and(|n| n.day() == 1) { d.format("%m/%Y").to_string() } else { fmt_date(d) }
}

/// Material que gasta un tratamiento al realizarse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Consumption {
    pub code: String,
    pub product_id: u32,
    pub qty: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum OrderState {
    Borrador,
    Enviado,
    Recibido,
    Cancelado,
}

impl OrderState {
    pub fn label(self) -> &'static str {
        match self {
            OrderState::Borrador => "Borrador",
            OrderState::Enviado => "Enviado",
            OrderState::Recibido => "Recibido",
            OrderState::Cancelado => "Cancelado",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderLine {
    pub product_id: u32,
    pub qty: f64,
    pub cost: f64,
    /// Lote y caducidad que vienen en la caja (se escriben al recibir).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lot: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expiry: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: u32,
    pub supplier_id: u32,
    pub date: NaiveDate,
    pub state: OrderState,
    pub lines: Vec<OrderLine>,
    pub notes: String,
    pub received_on: Option<NaiveDate>,
}

impl PurchaseOrder {
    pub fn total(&self) -> f64 {
        round2(self.lines.iter().map(|l| l.qty * l.cost).sum())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum LabState {
    Pendiente,
    Enviado,
    EnPrueba,
    Recibido,
    Colocado,
    Repeticion,
}

impl LabState {
    pub const ALL: [LabState; 6] = [LabState::Pendiente, LabState::Enviado, LabState::EnPrueba, LabState::Recibido, LabState::Colocado, LabState::Repeticion];
    pub fn label(self) -> &'static str {
        match self {
            LabState::Pendiente => "Por enviar",
            LabState::Enviado => "En laboratorio",
            LabState::EnPrueba => "Prueba en clínica",
            LabState::Recibido => "Recibido",
            LabState::Colocado => "Colocado",
            LabState::Repeticion => "Repetición",
        }
    }
    pub fn open(self) -> bool {
        matches!(self, LabState::Pendiente | LabState::Enviado | LabState::EnPrueba | LabState::Repeticion)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabWork {
    pub id: u32,
    pub patient_id: u32,
    pub lab_id: u32,
    pub entry_id: Option<u32>,
    pub description: String,
    pub teeth: Vec<u8>,
    pub shade: String,
    pub material: String,
    pub sent: NaiveDate,
    pub due: NaiveDate,
    pub received: Option<NaiveDate>,
    pub state: LabState,
    pub cost: f64,
    pub notes: String,
    /// Imágenes de la galería del paciente que se adjuntan a la orden (escaneados, fotos, radiografías).
    #[serde(default)]
    pub attachments: Vec<u32>,
    /// En la orden van sólo las iniciales y el nº de historia, no el nombre completo.
    #[serde(default)]
    pub anonymize: bool,
    /// Cuándo y a quién se envió la orden digital.
    #[serde(default)]
    pub order_sent: Option<chrono::NaiveDateTime>,
    #[serde(default)]
    pub order_to: String,
}

impl LabWork {
    pub fn overdue(&self, today: NaiveDate) -> bool {
        self.state.open() && self.state != LabState::EnPrueba && self.due < today
    }
}

pub fn is_prosthetic(odg: OdgKind) -> bool {
    matches!(odg, OdgKind::Corona | OdgKind::Puente | OdgKind::ProtesisRemovible | OdgKind::Carilla)
}

impl Db {
    pub fn supplier_name(&self, id: u32) -> String {
        self.suppliers.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_else(|| "—".into())
    }
    pub fn product(&self, id: u32) -> Option<&Product> {
        self.products.iter().find(|p| p.id == id)
    }

    /// Registra un movimiento y actualiza el stock del producto. Devuelve el
    /// movimiento (`None` si el producto no existe).
    pub fn stock_move(&mut self, product_id: u32, qty: f64, kind: MoveKind, reason: &str, patient_id: Option<u32>, lot: &str) -> Option<u32> {
        let p = self.products.iter_mut().find(|p| p.id == product_id)?;
        p.stock = round2(p.stock + qty);
        let id = self.new_id();
        let user = self.config.current_user.clone();
        self.stock_moves.push(StockMove { id, product_id, when: now(), qty: round2(qty), kind, reason: reason.into(), patient_id, lot: lot.into(), user, entry_id: None, expiry: None });
        Some(id)
    }

    /// Descuenta del almacén el material de un tratamiento realizado.
    pub fn consume_for_entry(&mut self, entry_id: u32) {
        let Some(e) = self.entries.iter().find(|e| e.id == entry_id).cloned() else { return };
        let list: Vec<Consumption> = self.consumptions.iter().filter(|c| c.code == e.code).cloned().collect();
        for c in list {
            let qty = c.qty * e.units.max(1) as f64;
            let reason = format!("{} {}", e.description, e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-"));
            // lo que lleva lote sale del que antes caduca (y queda apuntado en el paciente)
            let traced = self.product(c.product_id).is_some_and(|p| p.traced);
            let parts = if traced { self.pick_lots(c.product_id, qty) } else { vec![(String::new(), None, qty)] };
            for (lot, expiry, q) in parts {
                let id = self.stock_move(c.product_id, -q, MoveKind::Consumo, &reason, Some(e.patient_id), &lot);
                if let Some(m) = id.and_then(|id| self.stock_moves.iter_mut().find(|m| m.id == id)) {
                    m.entry_id = Some(entry_id);
                    m.expiry = expiry;
                }
            }
        }
    }

    /// Lotes de un producto con lo que queda de cada uno, en el orden en que entraron.
    pub fn lots(&self, product_id: u32) -> Vec<Lot> {
        let mut moves: Vec<&StockMove> = self.stock_moves.iter().filter(|m| m.product_id == product_id && !m.lot.trim().is_empty()).collect();
        moves.sort_by_key(|m| (m.when, m.id));
        let mut out: Vec<Lot> = vec![];
        for m in moves {
            let name = m.lot.trim();
            match out.iter_mut().find(|l| l.lot == name) {
                Some(l) => {
                    l.qty += m.qty;
                    if l.expiry.is_none() {
                        l.expiry = m.expiry;
                    }
                }
                None => out.push(Lot { lot: name.to_string(), expiry: m.expiry, qty: m.qty, since: m.when }),
            }
        }
        for l in out.iter_mut() {
            l.qty = round2(l.qty);
        }
        out
    }

    /// De qué lotes sale `qty` de un producto: primero del que antes caduca (a
    /// igual caducidad, del que entró antes), nunca de uno caducado. Lo que no
    /// cubren los lotes sale sin lote (para apuntarlo a mano).
    pub fn pick_lots(&self, product_id: u32, qty: f64) -> Vec<(String, Option<NaiveDate>, f64)> {
        let now = today();
        let mut lots: Vec<Lot> = self.lots(product_id).into_iter().filter(|l| l.qty > 1e-9 && l.expiry.is_none_or(|e| e >= now)).collect();
        lots.sort_by_key(|l| (l.expiry.unwrap_or(NaiveDate::MAX), l.since));
        let mut left = qty;
        let mut out = vec![];
        for l in lots {
            if left <= 1e-9 {
                break;
            }
            let take = left.min(l.qty);
            out.push((l.lot, l.expiry, round2(take)));
            left -= take;
        }
        if left > 1e-9 {
            out.push((String::new(), None, round2(left)));
        }
        out
    }

    /// Movimiento hecho a mano en el almacén. En los productos con lote, la
    /// entrada lo pide y la salida sin lote sale de los que antes caducan.
    pub fn manual_move(&mut self, product_id: u32, qty: f64, kind: MoveKind, reason: &str, lot: &str, expiry: &str) -> Result<(), String> {
        let p = self.product(product_id).cloned().ok_or("El producto ya no existe.")?;
        let exp = match expiry.trim() {
            "" => None,
            t => Some(parse_expiry(t).ok_or_else(|| format!("No se entiende la caducidad «{t}»: escríbala como mes/año, p. ej. 03/2029."))?),
        };
        let lot = lot.trim();
        if p.traced && kind == MoveKind::Entrada && lot.is_empty() {
            return Err(format!("«{}» lleva lote: escriba el de la caja.", p.name));
        }
        let reason = if reason.trim().is_empty() { kind.label() } else { reason.trim() };
        let parts = if p.traced && qty < 0.0 && lot.is_empty() && kind == MoveKind::Salida { self.pick_lots(product_id, -qty).into_iter().map(|(l, e, q)| (l, e, -q)).collect() } else { vec![(lot.to_string(), exp, qty)] };
        for (l, e, q) in parts {
            let e = e.or_else(|| self.lots(product_id).into_iter().find(|x| x.lot == l).and_then(|x| x.expiry));
            if let Some(m) = self.stock_move(product_id, q, kind, reason, None, &l).and_then(|id| self.stock_moves.iter_mut().find(|m| m.id == id)) {
                m.expiry = if l.is_empty() { None } else { e };
            }
        }
        Ok(())
    }

    /// Productos con lote que lleva el paciente (implantes, pilares…): lo que
    /// gastaron sus tratamientos, sin los que se borraron.
    pub fn patient_lots(&self, pid: u32) -> Vec<&StockMove> {
        let mut v: Vec<&StockMove> = self
            .stock_moves
            .iter()
            .filter(|m| m.patient_id == Some(pid) && m.kind == MoveKind::Consumo && m.qty < 0.0)
            .filter(|m| self.product(m.product_id).is_some_and(|p| p.traced) || !m.lot.is_empty())
            .filter(|m| m.entry_id.is_none_or(|e| self.entries.iter().any(|x| x.id == e)))
            .collect();
        v.sort_by_key(|m| (m.when, m.id));
        v
    }

    /// Pacientes que llevan un lote (para avisarles si el fabricante lo retira).
    pub fn lot_patients(&self, lot: &str) -> Vec<&StockMove> {
        let q = fold(lot.trim());
        if q.is_empty() {
            return vec![];
        }
        let mut v: Vec<&StockMove> =
            self.stock_moves.iter().filter(|m| m.patient_id.is_some() && m.kind == MoveKind::Consumo && m.qty < 0.0 && fold(&m.lot).contains(&q)).filter(|m| m.entry_id.is_none_or(|e| self.entries.iter().any(|x| x.id == e))).collect();
        v.sort_by_key(|m| std::cmp::Reverse((m.when, m.id)));
        v
    }

    /// Corrige el lote de lo que se le puso al paciente `patient` (con la
    /// caducidad de ese lote). Devuelve si lo ha cambiado (no, si el
    /// movimiento no es de ese paciente).
    pub fn set_move_lot(&mut self, move_id: u32, patient: u32, lot: &str) -> bool {
        let Some(product) = self.stock_moves.iter().find(|m| m.id == move_id && m.patient_id == Some(patient)).map(|m| m.product_id) else { return false };
        let lot = lot.trim().to_string();
        let expiry = self.lots(product).into_iter().find(|l| l.lot == lot).and_then(|l| l.expiry);
        match self.stock_moves.iter_mut().find(|m| m.id == move_id) {
            Some(m) => {
                m.lot = lot;
                m.expiry = expiry;
                true
            }
            None => false,
        }
    }

    /// Lotes con existencias que caducan hasta `limit` (también los ya
    /// caducados), el que antes caduca primero.
    pub fn expiring_lots(&self, limit: NaiveDate) -> Vec<(u32, Lot)> {
        let mut v: Vec<(u32, Lot)> = self.products.iter().filter(|p| p.active).flat_map(|p| self.lots(p.id).into_iter().map(move |l| (p.id, l))).filter(|(_, l)| l.qty > 1e-9 && l.expiry.is_some_and(|e| e <= limit)).collect();
        v.sort_by_key(|(_, l)| l.expiry);
        v
    }

    /// Primera caducidad de un producto entre los lotes que le quedan.
    pub fn next_expiry(&self, product_id: u32) -> Option<NaiveDate> {
        self.lots(product_id).into_iter().filter(|l| l.qty > 1e-9).filter_map(|l| l.expiry).min()
    }

    /// Hoja para el paciente con los implantes y demás productos con lote que
    /// lleva (fabricante, referencia, lote y caducidad), para guardarla con la
    /// tarjeta de implante.
    pub fn implant_card_html(&self, pid: u32) -> String {
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let p = self.patient(pid).cloned().unwrap_or_default();
        let rows: String = self
            .patient_lots(pid)
            .iter()
            .map(|m| {
                let prod = self.product(m.product_id);
                let teeth = m.entry_id.and_then(|e| self.entries.iter().find(|x| x.id == e)).map(|e| e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-")).unwrap_or_default();
                format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><b>{}</b></td><td>{}</td><td class='n'>{}</td></tr>",
                    fmt_date(m.when.date()),
                    esc(&teeth),
                    esc(&prod.map(|p| p.name.clone()).unwrap_or_default()),
                    esc(&prod.map(|p| self.supplier_name(p.supplier_id)).unwrap_or_default()),
                    esc(&prod.map(|p| p.code.clone()).unwrap_or_default()),
                    esc(if m.lot.is_empty() { "sin apuntar" } else { &m.lot }),
                    m.expiry.map(fmt_expiry).unwrap_or_default(),
                    -m.qty
                )
            })
            .collect();
        format!(
            "<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Implantes</title><style>body{{font-family:'Open Sans',Arial,sans-serif;max-width:860px;margin:32px auto;color:#1e293b}}h1{{font-size:21px}}table{{width:100%;border-collapse:collapse;margin-top:12px}}th{{text-align:left;font-size:12px;color:#475569;background:#eef3f8;padding:6px}}td{{padding:6px;border-bottom:1px solid #e2e8f0;font-size:13px}}.n{{text-align:right}}</style></head><body><b>{clinic}</b><br><span style='color:#64748b'>{addr} · {phone}</span><h1>Implantes y productos sanitarios que lleva</h1><p>Paciente: <b>{name}</b> · Nº de historia {hist}</p><table><tr><th>Fecha</th><th>Pieza</th><th>Producto</th><th>Fabricante / proveedor</th><th>Referencia</th><th>Lote</th><th>Caducidad</th><th class='n'>Uds.</th></tr>{rows}</table><p style='margin-top:28px;font-size:13px;color:#475569'>Guarde esta hoja con su tarjeta de implante: si el fabricante revisa un lote, sabremos que es el suyo. Enséñela si le atienden en otra clínica.</p></body></html>",
            clinic = esc(&self.config.name),
            addr = esc(&self.config.address),
            phone = esc(&self.config.phone),
            name = esc(&p.full_name()),
            hist = p.history_no,
        )
    }

    /// Por qué no se puede borrar un tratamiento del historial (`Ok` si se puede):
    /// lo facturado se corrige con una rectificativa y lo liquidado a la mutua,
    /// anulando la liquidación.
    pub fn can_delete_entry(&self, id: u32) -> Result<(), String> {
        let e = self.entries.iter().find(|e| e.id == id).ok_or("El tratamiento ya no existe.")?;
        if let Some(inv) = e.invoice_id {
            let num = self.invoices.iter().find(|i| i.id == inv).map(|i| i.code()).unwrap_or_else(|| format!("#{inv}"));
            return Err(format!("Está en la factura {num}: para quitarlo, haga antes la factura rectificativa."));
        }
        if let Some(c) = self.claims.iter().find(|c| c.lines.iter().any(|l| l.entry_id == id)) {
            return Err(format!("Va en la liquidación {} a la mutua: para quitarlo, anule antes la liquidación.", c.number));
        }
        Ok(())
    }

    /// Borra un tratamiento del historial: si ya estaba hecho, el material que
    /// gastó vuelve al almacén, y su trabajo de laboratorio queda sin tratamiento.
    pub fn delete_entry(&mut self, id: u32) -> Result<(), String> {
        self.can_delete_entry(id)?;
        let e = self.entries.iter().find(|e| e.id == id).cloned().ok_or("El tratamiento ya no existe.")?;
        self.return_entry_material(&e, "tratamiento borrado");
        for w in self.lab_works.iter_mut().filter(|w| w.entry_id == Some(id)) {
            w.entry_id = None;
        }
        let what = format!("{} · {}", self.patient_name(e.patient_id), e.description);
        self.audit("Borrar tratamiento", &what);
        self.entries.retain(|x| x.id != id);
        Ok(())
    }

    /// Un tratamiento marcado como hecho por error vuelve a previsto: el
    /// material que gastó vuelve a su lote y sigue en su presupuesto (que se
    /// reabre si se había cerrado con él). Facturado o liquidado a la mutua, no.
    pub fn unrealize_entry(&mut self, id: u32) -> Result<(), String> {
        self.can_delete_entry(id)?;
        let e = self.entries.iter().find(|e| e.id == id).cloned().ok_or("El tratamiento ya no existe.")?;
        if e.state != TxState::Realizado {
            return Err("Solo vuelve a previsto un tratamiento hecho.".into());
        }
        self.return_entry_material(&e, "vuelve a previsto");
        if let Some(x) = self.entries.iter_mut().find(|x| x.id == id) {
            x.state = TxState::Previsto;
        }
        if let Some(bid) = e.budget_id
            && let Some(b) = self.budgets.iter_mut().find(|b| b.id == bid)
            && b.state == crate::model::BudgetState::Cerrado
        {
            b.state = crate::model::BudgetState::Aceptado;
        }
        let what = format!("{} · {} ({})", self.patient_name(e.patient_id), e.description, fmt_date(e.date));
        self.audit("Tratamiento vuelto a previsto", &what);
        Ok(())
    }

    /// Carpeta compartida: consumos de material que este puesto acaba de
    /// apuntar para un tratamiento que otro puesto ya gastó a la vez (en el
    /// fichero, `disk`): se quitan y su cantidad vuelve a las existencias.
    pub fn drop_double_consumption(&mut self, disk: &serde_json::Value) {
        let on_disk: std::collections::HashSet<u64> = disk["stock_moves"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_u64()).collect();
        let ours: Vec<StockMove> = self.stock_moves.iter().filter(|m| m.entry_id.is_some() && !on_disk.contains(&(m.id as u64))).cloned().collect();
        if !ours.iter().any(|m| m.qty < 0.0) {
            return;
        }
        let mut net: std::collections::HashMap<(u32, u32), f64> = std::collections::HashMap::new();
        for m in disk["stock_moves"].as_array().into_iter().flatten() {
            if let (Some(e), Some(p), Some(q)) = (m["entry_id"].as_u64(), m["product_id"].as_u64(), m["qty"].as_f64()) {
                *net.entry((e as u32, p as u32)).or_default() += q;
            }
        }
        for m in ours.iter().filter(|m| m.qty > 0.0) {
            *net.entry((m.entry_id.unwrap_or(0), m.product_id)).or_default() += m.qty;
        }
        let dropped: Vec<u32> = ours.iter().filter(|m| m.qty < 0.0 && net.get(&(m.entry_id.unwrap_or(0), m.product_id)).is_some_and(|n| *n < -1e-9)).map(|m| m.id).collect();
        if dropped.is_empty() {
            return;
        }
        self.stock_moves.retain(|x| !dropped.contains(&x.id));
        // las existencias solo cambian con los movimientos: las del fichero más
        // los movimientos nuevos de este puesto que quedan
        let products: std::collections::HashSet<u32> = ours.iter().filter(|m| dropped.contains(&m.id)).map(|m| m.product_id).collect();
        for pid in products {
            let Some(on_file) = disk["products"].as_array().into_iter().flatten().find(|p| p["id"].as_u64() == Some(pid as u64)).and_then(|p| p["stock"].as_f64()) else { continue };
            let mine: f64 = self.stock_moves.iter().filter(|m| m.product_id == pid && !on_disk.contains(&(m.id as u64))).map(|m| m.qty).sum();
            if let Some(p) = self.products.iter_mut().find(|p| p.id == pid) {
                p.stock = ((on_file + mine) * 1000.0).round() / 1000.0;
            }
        }
    }

    /// Lo que gastó un tratamiento vuelve a cada lote.
    fn return_entry_material(&mut self, e: &MedEntry, why: &str) {
        let id = e.id;
        let mut spent: Vec<(u32, String, Option<NaiveDate>, f64)> = vec![];
        for m in self.stock_moves.iter().filter(|m| m.entry_id == Some(id)) {
            match spent.iter_mut().find(|x| x.0 == m.product_id && x.1 == m.lot) {
                Some(x) => x.3 += m.qty,
                None => spent.push((m.product_id, m.lot.clone(), m.expiry, m.qty)),
            }
        }
        for (product, lot, expiry, qty) in spent {
            if qty.abs() > 1e-9 {
                let reason = format!("Devuelto: {} ({why})", e.description);
                if let Some(mid) = self.stock_move(product, -qty, MoveKind::Ajuste, &reason, Some(e.patient_id), &lot)
                    && let Some(m) = self.stock_moves.iter_mut().find(|m| m.id == mid)
                {
                    m.entry_id = Some(id);
                    m.expiry = expiry;
                }
            }
        }
    }

    pub fn low_stock(&self) -> Vec<&Product> {
        self.products.iter().filter(|p| p.active && p.stock <= p.min_stock).collect()
    }

    pub fn stock_value(&self) -> f64 {
        round2(self.products.iter().filter(|p| p.stock > 0.0).map(|p| p.stock * p.cost).sum())
    }

    /// Cantidad ya pedida y aún no recibida de un producto.
    pub fn on_order(&self, product_id: u32) -> f64 {
        self.orders.iter().filter(|o| matches!(o.state, OrderState::Borrador | OrderState::Enviado)).flat_map(|o| o.lines.iter()).filter(|l| l.product_id == product_id).map(|l| l.qty).sum()
    }

    /// Crea pedidos en borrador (uno por proveedor) con lo que está bajo
    /// mínimo, para dejar cada producto en el doble del mínimo.
    pub fn propose_orders(&mut self) -> Vec<u32> {
        let mut by_supplier: std::collections::BTreeMap<u32, Vec<OrderLine>> = Default::default();
        for p in self.products.iter().filter(|p| p.active && p.stock <= p.min_stock) {
            let pending = self.on_order(p.id);
            let qty = (p.min_stock * 2.0 - p.stock - pending).ceil();
            if qty > 0.0 {
                by_supplier.entry(p.supplier_id).or_default().push(OrderLine { product_id: p.id, qty, cost: p.cost, lot: String::new(), expiry: String::new() });
            }
        }
        let mut ids = vec![];
        for (sid, lines) in by_supplier {
            let id = self.new_id();
            self.orders.push(PurchaseOrder { id, supplier_id: sid, date: today(), state: OrderState::Borrador, lines, notes: String::new(), received_on: None });
            ids.push(id);
        }
        ids
    }

    /// Recibe un pedido: entra el material en el almacén y actualiza costes.
    pub fn receive_order(&mut self, id: u32) -> Result<(), String> {
        let o = self.orders.iter().find(|o| o.id == id).cloned().ok_or("Pedido no encontrado")?;
        if o.state == OrderState::Recibido {
            return Err("El pedido ya está recibido.".into());
        }
        if o.state == OrderState::Cancelado {
            return Err("El pedido está cancelado.".into());
        }
        // lo que lleva lote no entra sin él (es lo que se apunta luego en el paciente)
        for l in &o.lines {
            let Some(p) = self.product(l.product_id) else { continue };
            if p.traced && l.qty > 0.0 && l.lot.trim().is_empty() {
                return Err(format!("Falta el lote de «{}»: viene en la caja, junto a la caducidad.", p.name));
            }
            if !l.expiry.trim().is_empty() && parse_expiry(&l.expiry).is_none() {
                return Err(format!("No se entiende la caducidad de «{}» ({}): escríbala como mes/año, p. ej. 03/2029.", p.name, l.expiry.trim()));
            }
        }
        for l in &o.lines {
            let id = self.stock_move(l.product_id, l.qty, MoveKind::Entrada, &format!("Pedido #{} · {}", o.id, self.supplier_name(o.supplier_id)), None, l.lot.trim());
            if let Some(m) = id.and_then(|id| self.stock_moves.iter_mut().find(|m| m.id == id)) {
                m.expiry = parse_expiry(&l.expiry);
            }
            if let Some(p) = self.products.iter_mut().find(|p| p.id == l.product_id)
                && l.cost > 0.0
            {
                p.cost = l.cost;
            }
        }
        if let Some(x) = self.orders.iter_mut().find(|x| x.id == id) {
            x.state = OrderState::Recibido;
            x.received_on = Some(today());
        }
        self.expense_from_order(id);
        Ok(())
    }

    pub fn order_text(&self, id: u32) -> String {
        let Some(o) = self.orders.iter().find(|o| o.id == id) else { return String::new() };
        let mut s = format!("Pedido nº {} de {}\nFecha: {}\n\n", o.id, self.config.name, fmt_date(o.date));
        for l in &o.lines {
            if let Some(p) = self.product(l.product_id) {
                s.push_str(&format!("- {} x {} ({}) ref. {}\n", l.qty, p.name, p.unit, p.code));
            }
        }
        s.push_str(&format!("\nEntregar en: {}\nTeléfono: {}\nGracias.", self.config.address, self.config.phone));
        s
    }

    /// Tratamientos protésicos previstos del paciente que aún no tienen trabajo
    /// de laboratorio (los ya hechos, como los que vienen de otro programa, no).
    pub fn prosthetic_without_lab(&self, pid: u32) -> Vec<&MedEntry> {
        self.entries.iter().filter(|e| e.patient_id == pid && e.state == TxState::Previsto && is_prosthetic(e.odg)).filter(|e| !self.lab_works.iter().any(|w| w.entry_id == Some(e.id))).collect()
    }

    pub fn new_lab_work(&mut self, pid: u32, entry_id: Option<u32>, lab_id: u32, days: i64) -> u32 {
        let e = entry_id.and_then(|id| self.entries.iter().find(|e| e.id == id).cloned());
        let id = self.new_id();
        let t = today();
        self.lab_works.push(LabWork {
            id,
            patient_id: pid,
            lab_id,
            entry_id,
            description: e.as_ref().map(|e| e.description.clone()).unwrap_or_else(|| "Trabajo de prótesis".into()),
            teeth: e.as_ref().map(|e| e.teeth.clone()).unwrap_or_default(),
            shade: "A2".into(),
            material: String::new(),
            sent: t,
            due: t + Duration::days(days),
            received: None,
            state: LabState::Enviado,
            cost: 0.0,
            notes: String::new(),
            attachments: vec![],
            anonymize: false,
            order_sent: None,
            order_to: String::new(),
        });
        id
    }

    pub fn set_lab_state(&mut self, id: u32, st: LabState) {
        if let Some(w) = self.lab_works.iter_mut().find(|w| w.id == id) {
            w.state = st;
            if matches!(st, LabState::Recibido | LabState::Colocado | LabState::EnPrueba) && w.received.is_none() {
                w.received = Some(today());
            }
            if st == LabState::Repeticion {
                w.received = None;
                w.due = today() + Duration::days(5);
            }
        }
        if matches!(st, LabState::Recibido | LabState::Colocado) {
            self.sync_lab_expense(id);
        }
    }

    /// Trabajo de laboratorio abierto del paciente (aviso al dar cita).
    pub fn pending_lab_for(&self, pid: u32) -> Option<&LabWork> {
        self.lab_works.iter().filter(|w| w.patient_id == pid && matches!(w.state, LabState::Enviado | LabState::Repeticion | LabState::Pendiente)).min_by_key(|w| w.due)
    }
}

pub fn sample_stock(db: &mut Db, today: NaiveDate) {
    let s =
        |id: u32, name: &str, nif: &str, phone: &str, email: &str, contact: &str, lab: bool| Supplier { id, name: name.into(), nif: nif.into(), phone: phone.into(), email: email.into(), contact: contact.into(), is_lab: lab, notes: String::new() };
    *db.suppliers = vec![
        s(1, "Depósito Dental Ejemplo", "B11111111", "900 100 100", "pedidos@deposito-ejemplo.es", "Marta", false),
        s(2, "Implantes Ejemplo S.L.", "B22222222", "900 200 200", "comercial@implantes-ejemplo.es", "Jorge", false),
        s(3, "Laboratorio Dental Arte", "B33333333", "91 300 30 30", "trabajos@lab-arte.es", "Luis", true),
        s(4, "Laboratorio Orto Plus", "B44444444", "91 400 40 40", "orto@lab-ortoplus.es", "Ana", true),
    ];
    let p = |id: u32, code: &str, name: &str, family: &str, unit: &str, stock: f64, min: f64, cost: f64, sup: u32, loc: &str| Product {
        id,
        code: code.into(),
        name: name.into(),
        family: family.into(),
        unit: unit.into(),
        stock,
        min_stock: min,
        cost,
        supplier_id: sup,
        location: loc.into(),
        active: true,
        traced: family == "Implantología",
    };
    *db.products = vec![
        p(1, "GUA-M", "Guantes nitrilo talla M", "Desechables", "caja 100", 6.0, 4.0, 6.5, 1, "Almacén A1"),
        p(2, "MAS-Q", "Mascarillas quirúrgicas", "Desechables", "caja 50", 3.0, 4.0, 4.2, 1, "Almacén A1"),
        p(3, "ANE-ART", "Articaína 4% con epinefrina", "Anestesia", "carpule", 38.0, 50.0, 0.65, 1, "Gabinetes"),
        p(4, "COM-A2", "Composite universal A2", "Restauración", "jeringa 4 g", 5.0, 3.0, 24.0, 1, "Gabinetes"),
        p(5, "COM-A3", "Composite universal A3", "Restauración", "jeringa 4 g", 2.0, 3.0, 24.0, 1, "Gabinetes"),
        p(6, "ADH-U", "Adhesivo universal", "Restauración", "dosis", 45.0, 20.0, 0.9, 1, "Gabinetes"),
        p(7, "LIM-R", "Limas rotatorias (set)", "Endodoncia", "set", 7.0, 4.0, 18.0, 1, "Gabinete 1"),
        p(8, "GUT-P", "Puntas de gutapercha", "Endodoncia", "caja", 4.0, 2.0, 9.5, 1, "Gabinete 1"),
        p(9, "IMP-410", "Implante 4.1 x 10 mm", "Implantología", "unidad", 3.0, 4.0, 145.0, 2, "Armario quirófano"),
        p(10, "PIL-REC", "Pilar recto de titanio", "Implantología", "unidad", 5.0, 3.0, 62.0, 2, "Armario quirófano"),
        p(11, "SUT-4", "Sutura reabsorbible 4/0", "Cirugía", "sobre", 9.0, 10.0, 2.8, 1, "Armario quirófano"),
        p(12, "PRO-P", "Pasta de profilaxis", "Higiene", "dosis", 60.0, 30.0, 0.35, 1, "Higiene"),
        p(13, "ALG", "Alginato para impresiones", "Prótesis", "bolsa 450 g", 2.0, 2.0, 11.0, 1, "Almacén A2"),
        p(14, "FRE-D", "Fresas de diamante (surtido)", "Instrumental", "caja 10", 3.0, 2.0, 21.0, 1, "Almacén A2"),
    ];
    let c = |code: &str, pid: u32, qty: f64| Consumption { code: code.into(), product_id: pid, qty };
    db.consumptions = vec![
        c("0301", 4, 0.15),
        c("0301", 6, 1.0),
        c("0301", 3, 1.0),
        c("0302", 4, 0.2),
        c("0302", 6, 1.0),
        c("0302", 3, 1.0),
        c("0303", 4, 0.25),
        c("0303", 6, 1.0),
        c("0303", 3, 1.0),
        c("0401", 7, 0.25),
        c("0401", 8, 0.1),
        c("0401", 3, 1.0),
        c("0402", 7, 0.3),
        c("0402", 8, 0.15),
        c("0402", 3, 2.0),
        c("0403", 7, 0.5),
        c("0403", 8, 0.2),
        c("0403", 3, 2.0),
        c("0501", 3, 1.0),
        c("0502", 3, 2.0),
        c("0502", 11, 1.0),
        c("0701", 9, 1.0),
        c("0701", 11, 1.0),
        c("0701", 3, 2.0),
        c("0702", 10, 1.0),
        c("0201", 12, 1.0),
        c("0602", 12, 1.0),
        c("0804", 13, 0.5),
        c("0805", 13, 0.5),
    ];
    // histórico de movimientos (no cambia el stock inicial; en implantes y
    // pilares cuadra con él). Con su lote: el implante y el pilar del 46 que
    // lleva el paciente 101, y un lote de pilares que caduca pronto
    let entry = |db: &Db, code: &str| db.entries.iter().find(|e| e.patient_id == 101 && e.code == code && e.teeth == [46]).map(|e| e.id);
    let (imp46, crown46) = (entry(db, "0701"), entry(db, "0702"));
    type Hist<'a> = (u32, f64, MoveKind, &'a str, i64, &'a str, Option<i64>, Option<u32>);
    let hist: [Hist; 8] = [
        (3, 100.0, MoveKind::Entrada, "Pedido inicial", 40, "", None, None),
        (9, 2.0, MoveKind::Entrada, "Pedido inicial", 200, "IMP2311B", Some(400), None),
        (10, 6.0, MoveKind::Entrada, "Pedido inicial", 300, "PIL2210C", Some(40), None),
        (9, -1.0, MoveKind::Consumo, "Implante dental 46", 150, "IMP2311B", Some(400), imp46),
        (9, 2.0, MoveKind::Entrada, "Pedido · Implantes Ejemplo S.L.", 60, "IMP2405A", Some(700), None),
        (10, -1.0, MoveKind::Consumo, "Corona sobre implante 46", 60, "PIL2210C", Some(40), crown46),
        (4, -0.2, MoveKind::Consumo, "Obturación composite 2 caras 15", 5, "", None, None),
        (11, -1.0, MoveKind::Consumo, "Implante dental 46", 150, "", None, imp46),
    ];
    for (pid, qty, kind, reason, ago, lot, exp, entry_id) in hist {
        let id = db.new_id();
        let patient_id = entry_id.map(|_| 101);
        db.stock_moves.push(StockMove {
            id,
            product_id: pid,
            when: (today - Duration::days(ago)).and_hms_opt(10, 0, 0).unwrap(),
            qty,
            kind,
            reason: reason.into(),
            patient_id,
            lot: lot.into(),
            user: "Recepción".into(),
            entry_id,
            expiry: exp.map(|d| today + Duration::days(d)),
        });
    }
    let id = db.new_id();
    db.orders.push(PurchaseOrder {
        id,
        supplier_id: 2,
        date: today - Duration::days(3),
        state: OrderState::Enviado,
        lines: vec![OrderLine { product_id: 9, qty: 4.0, cost: 145.0, lot: String::new(), expiry: String::new() }],
        notes: "Urgente para cirugía".into(),
        received_on: None,
    });
    let id = db.new_id();
    db.orders.push(PurchaseOrder {
        id,
        supplier_id: 1,
        date: today - Duration::days(40),
        state: OrderState::Recibido,
        lines: vec![OrderLine { product_id: 3, qty: 100.0, cost: 0.65, lot: String::new(), expiry: String::new() }, OrderLine { product_id: 1, qty: 10.0, cost: 6.5, lot: String::new(), expiry: String::new() }],
        notes: String::new(),
        received_on: Some(today - Duration::days(37)),
    });

    // trabajos de laboratorio
    let lw = |db: &mut Db, pid: u32, code: &str, lab: u32, sent: i64, due: i64, st: LabState, cost: f64, shade: &str, mat: &str| {
        let entry = db.entries.iter().find(|e| e.patient_id == pid && e.code == code).map(|e| e.id);
        let id = db.new_lab_work(pid, entry, lab, 7);
        if let Some(w) = db.lab_works.iter_mut().find(|w| w.id == id) {
            w.sent = today - Duration::days(sent);
            w.due = today + Duration::days(due);
            w.state = st;
            w.cost = cost;
            w.shade = shade.into();
            w.material = mat.into();
            if matches!(st, LabState::Recibido | LabState::Colocado | LabState::EnPrueba) {
                w.received = Some(today - Duration::days(1));
            }
        }
    };
    lw(db, 107, "0802", 3, 6, 1, LabState::Enviado, 160.0, "A2", "Zirconio monolítico");
    lw(db, 100, "0802", 3, 20, -8, LabState::EnPrueba, 160.0, "A3", "Zirconio estratificado");
    lw(db, 105, "0805", 3, 14, -2, LabState::Enviado, 210.0, "A3", "Cromo-cobalto");
    lw(db, 101, "0702", 3, 75, -60, LabState::Colocado, 190.0, "A2", "Corona atornillada sobre implante");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    #[test]
    fn realizing_treatment_consumes_stock() {
        let mut db = Db::sample(d());
        let before = db.product(4).unwrap().stock;
        db.add_entry(110, "0302", vec![16], vec!["O".into(), "M".into()], TxState::Realizado, 1, d());
        let after = db.product(4).unwrap().stock;
        assert!((before - after - 0.2).abs() < 1e-6);
        assert!(db.stock_moves.iter().any(|m| m.kind == MoveKind::Consumo && m.patient_id == Some(110)));
    }

    #[test]
    fn proposal_and_reception() {
        let mut db = Db::sample(d());
        assert!(!db.low_stock().is_empty());
        let ids = db.propose_orders();
        assert!(!ids.is_empty());
        // no se vuelve a pedir lo ya pedido
        assert!(db.propose_orders().is_empty());
        let id = ids[0];
        let line = db.orders.iter().find(|o| o.id == id).unwrap().lines[0].clone();
        let before = db.product(line.product_id).unwrap().stock;
        db.receive_order(id).unwrap();
        assert!((db.product(line.product_id).unwrap().stock - before - line.qty).abs() < 1e-6);
        assert!(db.receive_order(id).is_err());
    }

    #[test]
    fn a_childs_treatment_is_invoiced_to_the_parent() {
        let mut db = Db::sample(d());
        // un niño sin NIF con una ortodoncia de 2.400 €
        let pid = 110;
        db.patient_mut(pid).unwrap().nif.clear();
        let e = db.add_entry(pid, "0302", vec![16], vec![], TxState::Realizado, 1, d());
        db.entries.iter_mut().find(|x| x.id == e).unwrap().price = 2400.0;
        assert!(db.issue_invoice(pid, &[e], d()).unwrap_err().contains("NIF"), "sin NIF y más de 400 €, no");
        db.patient_mut(pid).unwrap().payer = Payer { name: "Ana Pérez Gil".into(), nif: "12345678Z".into(), address: "C/ Mayor 1, 28001 Madrid".into(), relation: "Madre".into() };
        let inv = db.issue_invoice(pid, &[e], d()).unwrap();
        let f = db.invoice(inv).unwrap().clone();
        assert_eq!((f.client_name.as_str(), f.client_nif.as_str(), f.client_address.as_str(), f.kind()), ("Ana Pérez Gil", "12345678Z", "C/ Mayor 1, 28001 Madrid", "F1"));
        assert_eq!(f.patient_name, db.patient(pid).unwrap().full_name(), "y quién es el paciente");
        // la rectificativa va a la misma persona
        let r = db.rectify_invoice(inv, "Error").unwrap();
        assert_eq!(db.invoice(r).unwrap().client_nif, "12345678Z");
        // si tiene menos de 16 años, el consentimiento lo firma ella
        db.patient_mut(pid).unwrap().birth = NaiveDate::from_ymd_opt(2016, 5, 1);
        assert_eq!(db.representative(pid, d()).as_deref(), Some("Ana Pérez Gil (madre)"));
        db.patient_mut(pid).unwrap().birth = NaiveDate::from_ymd_opt(2009, 5, 1);
        assert_eq!(db.representative(pid, d()), None, "con 17 años firma el paciente");
        // sin dirección del responsable, la del paciente
        db.patient_mut(pid).unwrap().payer.address.clear();
        let inv2 = db.issue_invoice(pid, &[e], d()).unwrap();
        assert!(db.invoice(inv2).unwrap().client_address.contains(&db.patient(pid).unwrap().city));
    }

    #[test]
    fn an_invoiced_or_claimed_treatment_cannot_be_deleted() {
        let mut db = Db::sample(d());
        let e = db.add_entry(110, "0302", vec![16], vec!["O".into()], TxState::Realizado, 1, d());
        db.add_payment(110, db.entries.iter().find(|x| x.id == e).unwrap().amount(), "Tarjeta", "");
        let inv = db.issue_invoice(110, &[e], d()).unwrap();
        let before = db.patient_balance(110);
        let err = db.delete_entry(e).unwrap_err();
        assert!(err.contains("factura"), "{err}");
        assert!(db.entries.iter().any(|x| x.id == e), "sigue en el historial");
        assert!((db.patient_balance(110) - before).abs() < 0.005, "y el paciente no queda con saldo a favor");
        // con la factura rectificada, sí
        db.rectify_invoice(inv, "Error en el tratamiento").unwrap();
        db.delete_entry(e).unwrap();
        assert!(!db.entries.iter().any(|x| x.id == e));
        // un acto que va en una liquidación a la mutua, tampoco
        let from = d() - Duration::days(365);
        let line = db.claimable(2, from, d())[0].clone();
        db.create_claim(2, from, d(), d()).unwrap();
        let err = db.delete_entry(line.entry_id).unwrap_err();
        assert!(err.contains("LIQ-"), "{err}");
    }

    /// Marcar en el odontograma como hecho lo que ya estaba previsto (de un
    /// presupuesto) completa el previsto: no crea otro ni lo cobra dos veces.
    #[test]
    fn marking_a_planned_treatment_done_on_the_chart_completes_it() {
        let mut db = Db::sample(d());
        let planned = db.add_entry(110, "0302", vec![16], vec![], TxState::Previsto, 1, d());
        db.entries.iter_mut().find(|e| e.id == planned).unwrap().budget_id = Some(4242);
        let n = db.entries.len();
        let stock = db.product(4).unwrap().stock;
        let (id, reused) = db.chart_entry(110, "0302", vec![16], vec!["O".into(), "M".into()], TxState::Realizado, 1, d());
        assert!(reused);
        assert_eq!(id, planned);
        assert_eq!(db.entries.len(), n, "no se crea otro tratamiento");
        let e = db.entries.iter().find(|e| e.id == planned).unwrap();
        assert_eq!((e.state, e.budget_id, e.faces.clone()), (TxState::Realizado, Some(4242), vec!["O".to_string(), "M".to_string()]));
        assert!((db.product(4).unwrap().stock - (stock - 0.2)).abs() < 1e-6, "el material se gasta una vez");
        // sin previsto en esa pieza: uno nuevo
        let (id2, reused2) = db.chart_entry(110, "0302", vec![17], vec!["O".into()], TxState::Realizado, 1, d());
        assert!(!reused2 && id2 != planned);
        assert_eq!(db.entries.len(), n + 1);
    }

    /// El código de obturación según las caras solo cambia a otro que en el
    /// catálogo de la clínica también sea una obturación.
    #[test]
    fn the_filling_code_follows_the_faces_only_within_fillings() {
        let mut db = Db::sample(d());
        assert_eq!(db.filling_code("0301", 2), "0302");
        assert_eq!(db.filling_code("0302", 1), "0301");
        assert_eq!(db.filling_code("0301", 4), "0303");
        // catálogo importado donde el 0302 es otra cosa
        db.treatments.iter_mut().find(|t| t.code == "0302").unwrap().odg = OdgKind::Endodoncia;
        assert_eq!(db.filling_code("0301", 2), "0301", "no cambia a un tratamiento que no es una obturación");
        assert_eq!(db.filling_code("9999", 2), "9999");
    }

    /// Un tratamiento marcado como hecho por error vuelve a previsto: el
    /// material vuelve a su lote y sigue en su presupuesto; facturado, no.
    #[test]
    fn a_treatment_marked_done_by_mistake_goes_back_to_planned() {
        let mut db = Db::sample(d());
        let stock = db.product(4).unwrap().stock;
        let e = db.add_entry(110, "0302", vec![16], vec![], TxState::Previsto, 1, d());
        db.entries.iter_mut().find(|x| x.id == e).unwrap().budget_id = Some(4242);
        db.realize_entry(e, d(), 1);
        assert!((db.product(4).unwrap().stock - (stock - 0.2)).abs() < 1e-6);
        db.unrealize_entry(e).unwrap();
        let x = db.entries.iter().find(|x| x.id == e).unwrap();
        assert_eq!((x.state, x.budget_id), (TxState::Previsto, Some(4242)));
        assert!((db.product(4).unwrap().stock - stock).abs() < 1e-6, "el material vuelve");
        assert!(db.audit_log.iter().any(|a| a.action == "Tratamiento vuelto a previsto"));
        assert!(db.unrealize_entry(e).is_err(), "ya estaba previsto");
        // facturado no se deshace
        db.realize_entry(e, d(), 1);
        let pid = db.entries.iter().find(|x| x.id == e).unwrap().patient_id;
        db.issue_invoice(pid, &[e], d()).unwrap();
        assert!(db.unrealize_entry(e).unwrap_err().contains("factura"));
    }

    #[test]
    fn deleting_a_done_treatment_returns_its_material() {
        let mut db = Db::sample(d());
        let before = db.product(4).unwrap().stock;
        let e = db.add_entry(110, "0302", vec![16], vec!["O".into()], TxState::Realizado, 1, d());
        assert!((db.product(4).unwrap().stock - (before - 0.2)).abs() < 1e-6);
        let w = db.new_lab_work(110, Some(e), 3, 7);
        db.delete_entry(e).unwrap();
        assert!((db.product(4).unwrap().stock - before).abs() < 1e-6, "el composite vuelve al almacén");
        assert!(db.stock_moves.iter().any(|m| m.product_id == 4 && m.qty > 0.0 && m.patient_id == Some(110)), "con su movimiento");
        assert_eq!(db.lab_works.iter().find(|x| x.id == w).unwrap().entry_id, None, "el trabajo de laboratorio queda suelto");
        // uno previsto no había gastado nada
        let p = db.add_entry(110, "0302", vec![17], vec![], TxState::Previsto, 1, d());
        let n = db.stock_moves.len();
        db.delete_entry(p).unwrap();
        assert_eq!(db.stock_moves.len(), n);
    }

    #[test]
    fn lab_cost_written_after_receiving_goes_to_expenses() {
        let mut db = Db::sample(d());
        let w = db.new_lab_work(110, None, 3, 7);
        db.set_lab_state(w, LabState::Recibido);
        let exp = |db: &Db| db.expenses.iter().filter(|e| e.concept.ends_with(&format!("#L{w}"))).map(|e| (e.base, e.paid)).collect::<Vec<_>>();
        assert!(exp(&db).is_empty(), "sin coste aún");
        // llega la factura del laboratorio y se escribe el coste
        db.lab_works.iter_mut().find(|x| x.id == w).unwrap().cost = 160.0;
        db.sync_lab_expense(w);
        assert_eq!(exp(&db), vec![(160.0, false)]);
        db.lab_works.iter_mut().find(|x| x.id == w).unwrap().cost = 170.0;
        db.sync_lab_expense(w);
        assert_eq!(exp(&db), vec![(170.0, false)], "se corrige, sin duplicar");
        // ya pagado: no se toca
        db.expenses.iter_mut().find(|e| e.concept.ends_with(&format!("#L{w}"))).unwrap().paid = true;
        db.lab_works.iter_mut().find(|x| x.id == w).unwrap().cost = 180.0;
        db.sync_lab_expense(w);
        assert_eq!(exp(&db), vec![(170.0, true)]);
        // un trabajo aún en el laboratorio no es gasto todavía
        let w2 = db.new_lab_work(110, None, 3, 7);
        db.lab_works.iter_mut().find(|x| x.id == w2).unwrap().cost = 90.0;
        db.sync_lab_expense(w2);
        assert!(!db.expenses.iter().any(|e| e.concept.ends_with(&format!("#L{w2}"))));
    }

    #[test]
    fn old_crowns_already_done_are_not_pending_for_the_lab() {
        let mut db = Db::sample(d());
        let done = db.add_entry(110, "0802", vec![11], vec![], TxState::Realizado, 1, d() - Duration::days(900));
        let planned = db.add_entry(110, "0802", vec![21], vec![], TxState::Previsto, 1, d());
        let ids: Vec<u32> = db.prosthetic_without_lab(110).iter().map(|e| e.id).collect();
        assert!(ids.contains(&planned));
        assert!(!ids.contains(&done), "una corona ya colocada no está por enviar");
    }

    #[test]
    fn expiry_is_read_as_written_on_the_box() {
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        assert_eq!(parse_expiry("03/2029"), ymd(2029, 3, 31), "mes/año: hasta fin de mes");
        assert_eq!(parse_expiry(" 2/29 "), ymd(2029, 2, 28));
        assert_eq!(parse_expiry("2029-12"), ymd(2029, 12, 31));
        assert_eq!(parse_expiry("15/03/2029"), ymd(2029, 3, 15));
        assert_eq!(parse_expiry("2029-03-15"), ymd(2029, 3, 15));
        assert_eq!(parse_expiry("13/2029"), None);
        assert_eq!(parse_expiry("pronto"), None);
        assert_eq!(parse_expiry(""), None);
    }

    #[test]
    fn an_implant_lot_goes_from_the_order_to_the_patient() {
        let mut db = Db::sample(d());
        assert!(db.product(9).unwrap().traced, "los implantes llevan lote");
        // el pedido de implantes que está en camino no se recibe sin lote
        let oid = db.orders.iter().find(|o| o.state == OrderState::Enviado && o.lines.iter().any(|l| l.product_id == 9)).unwrap().id;
        let err = db.receive_order(oid).unwrap_err();
        assert!(err.contains("lote"), "{err}");
        {
            let l = &mut db.orders.iter_mut().find(|o| o.id == oid).unwrap().lines[0];
            l.lot = "L-NUEVO".into();
            l.expiry = "mañana".into();
        }
        assert!(db.receive_order(oid).unwrap_err().contains("caducidad"));
        db.orders.iter_mut().find(|o| o.id == oid).unwrap().lines[0].expiry = "06/2031".into();
        db.receive_order(oid).unwrap();
        let lot = |db: &Db, name: &str| db.lots(9).into_iter().find(|l| l.lot == name).unwrap();
        assert_eq!((lot(&db, "L-NUEVO").qty, lot(&db, "L-NUEVO").expiry), (4.0, NaiveDate::from_ymd_opt(2031, 6, 30)));
        let total: f64 = db.lots(9).iter().map(|l| l.qty).sum();
        assert!((total - db.product(9).unwrap().stock).abs() < 1e-6, "los lotes suman el stock");
        // al colocar un implante sale del lote que antes caduca
        let first = db.pick_lots(9, 1.0)[0].0.clone();
        db.add_entry(110, "0701", vec![36], vec![], TxState::Realizado, 2, d());
        let mine = db.patient_lots(110);
        assert_eq!(mine.len(), 1, "solo lo que lleva lote (no la sutura ni la anestesia)");
        assert_eq!((mine[0].product_id, mine[0].lot.as_str(), mine[0].qty), (9, first.as_str(), -1.0));
        // tres implantes a la vez: si un lote no llega, el resto del siguiente
        let plan: Vec<(String, f64)> = db.pick_lots(9, 3.0).into_iter().map(|(l, _, q)| (l, q)).collect();
        assert!(plan.len() >= 2, "en los datos de ejemplo no llega un solo lote: {plan:?}");
        let e = db.add_entry(111, "0701", vec![14, 15, 16], vec![], TxState::Realizado, 2, d());
        let got: Vec<(String, f64)> = db.patient_lots(111).iter().map(|m| (m.lot.clone(), -m.qty)).collect();
        assert_eq!(got, plan);
        // se busca por lote y sale el paciente
        let who: Vec<u32> = db.lot_patients(&first).iter().filter_map(|m| m.patient_id).collect();
        assert!(who.contains(&110));
        // el lote se puede corregir
        let mid = db.patient_lots(110)[0].id;
        assert!(!db.set_move_lot(mid, 111, "L-NUEVO"), "solo en la ficha de su paciente");
        assert!(db.set_move_lot(mid, 110, "L-NUEVO"));
        assert_eq!(db.patient_lots(110)[0].lot, "L-NUEVO");
        assert_eq!(db.patient_lots(110)[0].expiry, NaiveDate::from_ymd_opt(2031, 6, 30), "con la caducidad de ese lote");
        // borrar el tratamiento devuelve cada implante a su lote
        let before: Vec<Lot> = db.lots(9);
        db.delete_entry(e).unwrap();
        assert!(db.patient_lots(111).is_empty());
        for (lot_name, q) in got {
            let was = before.iter().find(|l| l.lot == lot_name).unwrap().qty;
            assert!((lot(&db, &lot_name).qty - (was + q)).abs() < 1e-6, "{lot_name}");
        }
        assert!(db.implant_card_html(110).contains("L-NUEVO"));
    }

    #[test]
    fn lots_about_to_expire_are_flagged_and_expired_ones_are_not_used() {
        let mut db = Db::sample(d());
        let soon = db.expiring_lots(today() + Duration::days(60));
        assert!(soon.iter().any(|(p, _)| *p == 10), "el lote de pilares de ejemplo caduca pronto");
        // un lote caducado no se coloca
        db.manual_move(10, 2.0, MoveKind::Entrada, "", "VIEJO", "01/2020").unwrap();
        assert!(!db.pick_lots(10, 10.0).iter().any(|(l, _, _)| l == "VIEJO"));
        // a mano: la entrada pide el lote y la salida sale del que antes caduca
        assert!(db.manual_move(9, 1.0, MoveKind::Entrada, "", "", "").unwrap_err().contains("lote"));
        let first = db.pick_lots(9, 1.0)[0].0.clone();
        db.manual_move(9, -1.0, MoveKind::Salida, "caído al suelo", "", "").unwrap();
        assert_eq!(db.stock_moves.last().unwrap().lot, first);
        // los productos sin lote, como siempre
        db.manual_move(1, 1.0, MoveKind::Entrada, "", "", "").unwrap();
    }

    #[test]
    fn lab_overdue_and_pending() {
        let db = Db::sample(d());
        assert!(db.lab_works.iter().any(|w| w.overdue(d())));
        assert!(db.pending_lab_for(107).is_some());
    }
}

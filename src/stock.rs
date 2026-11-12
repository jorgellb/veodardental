//! Almacén (productos, movimientos, consumos por tratamiento, pedidos a
//! proveedores) y laboratorio protésico (trabajos enviados y su seguimiento).

use crate::model::*;
use chrono::{Duration, NaiveDate, NaiveDateTime};
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

    /// Registra un movimiento y actualiza el stock del producto.
    pub fn stock_move(&mut self, product_id: u32, qty: f64, kind: MoveKind, reason: &str, patient_id: Option<u32>, lot: &str) {
        let Some(p) = self.products.iter_mut().find(|p| p.id == product_id) else { return };
        p.stock = round2(p.stock + qty);
        let id = self.new_id();
        let user = self.config.current_user.clone();
        self.stock_moves.push(StockMove { id, product_id, when: now(), qty: round2(qty), kind, reason: reason.into(), patient_id, lot: lot.into(), user });
    }

    /// Descuenta del almacén el material de un tratamiento realizado.
    pub fn consume_for_entry(&mut self, entry_id: u32) {
        let Some(e) = self.entries.iter().find(|e| e.id == entry_id).cloned() else { return };
        let list: Vec<Consumption> = self.consumptions.iter().filter(|c| c.code == e.code).cloned().collect();
        for c in list {
            let qty = c.qty * e.units.max(1) as f64;
            self.stock_move(c.product_id, -qty, MoveKind::Consumo, &format!("{} {}", e.description, e.teeth.iter().map(|t| t.to_string()).collect::<Vec<_>>().join("-")), Some(e.patient_id), "");
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
        self.orders
            .iter()
            .filter(|o| matches!(o.state, OrderState::Borrador | OrderState::Enviado))
            .flat_map(|o| o.lines.iter())
            .filter(|l| l.product_id == product_id)
            .map(|l| l.qty)
            .sum()
    }

    /// Crea pedidos en borrador (uno por proveedor) con lo que está bajo
    /// mínimo, para dejar cada producto en el doble del mínimo.
    pub fn propose_orders(&mut self) -> Vec<u32> {
        let mut by_supplier: std::collections::BTreeMap<u32, Vec<OrderLine>> = Default::default();
        for p in self.products.iter().filter(|p| p.active && p.stock <= p.min_stock) {
            let pending = self.on_order(p.id);
            let qty = (p.min_stock * 2.0 - p.stock - pending).ceil();
            if qty > 0.0 {
                by_supplier.entry(p.supplier_id).or_default().push(OrderLine { product_id: p.id, qty, cost: p.cost });
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
        for l in &o.lines {
            self.stock_move(l.product_id, l.qty, MoveKind::Entrada, &format!("Pedido #{} · {}", o.id, self.supplier_name(o.supplier_id)), None, "");
            if let Some(p) = self.products.iter_mut().find(|p| p.id == l.product_id) && l.cost > 0.0 {
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

    /// Tratamientos protésicos del paciente que aún no tienen trabajo de laboratorio.
    pub fn prosthetic_without_lab(&self, pid: u32) -> Vec<&MedEntry> {
        self.entries
            .iter()
            .filter(|e| e.patient_id == pid && e.state != TxState::Previo && is_prosthetic(e.odg))
            .filter(|e| !self.lab_works.iter().any(|w| w.entry_id == Some(e.id)))
            .collect()
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
            self.expense_from_lab(id);
        }
    }

    /// Trabajo de laboratorio abierto del paciente (aviso al dar cita).
    pub fn pending_lab_for(&self, pid: u32) -> Option<&LabWork> {
        self.lab_works.iter().filter(|w| w.patient_id == pid && matches!(w.state, LabState::Enviado | LabState::Repeticion | LabState::Pendiente)).min_by_key(|w| w.due)
    }
}

pub fn sample_stock(db: &mut Db, today: NaiveDate) {
    let s = |id: u32, name: &str, nif: &str, phone: &str, email: &str, contact: &str, lab: bool| Supplier { id, name: name.into(), nif: nif.into(), phone: phone.into(), email: email.into(), contact: contact.into(), is_lab: lab, notes: String::new() };
    *db.suppliers = vec![
        s(1, "Depósito Dental Ejemplo", "B11111111", "900 100 100", "pedidos@deposito-ejemplo.es", "Marta", false),
        s(2, "Implantes Ejemplo S.L.", "B22222222", "900 200 200", "comercial@implantes-ejemplo.es", "Jorge", false),
        s(3, "Laboratorio Dental Arte", "B33333333", "91 300 30 30", "trabajos@lab-arte.es", "Luis", true),
        s(4, "Laboratorio Orto Plus", "B44444444", "91 400 40 40", "orto@lab-ortoplus.es", "Ana", true),
    ];
    let p = |id: u32, code: &str, name: &str, family: &str, unit: &str, stock: f64, min: f64, cost: f64, sup: u32, loc: &str| Product { id, code: code.into(), name: name.into(), family: family.into(), unit: unit.into(), stock, min_stock: min, cost, supplier_id: sup, location: loc.into(), active: true };
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
        c("0301", 4, 0.15), c("0301", 6, 1.0), c("0301", 3, 1.0),
        c("0302", 4, 0.2), c("0302", 6, 1.0), c("0302", 3, 1.0),
        c("0303", 4, 0.25), c("0303", 6, 1.0), c("0303", 3, 1.0),
        c("0401", 7, 0.25), c("0401", 8, 0.1), c("0401", 3, 1.0),
        c("0402", 7, 0.3), c("0402", 8, 0.15), c("0402", 3, 2.0),
        c("0403", 7, 0.5), c("0403", 8, 0.2), c("0403", 3, 2.0),
        c("0501", 3, 1.0), c("0502", 3, 2.0), c("0502", 11, 1.0),
        c("0701", 9, 1.0), c("0701", 11, 1.0), c("0701", 3, 2.0), c("0702", 10, 1.0),
        c("0201", 12, 1.0), c("0602", 12, 1.0),
        c("0804", 13, 0.5), c("0805", 13, 0.5),
    ];
    // histórico de movimientos (sin alterar el stock inicial)
    let hist = [(3, 100.0, MoveKind::Entrada, "Pedido inicial", 40), (9, 6.0, MoveKind::Entrada, "Pedido inicial", 60), (9, -1.0, MoveKind::Consumo, "Implante dental 46", 150), (9, -1.0, MoveKind::Consumo, "Implante dental 36", 15), (4, -0.2, MoveKind::Consumo, "Obturación composite 2 caras 15", 5)];
    for (pid, qty, kind, reason, ago) in hist {
        let id = db.new_id();
        db.stock_moves.push(StockMove { id, product_id: pid, when: (today - Duration::days(ago)).and_hms_opt(10, 0, 0).unwrap(), qty, kind, reason: reason.into(), patient_id: None, lot: String::new(), user: "Recepción".into() });
    }
    let id = db.new_id();
    db.orders.push(PurchaseOrder { id, supplier_id: 2, date: today - Duration::days(3), state: OrderState::Enviado, lines: vec![OrderLine { product_id: 9, qty: 4.0, cost: 145.0 }], notes: "Urgente para cirugía".into(), received_on: None });
    let id = db.new_id();
    db.orders.push(PurchaseOrder { id, supplier_id: 1, date: today - Duration::days(40), state: OrderState::Recibido, lines: vec![OrderLine { product_id: 3, qty: 100.0, cost: 0.65 }, OrderLine { product_id: 1, qty: 10.0, cost: 6.5 }], notes: String::new(), received_on: Some(today - Duration::days(37)) });

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
    fn lab_overdue_and_pending() {
        let db = Db::sample(d());
        assert!(db.lab_works.iter().any(|w| w.overdue(d())));
        assert!(db.pending_lab_for(107).is_some());
    }
}

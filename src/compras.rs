//! Pedidos a proveedores por stock mínimo (T-F6-05).
//!
//! - **Propuesta:** con lo que está bajo mínimo, para dejarlo en el doble del
//!   mínimo, sin contar lo que ya está pedido y en cajas enteras (unidades
//!   de compra, `Product::pack`). La hace el servidor
//!   (`ChainOp::ProposeOrders`): dos puestos no proponen lo mismo.
//! - **Aprobación configurable** (`ClinicConfig::order_approval_from`, que
//!   solo cambia Dirección): por encima de ese total, el pedido lo aprueba
//!   quien lleva los gastos. Cuentan juntos los pedidos al mismo proveedor que
//!   no pasaron por aprobación en los 30 días anteriores.
//! - **Envío:** a mano (correo o teléfono) o, solo si la clínica lo activa y
//!   el proveedor lo tiene marcado, por correo sin intervención.
//! - **Recepción:** de una vez o en partes, con el lote y la caducidad de
//!   cada parte (varios lotes por línea). Nunca más de lo que falta: dos
//!   puestos no reciben dos veces lo mismo. Lo que falta sigue «en pedido».
//!   Se puede cerrar con lo recibido.
//! - **Gasto:** al cerrarse, por lo recibido.
//!
//! Cada paso es una operación del servidor (`ChainOp::*Order*`): desde un
//! puesto solo se edita un borrador.

use crate::model::*;
use crate::stock::{MoveKind, OrderState, Receipt, parse_expiry};
use chrono::NaiveDateTime;

impl Db {
    fn order(&self, id: u32) -> Result<crate::stock::PurchaseOrder, String> {
        self.orders.iter().find(|o| o.id == id).cloned().ok_or_else(|| "El pedido ya no existe.".into())
    }

    /// ¿Necesita aprobación este total?
    pub fn order_needs_approval(&self, total: f64) -> bool {
        self.config.order_approval_from.is_some_and(|from| total + 0.005 >= from)
    }

    /// Lo que cuenta para la aprobación: el pedido y los que se confirmaron al
    /// mismo proveedor sin pasar por aprobación en los 30 días anteriores (así
    /// no se evita partiendo un pedido en varios pequeños).
    fn approval_total(&self, id: u32, now: NaiveDateTime) -> f64 {
        let Some(o) = self.orders.iter().find(|o| o.id == id) else { return 0.0 };
        // el mismo proveedor: la misma ficha, o el mismo NIF o correo en otra
        let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric() || *c == '@' || *c == '.').collect::<String>().to_lowercase();
        let key = |sid: u32| self.suppliers.iter().find(|s| s.id == sid).map(|s| (norm(&s.nif), norm(&s.email))).unwrap_or_default();
        let (nif, email) = key(o.supplier_id);
        let same = |sid: u32| {
            let (n, e) = key(sid);
            sid == o.supplier_id || (!nif.is_empty() && n == nif) || (!email.is_empty() && e == email)
        };
        let recent: f64 = self
            .orders
            .iter()
            .filter(|x| x.id != id && same(x.supplier_id) && x.state != OrderState::Cancelado)
            .filter(|x| x.approved.as_ref().is_some_and(|(t, how)| how.starts_with("sin aprobación") && (now - *t).num_days() < 30))
            .map(|x| x.total())
            .sum();
        o.total() + recent
    }

    /// Confirmar el borrador: a aprobar o, por debajo del importe, aprobado.
    pub fn order_submit(&mut self, id: u32, now: NaiveDateTime) -> Result<OrderState, String> {
        let o = self.order(id)?;
        if o.state != OrderState::Borrador {
            return Err("Solo se confirma un borrador.".into());
        }
        if o.lines.is_empty() || o.lines.iter().any(|l| !(l.qty > 0.0 && l.qty.is_finite()) || !(l.cost >= 0.0 && l.cost.is_finite())) {
            return Err("Revise las líneas: cantidades mayores que cero y precios no negativos.".into());
        }
        let needs = self.order_needs_approval(self.approval_total(id, now));
        let user = self.config.current_user.clone();
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        if needs {
            x.state = OrderState::PorAprobar;
        } else {
            x.state = OrderState::Aprobado;
            x.approved = Some((now, format!("sin aprobación (por debajo del importe) · {user}")));
        }
        let state = x.state;
        self.audit("Pedido confirmado", &format!("#{id} · {}", state.label()));
        Ok(state)
    }

    /// Aprobarlo (quien lleva los gastos).
    pub fn order_approve(&mut self, id: u32, now: NaiveDateTime) -> Result<(), String> {
        let user = self.config.current_user.clone();
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        if x.state != OrderState::PorAprobar {
            return Err("Este pedido no está por aprobar.".into());
        }
        x.state = OrderState::Aprobado;
        x.approved = Some((now, user));
        let total = x.total();
        self.audit("Pedido aprobado", &format!("#{id} · {}", money(total)));
        Ok(())
    }

    /// Enviado al proveedor (a mano o por correo automático).
    pub fn order_sent(&mut self, id: u32, channel: &str, now: NaiveDateTime) -> Result<(), String> {
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        if x.state != OrderState::Aprobado {
            return Err(if x.state == OrderState::PorAprobar { "Falta aprobarlo.".into() } else { "Este pedido no está aprobado para enviar.".into() });
        }
        x.state = OrderState::Enviado;
        x.sent = Some((now, channel.trim().chars().take(40).collect()));
        Ok(())
    }

    /// Recibe lo que ha llegado: (línea, cantidad en unidades del almacén,
    /// lote, caducidad). Una línea puede venir en varios lotes. Nunca más de
    /// lo que falta. Devuelve el estado en que queda.
    pub fn order_receive(&mut self, id: u32, got: &[(usize, f64, String, String)], now: NaiveDateTime) -> Result<OrderState, String> {
        let o = self.order(id)?;
        if !matches!(o.state, OrderState::Aprobado | OrderState::Enviado | OrderState::Parcial) {
            return Err(match o.state {
                OrderState::Recibido => "El pedido ya está recibido.".into(),
                OrderState::Cancelado => "El pedido está cancelado.".into(),
                OrderState::PorAprobar => "Falta aprobarlo.".into(),
                _ => "Primero confirme el pedido.".into(),
            });
        }
        if got.is_empty() {
            return Err("No ha llegado nada.".into());
        }
        let mut per_line = vec![0.0; o.lines.len()];
        for (i, qty, lot, expiry) in got {
            let l = o.lines.get(*i).ok_or("Una línea no es de este pedido.")?;
            if !(*qty > 0.0 && qty.is_finite()) {
                return Err("Las cantidades que llegan tienen que ser mayores que cero.".into());
            }
            per_line[*i] += qty;
            let Some(p) = self.product(l.product_id) else { continue };
            // lo que lleva lote no entra sin él (es lo que se apunta luego en el paciente)
            if p.traced && lot.trim().is_empty() {
                return Err(format!("Falta el lote de «{}»: viene en la caja, junto a la caducidad.", p.name));
            }
            if !expiry.trim().is_empty() && parse_expiry(expiry).is_none() {
                return Err(format!("No se entiende la caducidad de «{}» ({}): escríbala como mes/año, p. ej. 03/2029.", p.name, expiry.trim()));
            }
        }
        for (i, l) in o.lines.iter().enumerate() {
            if per_line[i] > l.qty - l.received + 1e-9 {
                let name = self.product(l.product_id).map(|p| p.name.clone()).unwrap_or_default();
                return Err(format!("De «{name}» faltan {} y llegan {}: no se recibe más de lo pedido (puede que otro puesto ya lo haya recibido).", l.qty - l.received, per_line[i]));
            }
        }
        let supplier = self.supplier_name(o.supplier_id);
        for (i, qty, lot, expiry) in got {
            let l = &o.lines[*i];
            let mid = self.stock_move(l.product_id, *qty, MoveKind::Entrada, &format!("Pedido #{} · {supplier}", o.id), None, lot.trim());
            if let Some(m) = mid.and_then(|m| self.stock_moves.iter_mut().find(|x| x.id == m)) {
                m.expiry = parse_expiry(expiry);
            }
            if let Some(p) = self.products.iter_mut().find(|p| p.id == l.product_id)
                && l.cost > 0.0
            {
                p.cost = l.cost;
            }
        }
        let by = self.config.current_user.clone();
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        for (i, add) in per_line.iter().enumerate() {
            x.lines[i].received += add;
        }
        x.receipts.push(Receipt { at: now, by, lines: got.iter().map(|(i, q, l, e)| (*i, *q, l.trim().to_string(), e.trim().to_string())).collect() });
        let done = x.lines.iter().all(|l| l.received + 1e-9 >= l.qty);
        x.state = if done { OrderState::Recibido } else { OrderState::Parcial };
        if done {
            x.received_on = Some(now.date());
        }
        let state = x.state;
        if done {
            self.expense_from_order(id);
        }
        Ok(state)
    }

    /// Cerrar con lo recibido: lo que falta ya no se espera.
    pub fn order_close(&mut self, id: u32, now: NaiveDateTime) -> Result<(), String> {
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        if x.state != OrderState::Parcial {
            return Err("Solo se cierra un pedido recibido en parte.".into());
        }
        x.state = OrderState::Recibido;
        x.received_on = Some(now.date());
        self.expense_from_order(id);
        self.audit("Pedido cerrado con lo recibido", &format!("#{id}"));
        Ok(())
    }

    /// Cancelarlo, si aún no ha llegado nada.
    pub fn order_cancel(&mut self, id: u32) -> Result<(), String> {
        let x = self.orders.iter_mut().find(|x| x.id == id).ok_or("El pedido ya no existe.")?;
        if !matches!(x.state, OrderState::Borrador | OrderState::PorAprobar | OrderState::Aprobado | OrderState::Enviado) {
            return Err("Ya ha llegado algo: ciérrelo con lo recibido.".into());
        }
        x.state = OrderState::Cancelado;
        self.audit("Pedido cancelado", &format!("#{id}"));
        Ok(())
    }

    /// ¿Se manda solo por correo? (la clínica lo tiene activado, el
    /// proveedor marcado, con correo, y el correo de la clínica configurado)
    pub fn order_auto_send(&self, id: u32) -> bool {
        let Some(o) = self.orders.iter().find(|o| o.id == id) else { return false };
        let Some(s) = self.suppliers.iter().find(|s| s.id == o.supplier_id) else { return false };
        self.config.orders_auto_send && s.auto_send && s.email.contains('@') && self.mail.ready() && o.state == OrderState::Aprobado
    }

    /// El correo del pedido: (para, asunto, texto).
    pub fn order_email(&self, id: u32) -> Option<(String, String, String)> {
        let o = self.orders.iter().find(|o| o.id == id)?;
        let s = self.suppliers.iter().find(|s| s.id == o.supplier_id)?;
        Some((s.email.trim().to_string(), format!("Pedido {} - {}", o.id, self.config.name), self.order_text(id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stock::OrderLine;

    fn at(d: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    /// Un proveedor ficticio y un producto que se compra en cajas de 100.
    fn shop() -> (Db, u32, u32) {
        let mut db = Db::sample(at(2).date());
        db.orders.clear();
        let sid = db.new_id();
        db.suppliers.push(crate::stock::Supplier {
            id: sid,
            name: "Proveedor Ficticio".into(),
            nif: String::new(),
            phone: String::new(),
            email: "pedidos@proveedor-ficticio.example".into(),
            contact: String::new(),
            is_lab: false,
            notes: String::new(),
            auto_send: false,
        });
        let pid = db.new_id();
        db.products.push(crate::stock::Product {
            id: pid,
            code: "GUA-S".into(),
            name: "Guantes talla S".into(),
            family: "Desechables".into(),
            unit: "unidad".into(),
            stock: 30.0,
            min_stock: 100.0,
            cost: 0.07,
            supplier_id: sid,
            location: String::new(),
            active: true,
            traced: false,
            pack: 100.0,
            pack_name: "caja de 100".into(),
        });
        (db, sid, pid)
    }

    #[test]
    fn the_proposal_is_in_whole_packs_and_counts_what_is_already_ordered() {
        let (mut db, sid, pid) = shop();
        let ids = db.propose_orders();
        let o = db.orders.iter().find(|o| o.supplier_id == sid && ids.contains(&o.id)).unwrap().clone();
        let l = o.lines.iter().find(|l| l.product_id == pid).unwrap();
        assert_eq!(l.qty, 200.0, "faltan 170 para el doble del mínimo: dos cajas de 100");
        assert!(db.order_text(o.id).contains("2 x Guantes talla S (caja de 100)"), "{}", db.order_text(o.id));
        // lo ya pedido cuenta: no se vuelve a proponer
        assert!(db.propose_orders().iter().all(|id| !db.orders.iter().find(|x| x.id == *id).unwrap().lines.iter().any(|l| l.product_id == pid)));
        assert_eq!(db.on_order(pid), 200.0);
    }

    #[test]
    fn approval_depends_on_the_amount_and_sending_needs_it() {
        let (mut db, sid, pid) = shop();
        db.config.order_approval_from = Some(100.0);
        let mk = |db: &mut Db, qty: f64, cost: f64| {
            let id = db.new_id();
            db.orders.push(crate::stock::PurchaseOrder { id, supplier_id: sid, date: at(2).date(), lines: vec![OrderLine { product_id: pid, qty, cost, ..Default::default() }], ..Default::default() });
            id
        };
        let small = mk(&mut db, 100.0, 0.07);
        let big = mk(&mut db, 2000.0, 0.07);
        assert_eq!(db.order_submit(small, at(2)).unwrap(), OrderState::Aprobado, "por debajo de 100 €, sin aprobación");
        assert!(db.orders.iter().find(|o| o.id == small).unwrap().approved.as_ref().unwrap().1.contains("sin aprobación"));
        assert_eq!(db.order_submit(big, at(2)).unwrap(), OrderState::PorAprobar);
        assert!(db.order_sent(big, "correo", at(2)).unwrap_err().contains("aprobarlo"));
        assert!(db.order_receive(big, &[(0, 1.0, String::new(), String::new())], at(3)).is_err(), "sin aprobar no se recibe");
        db.order_approve(big, at(3)).unwrap();
        assert!(db.order_approve(big, at(3)).is_err(), "una vez");
        db.order_sent(big, "correo", at(3)).unwrap();
        assert!(db.orders.iter().find(|o| o.id == big).unwrap().approved.as_ref().is_some_and(|a| a.1 == db.config.current_user));
        let bad = mk(&mut db, 0.0, 1.0);
        assert!(db.order_submit(bad, at(2)).is_err(), "sin cantidades, no");
        // partido en pedidos pequeños al mismo proveedor: al pasar del importe, a aprobar
        let mut states = vec![];
        for _ in 0..3 {
            let piece = mk(&mut db, 500.0, 0.07);
            states.push(db.order_submit(piece, at(4)).unwrap());
        }
        assert_eq!(states, vec![OrderState::Aprobado, OrderState::Aprobado, OrderState::PorAprobar], "7 + 35 + 35 = 77 €; con el tercero, 112 €: pasa de 100 €");
        // y en otra ficha del mismo proveedor (mismo correo), también cuentan juntos
        let twin = db.new_id();
        let mut s2 = db.suppliers.iter().find(|s| s.id == sid).unwrap().clone();
        (s2.id, s2.name, s2.email) = (twin, "Proveedor Ficticio (2)".into(), " Pedidos@Proveedor-Ficticio.example".into());
        db.suppliers.push(s2);
        let id = db.new_id();
        db.orders.push(crate::stock::PurchaseOrder { id, supplier_id: twin, date: at(4).date(), lines: vec![OrderLine { product_id: pid, qty: 400.0, cost: 0.07, ..Default::default() }], ..Default::default() });
        assert_eq!(db.order_submit(id, at(4)).unwrap(), OrderState::PorAprobar, "28 € solo no llega a 100, pero con los 77 € del mismo proveedor (otra ficha, mismo correo) sí");
        assert!(db.audit_log.iter().any(|a| a.action == "Pedido aprobado"));
    }

    /// En partes y con varios lotes; nunca más de lo que falta; lo que falta
    /// sigue pedido; se cierra con lo recibido y el gasto es por lo recibido.
    #[test]
    fn receiving_in_parts_with_lots_never_exceeds_the_order() {
        let (mut db, sid, pid) = shop();
        let id = db.new_id();
        db.orders.push(crate::stock::PurchaseOrder { id, supplier_id: sid, date: at(2).date(), lines: vec![OrderLine { product_id: pid, qty: 300.0, cost: 0.07, ..Default::default() }], ..Default::default() });
        db.order_submit(id, at(2)).unwrap();
        db.order_sent(id, "teléfono", at(2)).unwrap();
        let before = db.product(pid).unwrap().stock;
        let lot = |q: f64, l: &str| (0usize, q, l.to_string(), "12/2028".to_string());
        assert!(db.order_receive(id, &[lot(250.0, "A"), lot(100.0, "B")], at(4)).unwrap_err().contains("no se recibe más"), "350 de 300, no");
        assert_eq!(db.order_receive(id, &[lot(100.0, "A"), lot(100.0, "B")], at(4)).unwrap(), OrderState::Parcial);
        assert_eq!(db.on_order(pid), 100.0, "lo que falta sigue pedido");
        assert!((db.product(pid).unwrap().stock - (before + 200.0)).abs() < 1e-9);
        assert!(db.order_receive(id, &[lot(150.0, "C")], at(5)).is_err(), "faltan 100");
        assert!(db.order_receive(id, &[(0, 50.0, String::new(), "mañana".into())], at(5)).unwrap_err().contains("caducidad"));
        let expenses = db.expenses.len();
        db.order_close(id, at(6)).unwrap();
        assert_eq!(db.orders.iter().find(|o| o.id == id).unwrap().state, OrderState::Recibido);
        assert_eq!(db.on_order(pid), 0.0, "cerrado: ya no se espera");
        let e = db.expenses.iter().skip(expenses).find(|e| e.concept == format!("Pedido #{id}")).unwrap();
        assert!((e.base - 14.0).abs() < 1e-9, "el gasto, por lo recibido (200 × 0,07): {}", e.base);
        let lots: Vec<String> = db.lots(pid).into_iter().map(|l| l.lot).filter(|l| !l.is_empty()).collect();
        assert!(lots.contains(&"A".into()) && lots.contains(&"B".into()), "con sus lotes: {lots:?}");
        assert_eq!(db.orders.iter().find(|o| o.id == id).unwrap().receipts.len(), 1);
        assert!(db.order_cancel(id).is_err(), "con algo recibido no se cancela");
    }

    /// Correo automático, solo si la clínica y el proveedor lo tienen
    /// activado; probado con un proveedor ficticio (servidor de correo de
    /// prueba en este ordenador).
    #[test]
    fn the_automatic_email_needs_both_switches_and_reaches_the_fake_supplier() {
        let (mut db, sid, pid) = shop();
        let (port, srv) = crate::mail::tests::fake_smtp();
        db.mail = crate::mail::MailConfig { enabled: true, host: "127.0.0.1".into(), port, security: crate::mail::Security::Ninguna, from_name: "Clínica".into(), from_email: "clinica@example.com".into(), ..Default::default() };
        let id = db.new_id();
        db.orders.push(crate::stock::PurchaseOrder { id, supplier_id: sid, date: at(2).date(), lines: vec![OrderLine { product_id: pid, qty: 100.0, cost: 0.07, ..Default::default() }], ..Default::default() });
        db.order_submit(id, at(2)).unwrap();
        assert!(!db.order_auto_send(id), "desactivado de entrada");
        db.config.orders_auto_send = true;
        assert!(!db.order_auto_send(id), "falta marcar al proveedor");
        db.suppliers.iter_mut().find(|s| s.id == sid).unwrap().auto_send = true;
        assert!(db.order_auto_send(id));
        let (to, subject, text) = db.order_email(id).unwrap();
        crate::mail::send(&db.mail, &to, &subject, &text, &[]).unwrap();
        db.order_sent(id, "correo automático", at(2)).unwrap();
        let (cmds, data) = srv.join().unwrap();
        assert!(cmds.iter().any(|c| c.to_uppercase().contains("PEDIDOS@PROVEEDOR-FICTICIO.EXAMPLE")));
        assert!(data.contains("Guantes talla S"));
        assert!(!db.order_auto_send(id), "ya enviado, no se manda otra vez");
    }
}

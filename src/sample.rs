//! Datos de ejemplo (ficticios) para estrenar la aplicación.

use crate::model::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};

fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn tr(code: &str, desc: &str, group: &str, price: f64, min: u32, odg: OdgKind, per_tooth: bool, recall: u32) -> Treatment {
    Treatment { code: code.into(), description: desc.into(), group: group.into(), price, minutes: min, odg, per_tooth, recall_months: recall, active: true, only_tariff: None }
}

fn plain(s: &str) -> String {
    fold(s).replace(' ', "")
}

impl Db {
    pub fn sample(today: NaiveDate) -> Db {
        use OdgKind::*;
        let jan1 = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap();
        let treatments = vec![
            tr("0101", "Primera visita y diagnóstico", "Diagnóstico", 0.0, 30, Ninguno, false, 0),
            tr("0102", "Revisión periódica", "Diagnóstico", 25.0, 20, Ninguno, false, 6),
            tr("0103", "Radiografía periapical", "Radiología", 15.0, 10, Ninguno, true, 0),
            tr("0104", "Ortopantomografía", "Radiología", 40.0, 15, Ninguno, false, 0),
            tr("0105", "TAC dental (CBCT)", "Radiología", 90.0, 20, Ninguno, false, 0),
            tr("0201", "Higiene / tartrectomía", "Prevención", 60.0, 45, Higiene, false, 12),
            tr("0202", "Aplicación de flúor", "Prevención", 20.0, 15, Ninguno, false, 0),
            tr("0203", "Sellador de fisuras", "Prevención", 25.0, 15, Sellador, true, 0),
            tr("0301", "Obturación composite 1 cara", "Conservadora", 55.0, 30, Obturacion, true, 0),
            tr("0302", "Obturación composite 2 caras", "Conservadora", 70.0, 40, Obturacion, true, 0),
            tr("0303", "Obturación composite 3+ caras", "Conservadora", 85.0, 45, Obturacion, true, 0),
            tr("0304", "Reconstrucción", "Conservadora", 95.0, 50, Obturacion, true, 0),
            tr("0401", "Endodoncia unirradicular", "Endodoncia", 180.0, 60, Endodoncia, true, 0),
            tr("0402", "Endodoncia birradicular", "Endodoncia", 220.0, 75, Endodoncia, true, 0),
            tr("0403", "Endodoncia multirradicular", "Endodoncia", 280.0, 90, Endodoncia, true, 0),
            tr("0404", "Perno-muñón", "Endodoncia", 90.0, 30, PernoMunon, true, 0),
            tr("0405", "Apicectomía", "Cirugía", 250.0, 60, Apicectomia, true, 0),
            tr("0501", "Extracción simple", "Cirugía", 50.0, 30, Extraccion, true, 0),
            tr("0502", "Extracción cordal incluido", "Cirugía", 150.0, 60, Extraccion, true, 0),
            tr("0601", "Raspado y alisado (cuadrante)", "Periodoncia", 80.0, 45, Ninguno, false, 4),
            tr("0602", "Mantenimiento periodontal", "Periodoncia", 70.0, 45, Higiene, false, 4),
            tr("0701", "Implante dental", "Implantología", 950.0, 90, Implante, true, 12),
            tr("0702", "Corona sobre implante", "Implantología", 700.0, 45, Corona, true, 12),
            tr("0703", "Injerto óseo", "Implantología", 350.0, 60, Ninguno, true, 0),
            tr("0801", "Corona metal-porcelana", "Prótesis", 450.0, 60, Corona, true, 0),
            tr("0802", "Corona zirconio", "Prótesis", 600.0, 60, Corona, true, 0),
            tr("0803", "Puente (por pieza)", "Prótesis", 420.0, 60, Puente, true, 0),
            tr("0804", "Prótesis removible acrílica", "Prótesis", 650.0, 45, ProtesisRemovible, false, 0),
            tr("0805", "Prótesis esquelética", "Prótesis", 850.0, 45, ProtesisRemovible, false, 0),
            tr("0901", "Carilla composite", "Estética", 180.0, 45, Carilla, true, 0),
            tr("0902", "Carilla porcelana", "Estética", 450.0, 60, Carilla, true, 0),
            tr("0903", "Blanqueamiento", "Estética", 300.0, 60, Ninguno, false, 0),
            tr("1001", "Estudio de ortodoncia", "Ortodoncia", 120.0, 45, Ninguno, false, 0),
            tr("1002", "Ortodoncia brackets (mensualidad)", "Ortodoncia", 90.0, 20, Bracket, false, 1),
            tr("1003", "Férula de descarga", "Ortodoncia", 250.0, 30, Ninguno, false, 0),
            tr("9001", "Ausencia (pieza no presente)", "Odontograma", 0.0, 0, Ausente, true, 0),
        ];

        let tariff = |id: u32, name: &str, ins: bool, disc: f64, prices: Vec<(&str, f64)>, notes: &str| Tariff {
            id,
            name: name.into(),
            is_insurer: ins,
            discount_pct: disc,
            prices: prices.into_iter().map(|(c, p)| TariffPrice { code: c.into(), price: p, insurer_fee: 0.0 }).collect(),
            phone: if ins { format!("900 000 00{id}") } else { String::new() },
            notes: notes.into(),
            valid_from: jan1,
            valid_to: None,
            inactive: false,
        };
        let tariffs = vec![
            tariff(1, "Privado", false, 0.0, vec![], "Tarifa general de la clínica"),
            tariff(2, "Aseguradora Dental A", true, 20.0, vec![("0101", 0.0), ("0102", 0.0), ("0201", 0.0), ("0501", 0.0), ("0103", 0.0)], "Póliza dental con actos incluidos"),
            tariff(3, "Mutua Colectivos B", true, 15.0, vec![("0101", 0.0), ("0201", 30.0)], ""),
            tariff(4, "Convenio Empresa C", true, 10.0, vec![], "10% de descuento para empleados"),
        ];

        let col = |id: u32, alias: &str, name: &str, spec: &str, color: [u8; 3], pct: f64| Collaborator {
            id,
            code: format!("{id:02}"),
            alias: alias.into(),
            name: name.into(),
            specialty: spec.into(),
            license: if spec == "Higienista" { "-".into() } else { format!("28-{:04}", id * 137) },
            color,
            commission_pct: pct,
            active: true,
            phone: format!("600 100 00{id}"),
            centers: vec![],
            deduct_lab: false,
            commission_on_insurer: false,
            hours: None,
        };
        let collaborators = vec![
            col(1, "Dra. Ruiz", "Elena Ruiz Prado", "Odontología general", [56, 132, 214], 35.0),
            col(2, "Dr. Navarro", "Pablo Navarro Gil", "Implantología y cirugía", [46, 160, 104], 40.0),
            col(3, "Dra. Soler", "Marta Soler Vidal", "Ortodoncia", [214, 76, 120], 35.0),
            col(4, "Hig. Campos", "Lucía Campos Rey", "Higienista", [232, 152, 36], 20.0),
        ];
        let boxes = vec![
            Boxroom { center_id: 1, id: 1, name: "Gabinete 1".into() },
            Boxroom { center_id: 1, id: 2, name: "Gabinete 2".into() },
            Boxroom { center_id: 1, id: 3, name: "Gabinete 3".into() },
            Boxroom { center_id: 1, id: 4, name: "Higiene".into() },
        ];

        let names = [
            ("Ana", "García López", "F", 1985, 1),
            ("Carlos", "Martínez Sanz", "M", 1972, 2),
            ("Laura", "Fernández Ortiz", "F", 1990, 1),
            ("Javier", "Moreno Díaz", "M", 1968, 3),
            ("Sofía", "Romero Navas", "F", 2012, 1),
            ("Miguel", "Torres Blanco", "M", 1955, 2),
            ("Lucía", "Jiménez Prieto", "F", 1998, 4),
            ("David", "Ruiz Castillo", "M", 1981, 1),
            ("Marta", "Álvarez Rubio", "F", 1977, 2),
            ("Pedro", "Sánchez Molina", "M", 1949, 1),
            ("Elena", "Domínguez Gil", "F", 1993, 3),
            ("Hugo", "Vázquez Marín", "M", 2015, 1),
            ("Carmen", "Ramos Herrero", "F", 1962, 2),
            ("Raúl", "Iglesias Peña", "M", 1988, 1),
            ("Isabel", "Ortega Cano", "F", 1970, 4),
            ("Andrés", "Delgado Vega", "M", 1995, 1),
            ("Paula", "Castro Medina", "F", 2008, 1),
            ("Jorge", "Rubio Santos", "M", 1959, 3),
        ];
        let cities = ["Madrid", "Alcalá de Henares", "Getafe", "Móstoles", "Leganés", "Alcorcón"];
        let letters: Vec<char> = "TRWAGMYFPDXBNJZSQVHLCKE".chars().collect();
        let mut patients = vec![];
        for (i, (n, a, s, y, tar)) in names.iter().enumerate() {
            let num = 10_000_000 + i as u32 * 731_117;
            patients.push(Patient {
                id: 100 + i as u32,
                history_no: 1001 + i as u32,
                first_name: n.to_string(),
                last_name: a.to_string(),
                nif: format!("{num:08}{}", letters[(num % 23) as usize]),
                birth: NaiveDate::from_ymd_opt(*y, (i % 12) as u32 + 1, (i % 27) as u32 + 1),
                sex: s.to_string(),
                phone: format!("91 555 {:02} {:02}", 10 + i, 30 + i),
                mobile: format!("6{:02} {:03} {:03}", 10 + i * 3, 200 + i * 7, 100 + i * 11),
                email: format!("{}.{}@correo.es", plain(n), plain(a.split(' ').next().unwrap())),
                address: format!("C/ Mayor, {}", 3 + i * 4),
                zip: format!("28{:03}", 10 + i * 13),
                city: cities[i % cities.len()].to_string(),
                province: "Madrid".into(),
                tariff_id: *tar,
                policy_no: if *tar > 1 { format!("POL-{:06}", 482_000 + i * 97) } else { String::new() },
                doctor_id: (i as u32 % 3) + 1,
                alerts: match i {
                    1 => "Anticoagulado. Consultar antes de cirugía.".into(),
                    5 => "Hipertenso. Diabético tipo 2.".into(),
                    9 => "Portador de marcapasos.".into(),
                    _ => String::new(),
                },
                allergies: match i {
                    1 => "Penicilina".into(),
                    8 => "Látex".into(),
                    _ => String::new(),
                },
                background: String::new(),
                notes: String::new(),
                registered: Some(today - Duration::days(30 + i as i64 * 41)),
                accepts_gdpr: i % 5 != 3,
                accepts_sms: true,
                accepts_email: i % 3 != 0,
                accepts_whatsapp: i % 2 == 0,
                referral: ["Web", "Recomendación", "Redes sociales", "Aseguradora", "Paso por la calle"][i % 5].into(),
                profession: String::new(),
                active: true,
                discount_pct: 0.0,
                ..Default::default()
            });
        }

        let mut db = Db {
            config: ClinicConfig {
                name: "Clínica Dental Centro".into(),
                nif: "B00000000".into(),
                address: "C/ Ejemplo 1, 28001 Madrid".into(),
                phone: "91 000 00 00".into(),
                email: "recepcion@clinica-ejemplo.es".into(),
                open_time: t(9, 0),
                close_time: t(20, 0),
                hours: None,
                slot_minutes: 15,
                receipt_series: format!("R{}", today.year()),
                next_receipt: 1,
                budget_valid_days: 90,
                users: vec!["Recepción".into(), "Dirección".into(), "Dra. Ruiz".into()],
                current_user: "Recepción".into(),
                current_center: 1,
                creditor_id: String::new(),
                clinic_iban: String::new(),
                backup_extra: String::new(),
                board_initials: true,
                vat_exemption: Default::default(),
                mig_profiles: vec![],
                support_email: String::new(),
                iae_epigrafe: String::new(),
                actividad_tipo: String::new(),
                kpi_goals: Default::default(),
                pay_online: Default::default(),
                evo_templates: crate::evolucion::default_templates(),
                evo_ai: false,
                surveys: Default::default(),
                ui_lang: Default::default(),
            },
            treatments,
            tariffs: tariffs.into(),
            collaborators: collaborators.into(),
            boxes: boxes.into(),
            patients: patients.into(),
            entries: vec![].into(),
            budgets: vec![].into(),
            payments: vec![].into(),
            cash: vec![].into(),
            appointments: vec![].into(),
            blocks: vec![].into(),
            waitlist: vec![].into(),
            recalls: vec![].into(),
            next_id: 1000,
            invoices: vec![].into(),
            perio: vec![].into(),
            images: vec![].into(),
            connectors: default_connectors().into(),
            launches: vec![],
            msg_templates: crate::comms::default_msg_templates().into(),
            messages: vec![].into(),
            doc_templates: crate::comms::default_doc_templates().into(),
            documents: vec![].into(),
            suppliers: vec![].into(),
            products: vec![].into(),
            stock_moves: vec![].into(),
            consumptions: vec![],
            orders: vec![].into(),
            lab_works: vec![].into(),
            users: crate::users::default_users().into(),
            access_log: vec![],
            expenses: vec![].into(),
            drugs: crate::finance::default_drugs(),
            prescriptions: vec![].into(),
            ortho: vec![].into(),
            remittances: vec![].into(),
            contacts: vec![].into(),
            bank_accounts: vec![].into(),
            bank_moves: vec![].into(),
            audit_log: vec![],
            centers: default_centers().into(),
            holidays: crate::agenda_extra::default_holidays(today.year()),
            health: vec![].into(),
            claims: vec![].into(),
            auths: vec![],
            erx: Default::default(),
            tasks: vec![].into(),
            rev: 0,
            stations: vec![].into(),
            chat: vec![].into(),
            calls: vec![].into(),
            whatsapp: vec![].into(),
            wa_seen: vec![],
            wa_sender: String::new(),
            ia: Default::default(),
            verifactu: Default::default(),
            mail: Default::default(),
            vf_records: vec![].into(),
            ia_reports: vec![].into(),
            booking: Default::default(),
            booking_requests: vec![].into(),
            appt_links: vec![].into(),
            budget_links: vec![].into(),
            doc_links: vec![].into(),
            slot_offers: vec![].into(),
            golive: vec![],
            rx_links: vec![].into(),
            imports: vec![].into(),
            measures: vec![].into(),
            rights_requests: vec![].into(),
            pay_requests: vec![].into(),
            evo_notes: vec![].into(),
            alert_acks: vec![].into(),
            sterile_cycles: vec![].into(),
            sterile_packs: vec![].into(),
            surveys: vec![].into(),
            portal_access: vec![].into(),
            campaigns: vec![].into(),
            group_keys: vec![].into(),
            card_txs: vec![].into(),
            financings: vec![].into(),
            punches: vec![].into(),
            data_version: DATA_VERSION,
            station: 0,
        };

        for d in 1..=4 {
            let id = db.new_id();
            db.blocks.push(Block { id, doctor_id: d, date: None, start: t(14, 0), end: t(15, 30), reason: "Comida".into() });
        }

        let past = |n: i64| today - Duration::days(n);
        use TxState::*;
        // (paciente, código, piezas, caras, estado, doctor, hace cuántos días)
        type Done<'a> = (u32, &'a str, Vec<u8>, Vec<&'a str>, TxState, u32, i64);
        let ex: Vec<Done> = vec![
            (100, "0101", vec![], vec![], Realizado, 1, 120),
            (100, "0201", vec![], vec![], Realizado, 4, 118),
            (100, "0302", vec![16], vec!["O", "M"], Realizado, 1, 90),
            (100, "0301", vec![25], vec!["O"], Realizado, 1, 60),
            (100, "0403", vec![36], vec![], Realizado, 1, 30),
            (100, "0802", vec![36], vec![], Previsto, 1, 30),
            (100, "0301", vec![47], vec!["O"], Previsto, 1, 30),
            (100, "9001", vec![18], vec![], Previo, 1, 120),
            (100, "9001", vec![28], vec![], Previo, 1, 120),
            (100, "0801", vec![14], vec![], Previo, 1, 120),
            (101, "0101", vec![], vec![], Realizado, 2, 200),
            (101, "0701", vec![46], vec![], Realizado, 2, 150),
            (101, "0702", vec![46], vec![], Realizado, 2, 60),
            (101, "0501", vec![37], vec![], Realizado, 2, 150),
            (101, "0701", vec![36], vec![], Previsto, 2, 10),
            (101, "0702", vec![36], vec![], Previsto, 2, 10),
            (101, "9001", vec![38, 48], vec![], Previo, 2, 200),
            (102, "0201", vec![], vec![], Realizado, 4, 15),
            (102, "0901", vec![11, 21], vec![], Previsto, 1, 15),
            (103, "0102", vec![], vec![], Realizado, 3, 40),
            (103, "1002", vec![], vec![], Realizado, 3, 35),
            (104, "0203", vec![36, 46], vec!["O"], Realizado, 1, 80),
            (105, "0805", vec![], vec![], Previsto, 2, 20),
            (105, "0501", vec![44], vec![], Realizado, 2, 22),
            (105, "0803", vec![24, 25, 26], vec![], Previo, 2, 300),
            (107, "0401", vec![21], vec![], Realizado, 1, 12),
            (107, "0404", vec![21], vec![], Realizado, 1, 8),
            (107, "0802", vec![21], vec![], Previsto, 1, 8),
            (108, "0601", vec![], vec![], Realizado, 4, 50),
            (109, "0804", vec![], vec![], Realizado, 2, 70),
            (112, "0302", vec![15], vec!["O", "D"], Realizado, 1, 5),
            (113, "0201", vec![], vec![], Realizado, 4, 3),
            (113, "0902", vec![12, 11, 21, 22], vec![], Previsto, 1, 3),
        ];
        for (pid, code, teeth, faces, st, doc, ago) in ex {
            db.add_entry(pid, code, teeth, faces.iter().map(|s| s.to_string()).collect(), st, doc, past(ago));
        }

        let set_budget = |db: &mut Db, b: u32, title: &str, ago: i64| {
            if let Some(bb) = db.budgets.iter_mut().find(|x| x.id == b) {
                bb.date = past(ago);
                bb.title = title.into();
            }
        };
        if let Some(b) = db.budget_from_planned(100) {
            set_budget(&mut db, b, "Rehabilitación 36 y obturación 47", 30);
            db.accept_budget(b, past(28));
        }
        if let Some(b) = db.budget_from_planned(101) {
            set_budget(&mut db, b, "Implante 36", 10);
            if let Some(bb) = db.budgets.iter_mut().find(|x| x.id == b) {
                bb.plan = Some(FinancePlan { down_payment: 300.0, installments: 6, first_date: today, every_months: 1, paid: vec![false; 6] });
            }
        }
        for (pid, title, ago) in [(102, "Carillas sector anterior", 14), (105, "Prótesis esquelética superior", 20), (107, "Corona 21", 8), (113, "Estética anterior", 3)] {
            if let Some(b) = db.budget_from_planned(pid) {
                set_budget(&mut db, b, title, ago);
            }
        }
        if let Some(bid) = db.budgets.iter().find(|x| x.patient_id == 113).map(|b| b.id) {
            db.reject_budget(bid, "Lo pensará más adelante");
        }
        if let Some(bid) = db.budgets.iter().find(|x| x.patient_id == 107).map(|b| b.id) {
            db.accept_budget(bid, past(6));
        }

        let pays = [
            (100, 300.0, "Tarjeta", 90),
            (100, 280.0, "Efectivo", 30),
            (101, 950.0, "Transferencia", 150),
            (101, 700.0, "Financiación", 60),
            (102, 60.0, "Tarjeta", 15),
            (103, 90.0, "Efectivo", 35),
            (105, 50.0, "Efectivo", 22),
            (107, 180.0, "Tarjeta", 12),
            (108, 80.0, "Bizum", 50),
            (109, 400.0, "Tarjeta", 70),
        ];
        for (pid, amt, m, ago) in pays {
            let id = db.add_payment(pid, amt, m, "Pago a cuenta");
            if let Some(p) = db.payments.iter_mut().find(|p| p.id == id) {
                p.date = past(ago).and_time(t(11, 30));
            }
        }

        let cid = db.new_id();
        db.cash.push(CashSession {
            id: cid,
            opened: today.and_time(t(8, 55)),
            closed: None,
            opening_balance: 150.0,
            moves: vec![CashMove { when: today.and_time(t(10, 5)), amount: -12.5, concept: "Material de oficina".into() }],
            counted: None,
            state: CashState::Abierta,
            user: "Recepción".into(),
            notes: String::new(),
            center_id: 1,
        });
        for (pid, amt, m) in [(112, 70.0, "Efectivo"), (113, 60.0, "Tarjeta"), (107, 90.0, "Efectivo")] {
            let id = db.add_payment(pid, amt, m, "Pago tratamiento del día");
            if let Some(p) = db.payments.iter_mut().find(|p| p.id == id) {
                p.date = today.and_time(t(9, 40 + (pid % 10)));
            }
        }

        use ApptState::*;
        // (días desde hoy, doctor, hora, minuto, duración, paciente, motivo, código, estado)
        type Planned<'a> = (i64, u32, u32, u32, u32, Option<u32>, &'a str, &'a str, ApptState);
        let mut plan: Vec<Planned> = vec![
            (0, 1, 9, 0, 30, Some(100), "Obturación 47", "0301", Finalizada),
            (0, 1, 9, 30, 60, Some(107), "Prueba corona 21", "0802", Finalizada),
            (0, 1, 10, 45, 30, Some(112), "Revisión", "0102", EnConsulta),
            (0, 1, 11, 15, 45, Some(102), "Valorar carillas", "0901", SalaEspera),
            (0, 1, 12, 15, 30, None, "Primera visita", "0101", Confirmada),
            (0, 1, 16, 0, 60, Some(116), "Endodoncia 46", "0403", Planificada),
            (0, 1, 17, 30, 30, Some(110), "Revisión", "0102", Planificada),
            (0, 2, 9, 30, 90, Some(101), "Cirugía implante 36", "0701", Finalizada),
            (0, 2, 11, 30, 30, Some(105), "Toma de impresiones", "0805", SalaEspera),
            (0, 2, 12, 15, 60, Some(109), "Ajuste prótesis", "0804", Planificada),
            (0, 2, 16, 30, 60, Some(117), "Extracción cordal", "0502", Confirmada),
            (0, 3, 10, 0, 20, Some(103), "Control brackets", "1002", Finalizada),
            (0, 3, 10, 30, 20, Some(104), "Revisión infantil", "0102", NoAcude),
            (0, 3, 11, 0, 45, Some(111), "Estudio ortodoncia", "1001", Recepcion),
            (0, 3, 16, 0, 20, Some(116), "Control brackets", "1002", Planificada),
            (0, 4, 9, 0, 45, Some(113), "Higiene", "0201", Finalizada),
            (0, 4, 10, 0, 45, Some(108), "Mantenimiento perio", "0602", EnConsulta),
            (0, 4, 11, 0, 45, Some(114), "Higiene", "0201", Planificada),
            (0, 4, 12, 30, 45, Some(106), "Higiene", "0201", Anulada),
            (0, 4, 16, 0, 45, Some(115), "Higiene", "0201", Planificada),
            (1, 1, 9, 30, 45, Some(100), "Corona 36 - tallado", "0802", Confirmada),
            (1, 2, 10, 0, 60, Some(101), "Revisión implante", "0102", Planificada),
            (1, 4, 12, 0, 45, Some(110), "Higiene", "0201", Planificada),
            (2, 1, 11, 0, 60, Some(107), "Cementado corona 21", "0802", Planificada),
            (2, 3, 16, 0, 20, Some(103), "Control brackets", "1002", Planificada),
            (3, 2, 9, 0, 90, Some(105), "Prueba esquelético", "0805", Planificada),
            (-1, 1, 10, 0, 30, Some(110), "Revisión", "0102", Finalizada),
            (-1, 4, 11, 0, 45, Some(102), "Higiene", "0201", Finalizada),
        ];
        for row in plan.iter_mut() {
            let mut d = today + Duration::days(row.0);
            if row.0 != 0 {
                while d.weekday().num_days_from_monday() >= 5 {
                    d += Duration::days(if row.0 < 0 { -1 } else { 1 });
                }
            }
            row.0 = (d - today).num_days();
        }
        for (off, doc, h, m, min, pid, reason, code, st) in plan {
            let id = db.new_id();
            let start = t(h, m);
            let arrived = matches!(st, SalaEspera | EnConsulta | Finalizada | Recepcion).then(|| start - Duration::minutes(8));
            let seen = matches!(st, EnConsulta | Finalizada).then_some(start);
            db.appointments.push(Appointment {
                id,
                date: today + Duration::days(off),
                start,
                minutes: min,
                patient_id: pid,
                free_name: if pid.is_none() { "Nuria Peña".into() } else { String::new() },
                phone: if pid.is_none() { "622 000 111".into() } else { String::new() },
                doctor_id: doc,
                box_id: doc,
                reason: reason.into(),
                treatment_code: code.into(),
                state: st,
                notes: String::new(),
                arrived_at: arrived,
                seen_at: seen,
                finished_at: (st == Finalizada).then(|| start + Duration::minutes(min as i64)),
                reminder_sent: off >= 0,
                first_visit: pid.is_none(),
                center_id: 1,
            });
        }

        let rc = [(104, "Revisión semestral", -5, 1), (106, "Higiene anual", -2, 4), (110, "Revisión semestral", 3, 1), (114, "Higiene anual", 8, 4), (117, "Revisión semestral", 14, 2), (102, "Seguimiento de presupuesto", 1, 1)];
        for (pid, reason, off, doc) in rc {
            let id = db.new_id();
            db.recalls.push(Recall { id, patient_id: pid, date: today + Duration::days(off), reason: reason.into(), doctor_id: doc, state: RecallState::Pendiente, channel: String::new(), comment: String::new(), contacted_on: None, appt_id: None });
        }

        let wid = db.new_id();
        db.waitlist.push(WaitItem { id: wid, patient_id: 106, doctor_id: 4, reason: "Higiene (adelantar si hay hueco)".into(), since: today - Duration::days(3), preference: "Mañanas".into() });
        let wid = db.new_id();
        db.waitlist.push(WaitItem { id: wid, patient_id: 115, doctor_id: 1, reason: "Revisión molestia 26".into(), since: today - Duration::days(1), preference: "Tardes".into() });

        // Facturas emitidas (en orden cronológico para que la cadena sea coherente).
        for (pid, ago) in [(101, 140), (109, 65), (100, 25)] {
            let ids: Vec<u32> = db.uninvoiced(pid).iter().filter(|e| e.date <= past(ago)).map(|e| e.id).collect();
            let _ = db.issue_invoice(pid, &ids, past(ago));
        }

        // Imágenes de demostración (generadas por la aplicación).
        for (pid, kind, ago, teeth, src, notes) in [
            (100, ImageKind::Panoramica, 120, vec![], "Ortopantomógrafo", "Estudio inicial"),
            (100, ImageKind::Periapical, 30, vec![36], "Radiovisiógrafo (RVG)", "Conductometría 36"),
            (100, ImageKind::Periapical, 29, vec![36], "Radiovisiógrafo (RVG)", "Control final endodoncia"),
            (101, ImageKind::Panoramica, 200, vec![], "Ortopantomógrafo", "Primera visita"),
            (101, ImageKind::Tac, 15, vec![36, 46], "TAC / CBCT 3D", "Planificación implante 36"),
            (101, ImageKind::Periapical, 60, vec![46], "Radiovisiógrafo (RVG)", "Control implante 46"),
            (102, ImageKind::FotoIntraoral, 15, vec![11, 21], "Cámara intraoral", "Estudio estético"),
            (107, ImageKind::Periapical, 12, vec![21], "Radiovisiógrafo (RVG)", "Endodoncia 21"),
        ] {
            let id = db.new_id();
            db.images.push(PatientImage { id, patient_id: pid, kind, date: past(ago), teeth, path: String::new(), source: src.into(), notes: notes.into(), demo: true });
        }
        for c in db.connectors.iter_mut() {
            c.enabled = c.id <= 4;
        }
        // Un análisis con IA de ejemplo (la panorámica de Carlos Martínez).
        if let Some(img) = db.images.iter().find(|i| i.patient_id == 101 && i.kind == ImageKind::Panoramica).map(|i| i.id) {
            use crate::ia::{Analysis, Attempt, Finding, IaReport, Proposal};
            let f = |t: u8, kind: &str, faces: &[&str], desc: &str, sev: &str, conf: f32| Finding {
                tooth: Some(t),
                kind: kind.into(),
                faces: faces.iter().map(|x| x.to_string()).collect(),
                description: desc.into(),
                severity: sev.into(),
                confidence: conf,
            };
            let analysis = Analysis {
                summary: "Panorámica de adulto con implante osteointegrado en 46 y ausencia de 36 con buen volumen óseo aparente. Posible caries oclusal en 17 y cálculo en la zona anteroinferior.".into(),
                quality: "buena".into(),
                findings: vec![
                    f(46, "implante", &[], "Implante con corona, sin pérdida ósea marginal visible.", "baja", 0.9),
                    f(36, "ausencia", &[], "Espacio edéntulo; reborde con altura suficiente aparente.", "media", 0.85),
                    f(17, "caries", &["O"], "Radiolucidez oclusal compatible con caries.", "media", 0.6),
                    f(41, "calculo", &[], "Imagen compatible con cálculo en incisivos inferiores.", "baja", 0.55),
                ],
                proposals: vec![
                    Proposal { kind: "obturacion".into(), teeth: vec![17], faces: vec!["O".into()], reason: "Caries oclusal en 17.".into() },
                    Proposal { kind: "implante".into(), teeth: vec![36], faces: vec![], reason: "Reponer la pieza ausente.".into() },
                    Proposal { kind: "limpieza".into(), teeth: vec![], faces: vec![], reason: "Retirar el cálculo y revisar la higiene.".into() },
                ],
            };
            let id = db.new_id();
            db.ia_reports.push(IaReport {
                id,
                patient_id: 101,
                image_id: img,
                when: past(20).and_hms_opt(10, 12, 0).unwrap(),
                agent: "OpenRouter · nemotron-3.5-super-vl:free".into(),
                model: "nvidia/nemotron-3.5-super-vl:free".into(),
                free: true,
                analysis,
                attempts: vec![
                    Attempt { agent: "OpenRouter · gemma-4-31b-it:free".into(), model: "google/gemma-4-31b-it:free".into(), free: true, ok: false, message: "límite de uso alcanzado (429)".into(), millis: 900 },
                    Attempt { agent: "OpenRouter · nemotron-3.5-super-vl:free".into(), model: "nvidia/nemotron-3.5-super-vl:free".into(), free: true, ok: true, message: "respondió".into(), millis: 14200 },
                ],
                user: "Dr. Navarro".into(),
                planned: vec![],
                review: None,
                prompt_version: crate::ia::PROMPT_VERSION.into(),
            });
        }

        // Documentos firmados de ejemplo (firma manuscrita sintética).
        let scribble = |seed: f32| -> Vec<Vec<[f32; 2]>> {
            let a: Vec<[f32; 2]> = (0..60)
                .map(|i| {
                    let t = i as f32 / 59.0;
                    [0.08 + t * 0.55, 0.55 + 0.18 * (t * 18.0 + seed).sin() * (1.0 - t * 0.5) - 0.1 * t]
                })
                .collect();
            let b: Vec<[f32; 2]> = (0..30)
                .map(|i| {
                    let t = i as f32 / 29.0;
                    [0.6 + t * 0.3, 0.62 - 0.12 * (t * 9.0 + seed).cos()]
                })
                .collect();
            vec![a, b, vec![[0.12, 0.78], [0.85, 0.74]]]
        };
        for (pid, tpl, teeth, seed) in [(100u32, 1u32, vec![], 0.3f32), (100, 4, vec![36u8], 1.1), (101, 1, vec![], 2.0), (101, 3, vec![36], 0.7), (101, 2, vec![37], 1.7)] {
            if let Some(id) = db.new_document(pid, tpl, &teeth) {
                let name = db.patient_name(pid);
                let _ = db.sign_document(id, &name, scribble(seed));
            }
        }
        if let Some(id) = db.new_document(105, 5, &[]) {
            let _ = id;
        }

        crate::stock::sample_stock(&mut db, today);
        crate::finance::sample_finance(&mut db, today);
        crate::extra::sample_extra(&mut db, today);
        crate::tools::sample_contacts(&mut db);
        crate::bank::sample_bank(&mut db, today);
        crate::health::sample_health(&mut db, today);
        crate::insurer::sample_insurer(&mut db);
        crate::tasks::sample_tasks(&mut db, today);
        crate::phone::sample_calls(&mut db);
        crate::whatsapp::sample_whatsapp(&mut db, today);
        crate::booking::sample_booking(&mut db, today);

        // Periodontogramas de la paciente en mantenimiento periodontal.
        let mk = |seed: u32, worse: u8| -> Vec<PerioTooth> {
            crate::screens::odontograma::UPPER_ADULT
                .iter()
                .chain(crate::screens::odontograma::LOWER_ADULT.iter())
                .enumerate()
                .map(|(i, &t)| {
                    let mut d = [0u8; 6];
                    let mut b = [false; 6];
                    for s in 0..6 {
                        let h = (i as u32 * 7 + s as u32 * 13 + seed) % 11;
                        let molar = t % 10 >= 6;
                        d[s] = 2 + (h % 3) as u8 + if molar && (s == 0 || s == 2 || s == 3 || s == 5) { worse } else { 0 };
                        b[s] = d[s] >= 4 && h.is_multiple_of(2);
                    }
                    PerioTooth {
                        tooth: t,
                        depth: d,
                        recession: [if t % 10 <= 3 { 1 } else { 0 }; 6],
                        bleeding: b,
                        plaque: [i % 4 == 0; 6],
                        mobility: if (t == 31 || t == 41) && worse > 1 { 1 } else { 0 },
                        furcation: if (t == 36 || t == 46) && worse > 1 { 1 } else { 0 },
                        missing: t == 18 || t == 28,
                    }
                })
                .collect()
        };
        let id = db.new_id();
        db.perio.push(PerioChart { id, patient_id: 108, date: past(170), doctor_id: 4, teeth: mk(3, 3), notes: "Periodontitis estadio II. Raspado por cuadrantes.".into() });
        let id = db.new_id();
        db.perio.push(PerioChart { id, patient_id: 108, date: past(50), doctor_id: 4, teeth: mk(5, 1), notes: "Reevaluación tras raspado: mejoría.".into() });
        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    #[test]
    fn tariff_prices_apply() {
        let db = Db::sample(d());
        assert_eq!(db.price_for("0201", 1), 60.0);
        assert_eq!(db.price_for("0201", 2), 0.0); // incluido en póliza
        assert_eq!(db.price_for("0701", 2), 760.0); // 20% dto
        assert_eq!(db.price_for("0201", 3), 30.0);
    }

    #[test]
    fn balance_is_debit_minus_paid() {
        let db = Db::sample(d());
        let pid = 100;
        assert_eq!(db.patient_balance(pid), round2(db.patient_debit(pid) - db.patient_paid(pid)));
    }

    #[test]
    fn accept_budget_links_planned() {
        let mut db = Db::sample(d());
        let pid = 102;
        let bid = db.budgets.iter().find(|b| b.patient_id == pid).unwrap().id;
        db.accept_budget(bid, d());
        assert_eq!(db.budgets.iter().find(|b| b.id == bid).unwrap().state, BudgetState::Aceptado);
        let linked = db.entries.iter().filter(|e| e.budget_id == Some(bid)).count();
        assert_eq!(linked, 1);
    }

    #[test]
    fn realizing_hygiene_creates_recall() {
        let mut db = Db::sample(d());
        let pid = 115;
        db.add_entry(pid, "0201", vec![], vec![], TxState::Realizado, 4, d());
        assert!(db.recalls.iter().any(|r| r.patient_id == pid && r.reason == "Higiene anual"));
    }

    #[test]
    fn gaps_avoid_lunch_and_busy() {
        let db = Db::sample(d());
        let gaps = db.find_gaps(1, d(), 60, t(13, 30), t(16, 0), 1);
        for (_, s) in gaps {
            assert!(s + Duration::minutes(60) <= t(14, 0) || s >= t(15, 30));
        }
    }

    #[test]
    fn invoices_are_correlative_and_chained() {
        let mut db = Db::sample(d());
        let n0 = db.invoices.len();
        assert!(n0 >= 2);
        assert_eq!(db.verify_chain(), Ok(n0));
        // Nueva factura con lo pendiente de un paciente.
        let ids: Vec<u32> = db.uninvoiced(112).iter().map(|e| e.id).collect();
        let id = db.issue_invoice(112, &ids, d()).unwrap();
        let inv = db.invoice(id).unwrap().clone();
        let max_prev = db.invoices.iter().filter(|i| i.series == inv.series && i.id != id).map(|i| i.number).max().unwrap_or(0);
        assert_eq!(inv.number, max_prev + 1);
        assert!(db.uninvoiced(112).is_empty());
        // No se puede facturar dos veces.
        assert!(db.issue_invoice(112, &ids, d()).is_err());
        // Rectificar libera los tratamientos y mantiene la cadena.
        let r = db.rectify_invoice(id, "Error en el importe").unwrap();
        assert!((db.invoice(r).unwrap().total() + inv.total()).abs() < 0.001);
        assert!(!db.uninvoiced(112).is_empty());
        assert_eq!(db.verify_chain(), Ok(n0 + 2));
        // Alterar un importe rompe la cadena.
        db.invoices[0].lines[0].price += 1.0;
        assert!(db.verify_chain().is_err());
    }

    #[test]
    fn imaging_bridge_and_args() {
        let db = Db::sample(d());
        let ini = db.bridge_ini(100);
        assert!(ini.starts_with("[DATOS FILIACION]"));
        assert!(ini.contains("NumPac=1001"));
        assert!(ini.contains("Nombre=ANA"));
        assert!(ini.contains("Sexo=M")); // M = mujer, como el original
        let args = db.expand_args("-P\\GESDEN\\[NUMPAC] -N[NOMPAC] [APELPAC]", 100);
        assert_eq!(args, "-P\\GESDEN\\1001 -NAna García López");
        assert!(db.connectors.len() >= 8);
    }

    #[test]
    fn each_budget_line_gets_its_own_treatment() {
        let mut db = Db::sample(d());
        let pid = db.patients[3].id;
        db.entries.retain(|e| !(e.patient_id == pid && e.state == TxState::Previsto));
        db.add_entry(pid, "0201", vec![], vec![], TxState::Previsto, 1, d());
        db.add_entry(pid, "0201", vec![], vec![], TxState::Previsto, 1, d());
        let bid = db.budget_from_planned(pid).unwrap();
        // como un presupuesto escrito a mano: sin tratamientos previstos detrás
        db.entries.retain(|e| e.budget_id != Some(bid));
        let total = db.budgets.iter().find(|b| b.id == bid).unwrap().total();
        db.accept_budget(bid, d());
        let linked: Vec<f64> = db.entries.iter().filter(|e| e.budget_id == Some(bid)).map(|e| e.amount()).collect();
        assert_eq!(linked.len(), 2, "dos líneas, dos tratamientos");
        assert!((linked.iter().sum::<f64>() - total).abs() < 0.01, "el total del presupuesto");
        // y si había un previsto igual, se aprovecha uno y se crea el otro
        let mut db2 = Db::sample(d());
        db2.entries.retain(|e| !(e.patient_id == pid && e.state == TxState::Previsto));
        db2.add_entry(pid, "0201", vec![], vec![], TxState::Previsto, 1, d());
        db2.add_entry(pid, "0201", vec![], vec![], TxState::Previsto, 1, d());
        let bid2 = db2.budget_from_planned(pid).unwrap();
        let keep = db2.entries.iter().find(|e| e.budget_id == Some(bid2)).unwrap().id;
        db2.entries.retain(|e| e.budget_id != Some(bid2) || e.id == keep);
        db2.entries.iter_mut().find(|e| e.id == keep).unwrap().budget_id = None;
        db2.accept_budget(bid2, d());
        let ids: Vec<u32> = db2.entries.iter().filter(|e| e.budget_id == Some(bid2)).map(|e| e.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&keep));
    }

    #[test]
    fn receipts_are_unique_per_station() {
        let mut db = Db::sample(d());
        // un solo ordenador: el contador de la configuración
        let next = db.config.next_receipt;
        let id = db.add_payment(100, 10.0, "Efectivo", "");
        assert_eq!(db.payments.iter().find(|p| p.id == id).unwrap().receipt_no, format!("{}-{next:05}", db.config.receipt_series));
        // en red, cada puesto su serie: nunca coinciden
        let mut a = db.clone();
        a.station = 2;
        let mut b = db.clone();
        b.station = 3;
        let ra: Vec<String> = (0..2)
            .map(|_| {
                let id = a.add_payment(100, 10.0, "Efectivo", "");
                a.payments.iter().find(|p| p.id == id).unwrap().receipt_no.clone()
            })
            .collect();
        let id = b.add_payment(101, 10.0, "Tarjeta", "");
        let rb = b.payments.iter().find(|p| p.id == id).unwrap().receipt_no.clone();
        assert_eq!(ra, vec![format!("{}2-00001", db.config.receipt_series), format!("{}2-00002", db.config.receipt_series)]);
        assert_eq!(rb, format!("{}3-00001", db.config.receipt_series));
    }

    #[test]
    fn repeated_history_numbers_get_the_next_free_one() {
        let mut db = Db::sample(d());
        let mut p = db.patients[0].clone();
        p.id = db.new_id();
        p.history_no = db.patients[1].history_no;
        db.patients.push(p.clone());
        let keep_old = db.patients[1].id;
        assert_eq!(db.fix_history_numbers(|id| id != p.id), 1);
        assert_eq!(db.patient(keep_old).unwrap().history_no, db.patients[1].history_no, "el que ya estaba lo conserva");
        let mut all: Vec<u32> = db.patients.iter().map(|x| x.history_no).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), db.patients.len());
        assert_eq!(db.fix_history_numbers(|_| false), 0, "sin repetidos no cambia nada");
    }

    #[test]
    fn patient_data_cannot_inject_program_options() {
        let mut db = Db::sample(d());
        // nombre escrito por alguien en la cita online, con opciones y rutas
        let p = db.patient_mut(100).unwrap();
        p.first_name = "Ana\" -P\\otro /DELETE".into();
        p.last_name = "García\r\nNumPac=999 --abrir C:\\x.dcm".into();
        p.nif = "12345678Z \"/x".into();
        let args = db.expand_args("-P\\GESDEN\\[NUMPAC] -N[NOMPAC] [APELPAC] [NIF]", 100);
        assert_eq!(args, "-P\\GESDEN\\1001 -NAna P otro DELETE García NumPac 999 abrir C x.dcm 12345678Z x");
        // ninguna palabra que venga del paciente empieza como una opción
        for w in args.split_whitespace().skip(1) {
            assert!(!w.starts_with('-') || w.starts_with("-NAna"), "opción colada: {w}");
            assert!(!w.contains(['/', '\\', '"', ':']), "carácter peligroso en {w}");
        }
        // los nombres normales no cambian: guiones dentro, apóstrofos, acentos
        let p = db.patient_mut(100).unwrap();
        p.first_name = "María-José".into();
        p.last_name = "O'Neill Núñez".into();
        assert_eq!(db.expand_args("[NOMPAC] [APELPAC]", 100), "María-José O'Neill Núñez");
        // el fichero puente no admite líneas nuevas metidas en un dato
        let p = db.patient_mut(100).unwrap();
        p.address = "C/ Mayor 3\r\nNumPac=999".into();
        let ini = db.bridge_ini(100);
        assert_eq!(ini.matches("NumPac=").count(), 1, "{ini}");
        assert!(ini.contains("Direccion=C/ MAYOR 3 NUMPAC=999\r\n"));
    }

    #[test]
    fn perio_stats() {
        let db = Db::sample(d());
        let charts = db.patient_perio(108);
        assert_eq!(charts.len(), 2);
        let (a, b) = (charts[0].stats(), charts[1].stats());
        assert!(b.mean_depth < a.mean_depth);
        assert_eq!(a.sites, 30 * 6);
    }

    #[test]
    fn money_format() {
        assert_eq!(money(1234.5), "1.234,50 €");
        assert_eq!(money(-12.0), "-12,00 €");
    }

    #[test]
    fn add_months_clamps() {
        let x = add_months(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(), 1);
        assert_eq!(x, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }
}

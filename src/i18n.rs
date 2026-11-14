//! Idiomas (T-F5-11): castellano, catalán, euskera, gallego e inglés.
//!
//! - **Catálogo:** la clave es el texto en castellano, y lo que falta en un
//!   idioma sale en castellano (alternativa predeterminada). Así un texto nuevo
//!   nunca se queda sin mostrar.
//! - **Qué se traduce:**
//!   - la navegación de la interfaz del personal (menú y pestañas de la ficha);
//!   - los avisos a pacientes de las plantillas de siempre (si la clínica
//!     cambia el texto, sale el suyo: no se traduce lo que no se conoce);
//!   - las páginas de «Mi espacio», de la encuesta y del pago.
//! - **Fechas e importes:** con el formato de cada idioma ([`long_date`],
//!   [`money`]).
//! - **Comodines** (`[NOMBRE]`…): los mismos en todas las traducciones
//!   (comprobado en las pruebas).
//! - **Lo clínico y lo legal** (consentimientos, cuestionarios): no se traduce
//!   aquí. Necesita una traducción validada (B-17, T-F5-11X); hasta entonces,
//!   en castellano.
//! - **Revisión pendiente:** las traducciones de este catálogo las revisa un
//!   traductor nativo de cada idioma ([`REVIEWED`]).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// Las traducciones aún no las ha revisado un traductor nativo (T-F5-11X).
pub const REVIEWED: bool = false;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Es,
    Ca,
    Eu,
    Gl,
    En,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::Es, Lang::Ca, Lang::Eu, Lang::Gl, Lang::En];

    fn index(self) -> usize {
        self as usize
    }

    /// El nombre del idioma en el propio idioma.
    pub fn name(self) -> &'static str {
        ["Castellano", "Català", "Euskara", "Galego", "English"][self.index()]
    }

    pub fn code(self) -> &'static str {
        ["es", "ca", "eu", "gl", "en"][self.index()]
    }

    pub fn from_code(c: &str) -> Option<Lang> {
        let c = c.trim().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| c == l.code() || c.starts_with(&format!("{}-", l.code())))
    }

    /// El primero que se entienda de una cabecera `Accept-Language`.
    pub fn from_accept(h: &str) -> Option<Lang> {
        h.split(',').filter_map(|x| Lang::from_code(x.split(';').next().unwrap_or(""))).next()
    }
}

/// (castellano, [catalán, euskera, gallego, inglés]). Vacío: sin traducir.
pub const CATALOG: &[(&str, [&str; 4])] = &[
    // ── navegación ──
    ("Inicio", ["Inici", "Hasiera", "Inicio", "Home"]),
    ("Agenda", ["Agenda", "Agenda", "Axenda", "Calendar"]),
    ("Pacientes", ["Pacients", "Pazienteak", "Pacientes", "Patients"]),
    ("Presupuestos", ["Pressupostos", "Aurrekontuak", "Orzamentos", "Estimates"]),
    ("Cobros", ["Cobraments", "Kobrantzak", "Cobros", "Payments"]),
    ("Facturas", ["Factures", "Fakturak", "Facturas", "Invoices"]),
    ("Caja", ["Caixa", "Kutxa", "Caixa", "Cash desk"]),
    ("Recalls", ["Revisions", "Berrikuspenak", "Revisións", "Recalls"]),
    ("Comunicaciones", ["Comunicacions", "Komunikazioak", "Comunicacións", "Messages"]),
    ("Listín telefónico", ["Agenda telefònica", "Telefono-zerrenda", "Axenda telefónica", "Phone book"]),
    ("Imágenes y enlaces", ["Imatges i enllaços", "Irudiak eta estekak", "Imaxes e ligazóns", "Images and links"]),
    ("Almacén", ["Magatzem", "Biltegia", "Almacén", "Stock"]),
    ("Laboratorio", ["Laboratori", "Laborategia", "Laboratorio", "Laboratory"]),
    ("Ortodoncia", ["Ortodòncia", "Ortodontzia", "Ortodoncia", "Orthodontics"]),
    ("Gastos", ["Despeses", "Gastuak", "Gastos", "Expenses"]),
    ("Bancos", ["Bancs", "Bankuak", "Bancos", "Banks"]),
    ("Tarifas y tratamientos", ["Tarifes i tractaments", "Tarifak eta tratamenduak", "Tarifas e tratamentos", "Fees and treatments"]),
    ("Colaboradores", ["Col·laboradors", "Laguntzaileak", "Colaboradores", "Staff"]),
    ("Informes", ["Informes", "Txostenak", "Informes", "Reports"]),
    ("Configuración", ["Configuració", "Konfigurazioa", "Configuración", "Settings"]),
    ("Más módulos", ["Més mòduls", "Modulu gehiago", "Máis módulos", "More modules"]),
    ("Clínica", ["Clínica", "Klinika", "Clínica", "Clinic"]),
    ("Economía", ["Economia", "Ekonomia", "Economía", "Finance"]),
    ("Gestión", ["Gestió", "Kudeaketa", "Xestión", "Management"]),
    ("Sistema", ["Sistema", "Sistema", "Sistema", "System"]),
    ("Gestión de clínica dental", ["Gestió de clínica dental", "Hortz-klinikaren kudeaketa", "Xestión de clínica dental", "Dental practice management"]),
    // ── pestañas de la ficha ──
    ("Resumen", ["Resum", "Laburpena", "Resumo", "Summary"]),
    ("Filiación", ["Filiació", "Datu pertsonalak", "Filiación", "Personal details"]),
    ("Salud", ["Salut", "Osasuna", "Saúde", "Health"]),
    ("Odontograma", ["Odontograma", "Odontograma", "Odontograma", "Dental chart"]),
    ("Periodontograma", ["Periodontograma", "Periodontograma", "Periodontograma", "Periodontal chart"]),
    ("Imágenes", ["Imatges", "Irudiak", "Imaxes", "Images"]),
    ("Documentos", ["Documents", "Dokumentuak", "Documentos", "Documents"]),
    ("Historial", ["Historial", "Historia", "Historial", "History"]),
    ("Cuenta", ["Compte", "Kontua", "Conta", "Account"]),
    ("Citas", ["Cites", "Hitzorduak", "Citas", "Appointments"]),
    // ── avisos a pacientes (plantillas de siempre) ──
    (
        "Hola [NOMBRE], le recordamos su cita en [CLINICA] el [FECHA] a las [HORA] con [DOCTOR]. Si no puede venir, llámenos al [TELCLINICA]. ¡Gracias!",
        [
            "Hola [NOMBRE], us recordem la cita a [CLINICA] el [FECHA] a les [HORA] amb [DOCTOR]. Si no podeu venir, truqueu-nos al [TELCLINICA]. Gràcies!",
            "Kaixo [NOMBRE]: zure hitzordua gogorarazten dizugu. Klinika: [CLINICA]. Eguna: [FECHA], [HORA]. Profesionala: [DOCTOR]. Ezin baduzu etorri, deitu [TELCLINICA] zenbakira. Eskerrik asko!",
            "Ola [NOMBRE], lembrámoslle a súa cita en [CLINICA] o [FECHA] ás [HORA] con [DOCTOR]. Se non pode vir, chámenos ao [TELCLINICA]. Grazas!",
            "Hello [NOMBRE], this is a reminder of your appointment at [CLINICA] on [FECHA] at [HORA] with [DOCTOR]. If you cannot come, please call us on [TELCLINICA]. Thank you!",
        ],
    ),
    (
        "[CLINICA]: cita el [FECHA] a las [HORA]. Para cambiarla llame al [TELCLINICA].",
        [
            "[CLINICA]: cita el [FECHA] a les [HORA]. Per canviar-la truqueu al [TELCLINICA].",
            "[CLINICA]: hitzordua [FECHA], [HORA]. Aldatzeko deitu [TELCLINICA] zenbakira.",
            "[CLINICA]: cita o [FECHA] ás [HORA]. Para cambiala chame ao [TELCLINICA].",
            "[CLINICA]: appointment on [FECHA] at [HORA]. To change it, call [TELCLINICA].",
        ],
    ),
    (
        "Hola [NOMBRE], ya le toca su [MOTIVO] en [CLINICA]. Llámenos al [TELCLINICA] o responda a este mensaje para darle cita.",
        [
            "Hola [NOMBRE], ja us toca la [MOTIVO] a [CLINICA]. Truqueu-nos al [TELCLINICA] o responeu aquest missatge i us donarem cita.",
            "Kaixo [NOMBRE]: [MOTIVO] egiteko garaia da. Klinika: [CLINICA]. Deitu [TELCLINICA] zenbakira edo erantzun mezu honi hitzordua emateko.",
            "Ola [NOMBRE], xa lle toca a súa [MOTIVO] en [CLINICA]. Chámenos ao [TELCLINICA] ou responda a esta mensaxe e dámoslle cita.",
            "Hello [NOMBRE], your [MOTIVO] at [CLINICA] is now due. Call us on [TELCLINICA] or reply to this message to book an appointment.",
        ],
    ),
    (
        "¡Feliz cumpleaños, [NOMBRE]! Todo el equipo de [CLINICA] le desea un gran día.",
        [
            "Per molts anys, [NOMBRE]! Tot l'equip de [CLINICA] us desitja un gran dia.",
            "Zorionak, [NOMBRE]! [CLINICA] taldeak egun zoriontsua opa dizu.",
            "Feliz aniversario, [NOMBRE]! Todo o equipo de [CLINICA] deséxalle un gran día.",
            "Happy birthday, [NOMBRE]! Everyone at [CLINICA] wishes you a great day.",
        ],
    ),
    ("Su presupuesto en [CLINICA]", ["El vostre pressupost a [CLINICA]", "Zure aurrekontua ([CLINICA])", "O seu orzamento en [CLINICA]", "Your estimate at [CLINICA]"]),
    (
        "Estimado/a [NOMBRE]:\n\nLe recordamos que tiene pendiente el presupuesto «[MOTIVO]» por importe de [IMPORTE]. Si tiene cualquier duda, estaremos encantados de atenderle en el [TELCLINICA].\n\nUn saludo,\n[CLINICA]",
        [
            "Benvolgut/da [NOMBRE]:\n\nUs recordem que teniu pendent el pressupost «[MOTIVO]» per un import de [IMPORTE]. Si teniu qualsevol dubte, us atendrem amb molt de gust al [TELCLINICA].\n\nSalutacions,\n[CLINICA]",
            "[NOMBRE] agurgarria:\n\n«[MOTIVO]» aurrekontua onartzeke duzula gogorarazten dizugu. Zenbatekoa: [IMPORTE]. Zalantzarik baduzu, pozik lagunduko dizugu [TELCLINICA] zenbakian.\n\nAgur bero bat,\n[CLINICA]",
            "Estimado/a [NOMBRE]:\n\nLembrámoslle que ten pendente o orzamento «[MOTIVO]» por importe de [IMPORTE]. Se ten calquera dúbida, atenderémolo con moito gusto no [TELCLINICA].\n\nUn saúdo,\n[CLINICA]",
            "Dear [NOMBRE],\n\nThis is a reminder that your estimate «[MOTIVO]» for [IMPORTE] is still pending. If you have any questions, we will be happy to help on [TELCLINICA].\n\nKind regards,\n[CLINICA]",
        ],
    ),
    (
        "Recordatorio: su cita en [CLINICA] el [FECHA] a las [HORA]",
        [
            "Recordatori: la vostra cita a [CLINICA] el [FECHA] a les [HORA]",
            "Oroigarria: zure hitzordua ([CLINICA]), [FECHA], [HORA]",
            "Recordatorio: a súa cita en [CLINICA] o [FECHA] ás [HORA]",
            "Reminder: your appointment at [CLINICA] on [FECHA] at [HORA]",
        ],
    ),
    (
        "Hola, [NOMBRE]:\n\nLe recordamos su cita en [CLINICA] el [FECHA] a las [HORA] con [DOCTOR].\n\nSi no puede venir, avísenos llamando al [TELCLINICA].\n\nUn saludo,\n[CLINICA]",
        [
            "Hola, [NOMBRE]:\n\nUs recordem la cita a [CLINICA] el [FECHA] a les [HORA] amb [DOCTOR].\n\nSi no podeu venir, aviseu-nos trucant al [TELCLINICA].\n\nSalutacions,\n[CLINICA]",
            "Kaixo, [NOMBRE]:\n\nZure hitzordua gogorarazten dizugu. Klinika: [CLINICA]. Eguna: [FECHA], [HORA]. Profesionala: [DOCTOR].\n\nEzin baduzu etorri, abisatu [TELCLINICA] zenbakira deituta.\n\nAgur bero bat,\n[CLINICA]",
            "Ola, [NOMBRE]:\n\nLembrámoslle a súa cita en [CLINICA] o [FECHA] ás [HORA] con [DOCTOR].\n\nSe non pode vir, avísenos chamando ao [TELCLINICA].\n\nUn saúdo,\n[CLINICA]",
            "Hello [NOMBRE],\n\nThis is a reminder of your appointment at [CLINICA] on [FECHA] at [HORA] with [DOCTOR].\n\nIf you cannot come, please let us know by calling [TELCLINICA].\n\nKind regards,\n[CLINICA]",
        ],
    ),
    ("Le toca su [MOTIVO] en [CLINICA]", ["Us toca la [MOTIVO] a [CLINICA]", "[MOTIVO] egiteko garaia ([CLINICA])", "Tócalle a súa [MOTIVO] en [CLINICA]", "Your [MOTIVO] at [CLINICA] is due"]),
    (
        "Hola, [NOMBRE]:\n\nYa le toca su [MOTIVO]. Llámenos al [TELCLINICA] y le damos cita.\n\nUn saludo,\n[CLINICA]",
        [
            "Hola, [NOMBRE]:\n\nJa us toca la [MOTIVO]. Truqueu-nos al [TELCLINICA] i us donarem cita.\n\nSalutacions,\n[CLINICA]",
            "Kaixo, [NOMBRE]:\n\n[MOTIVO] egiteko garaia da. Deitu [TELCLINICA] zenbakira eta hitzordua emango dizugu.\n\nAgur bero bat,\n[CLINICA]",
            "Ola, [NOMBRE]:\n\nXa lle toca a súa [MOTIVO]. Chámenos ao [TELCLINICA] e dámoslle cita.\n\nUn saúdo,\n[CLINICA]",
            "Hello [NOMBRE],\n\nYour [MOTIVO] is now due. Call us on [TELCLINICA] and we will give you an appointment.\n\nKind regards,\n[CLINICA]",
        ],
    ),
    ("¿Qué tal su visita a [CLINICA]?", ["Què tal la visita a [CLINICA]?", "Zer moduz joan zen zure bisita ([CLINICA])?", "Que tal a súa visita a [CLINICA]?", "How was your visit to [CLINICA]?"]),
    (
        "Hola [NOMBRE], gracias por venir a [CLINICA]. ¿Nos cuenta en un minuto cómo fue su visita? [ENLACE_ENCUESTA]",
        [
            "Hola [NOMBRE], gràcies per venir a [CLINICA]. Ens expliqueu en un minut com va anar la visita? [ENLACE_ENCUESTA]",
            "Kaixo [NOMBRE], eskerrik asko etortzeagatik ([CLINICA]). Minutu batean kontatuko diguzu nola joan zen bisita? [ENLACE_ENCUESTA]",
            "Ola [NOMBRE], grazas por vir a [CLINICA]. Cóntanos nun minuto como foi a súa visita? [ENLACE_ENCUESTA]",
            "Hello [NOMBRE], thank you for visiting [CLINICA]. Could you take a minute to tell us how your visit went? [ENLACE_ENCUESTA]",
        ],
    ),
    ("Confirme, cambie o anule su cita aquí", ["Confirmeu, canvieu o anul·leu la cita aquí", "Berretsi, aldatu edo ezeztatu zure hitzordua hemen", "Confirme, cambie ou anule a súa cita aquí", "Confirm, change or cancel your appointment here"]),
    ("Pida su cita por internet", ["Demaneu cita per internet", "Eskatu hitzordua internetez", "Pida a súa cita por internet", "Book your appointment online"]),
    (
        "Revise su presupuesto y acéptelo con su firma desde el móvil",
        [
            "Reviseu el pressupost i accepteu-lo amb la vostra signatura des del mòbil",
            "Berrikusi zure aurrekontua eta onartu sinadurarekin mugikorretik",
            "Revise o seu orzamento e acépteo coa súa sinatura desde o móbil",
            "Review your estimate and accept it with your signature from your phone",
        ],
    ),
    ("Cuéntenos cómo fue su visita", ["Expliqueu-nos com va anar la visita", "Kontaiguzu nola joan zen zure bisita", "Cóntenos como foi a súa visita", "Tell us how your visit went"]),
    (
        "Si no quiere recibir estos mensajes, respóndanos con la palabra BAJA o llámenos al [TELCLINICA].",
        [
            "Si no voleu rebre aquests missatges, responeu-nos amb la paraula BAIXA o truqueu-nos al [TELCLINICA].",
            "Mezu hauek jaso nahi ez badituzu, erantzun BAJA hitzarekin edo deitu [TELCLINICA] zenbakira.",
            "Se non quere recibir estas mensaxes, respóndanos coa palabra BAIXA ou chámenos ao [TELCLINICA].",
            "If you do not want to receive these messages, reply with the word BAJA or call us on [TELCLINICA].",
        ],
    ),
    // ── páginas del paciente ──
    ("Mi espacio", ["El meu espai", "Nire gunea", "O meu espazo", "My area"]),
    (
        "Para entrar, escriba su DNI/NIE y su fecha de nacimiento (si entra por un menor a su cargo, su DNI/NIE y la fecha de nacimiento del menor).",
        [
            "Per entrar, escriviu el vostre DNI/NIE i la data de naixement (si entreu per un menor al vostre càrrec, el vostre DNI/NIE i la data de naixement del menor).",
            "Sartzeko, idatzi zure NAN/AIZ eta jaiotze-data (zure ardurapeko adingabe baten izenean sartzen bazara, zure NAN/AIZ eta adingabearen jaiotze-data).",
            "Para entrar, escriba o seu DNI/NIE e a súa data de nacemento (se entra por un menor ao seu cargo, o seu DNI/NIE e a data de nacemento do menor).",
            "To sign in, enter your ID number (DNI/NIE) and your date of birth (if you sign in for a minor in your care, your ID number and the minor's date of birth).",
        ],
    ),
    ("DNI/NIE", ["DNI/NIE", "NAN/AIZ", "DNI/NIE", "ID number (DNI/NIE)"]),
    ("Fecha de nacimiento (dd/mm/aaaa)", ["Data de naixement (dd/mm/aaaa)", "Jaiotze-data (ee/hh/uuuu)", "Data de nacemento (dd/mm/aaaa)", "Date of birth (dd/mm/yyyy)"]),
    ("Entrar", ["Entrar", "Sartu", "Entrar", "Sign in"]),
    ("Salir", ["Sortir", "Irten", "Saír", "Sign out"]),
    ("Los datos no coinciden con los de la ficha.", ["Les dades no coincideixen amb les de la fitxa.", "Datuak ez datoz bat fitxakoekin.", "Os datos non coinciden cos da ficha.", "The details do not match our records."]),
    (
        "Por seguridad, este enlace está bloqueado media hora por demasiados intentos.",
        [
            "Per seguretat, aquest enllaç està bloquejat mitja hora per massa intents.",
            "Segurtasunagatik, esteka hau ordu erdiz blokeatuta dago, saiakera gehiegi egin direlako.",
            "Por seguridade, esta ligazón está bloqueada media hora por demasiados intentos.",
            "For security, this link is locked for half an hour after too many attempts.",
        ],
    ),
    ("Próximas citas", ["Properes cites", "Hurrengo hitzorduak", "Próximas citas", "Upcoming appointments"]),
    ("Ninguna", ["Cap", "Bat ere ez", "Ningunha", "None"]),
    ("confirmar o cambiar", ["confirmar o canviar", "baieztatu edo aldatu", "confirmar ou cambiar", "confirm or change"]),
    ("ver y firmar", ["veure i signar", "ikusi eta sinatu", "ver e asinar", "view and sign"]),
    ("Documentos firmados", ["Documents signats", "Sinatutako dokumentuak", "Documentos asinados", "Signed documents"]),
    ("Mis recetas", ["Les meves receptes", "Nire errezetak", "As miñas receitas", "My prescriptions"]),
    ("Factura", ["Factura", "Faktura", "Factura", "Invoice"]),
    ("Total", ["Total", "Guztira", "Total", "Total"]),
    ("Firmado", ["Signat", "Sinatuta", "Asinado", "Signed"]),
    (
        "Nos ayuda mucho saber cómo fue su visita.",
        ["Ens ajuda molt saber com va anar la visita.", "Asko laguntzen digu zure bisita nola joan zen jakiteak.", "Axúdanos moito saber como foi a súa visita.", "It helps us a lot to know how your visit went."],
    ),
    (
        "Del 0 al 10, ¿con qué probabilidad recomendaría [CLINICA] a un familiar o amigo?",
        [
            "Del 0 al 10, amb quina probabilitat recomanaríeu [CLINICA] a un familiar o amic?",
            "0tik 10era, zenbateko probabilitatez gomendatuko zenioke [CLINICA] senide edo lagun bati?",
            "Do 0 ao 10, con que probabilidade recomendaría [CLINICA] a un familiar ou amigo?",
            "From 0 to 10, how likely are you to recommend [CLINICA] to a friend or relative?",
        ],
    ),
    ("¿Algo que quiera contarnos? (opcional)", ["Alguna cosa que ens vulgueu explicar? (opcional)", "Zerbait kontatu nahi diguzu? (aukerakoa)", "Algo que nos queira contar? (opcional)", "Anything you would like to tell us? (optional)"]),
    ("Enviar", ["Enviar", "Bidali", "Enviar", "Send"]),
    ("No quiero recibir más encuestas", ["No vull rebre més enquestes", "Ez dut inkesta gehiagorik jaso nahi", "Non quero recibir máis enquisas", "I do not want to receive more surveys"]),
    ("Gracias por responder.", ["Gràcies per respondre.", "Eskerrik asko erantzuteagatik.", "Grazas por responder.", "Thank you for your answer."]),
    (
        "Si quiere, también puede contar su experiencia en una reseña pública:",
        [
            "Si voleu, també podeu explicar la vostra experiència en una ressenya pública:",
            "Nahi baduzu, zure esperientzia iritzi publiko batean ere konta dezakezu:",
            "Se quere, tamén pode contar a súa experiencia nunha recensión pública:",
            "If you wish, you can also share your experience in a public review:",
        ],
    ),
    ("dejar una reseña", ["deixar una ressenya", "iritzia utzi", "deixar unha recensión", "leave a review"]),
    ("Hola", ["Hola", "Kaixo", "Ola", "Hello"]),
];

/// La traducción de `es` (o el propio `es` si falta).
pub fn t(lang: Lang, es: &'static str) -> &'static str {
    if lang == Lang::Es {
        return es;
    }
    CATALOG.iter().find(|(k, _)| *k == es).map(|(_, v)| v[lang.index() - 1]).filter(|x| !x.is_empty()).unwrap_or(es)
}

/// Igual que [`t`] para un texto que no es fijo (una plantilla): la
/// traducción si es uno de los textos conocidos; si no, el mismo texto.
pub fn tr(lang: Lang, es: &str) -> String {
    if lang == Lang::Es {
        return es.to_string();
    }
    CATALOG.iter().find(|(k, _)| *k == es).map(|(_, v)| v[lang.index() - 1]).filter(|x| !x.is_empty()).unwrap_or(es).to_string()
}

const WEEKDAYS: [[&str; 7]; 5] = [
    ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"],
    ["dilluns", "dimarts", "dimecres", "dijous", "divendres", "dissabte", "diumenge"],
    ["astelehena", "asteartea", "asteazkena", "osteguna", "ostirala", "larunbata", "igandea"],
    ["luns", "martes", "mércores", "xoves", "venres", "sábado", "domingo"],
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
];

const MONTHS: [[&str; 12]; 5] = [
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
    ["gener", "febrer", "març", "abril", "maig", "juny", "juliol", "agost", "setembre", "octubre", "novembre", "desembre"],
    ["urtarrila", "otsaila", "martxoa", "apirila", "maiatza", "ekaina", "uztaila", "abuztua", "iraila", "urria", "azaroa", "abendua"],
    ["xaneiro", "febreiro", "marzo", "abril", "maio", "xuño", "xullo", "agosto", "setembro", "outubro", "novembro", "decembro"],
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
];

/// «sábado, 14 de noviembre» en cada idioma.
pub fn long_date(lang: Lang, d: NaiveDate) -> String {
    let w = WEEKDAYS[lang.index()][d.weekday().num_days_from_monday() as usize];
    let m = MONTHS[lang.index()][d.month0() as usize];
    match lang {
        Lang::Es | Lang::Gl => format!("{w}, {} de {m}", d.day()),
        // «de novembre», pero «d'octubre» y «d'abril», «d'agost»
        Lang::Ca => format!("{w}, {} {m}", d.day()).replacen(&format!("{} {m}", d.day()), &format!("{} {}{m}", d.day(), if m.starts_with(['a', 'o']) { "d'" } else { "de " }), 1),
        // genitivo: «azaroa» → «azaroaren»
        Lang::Eu => format!("{w}, {m}ren {}a", d.day()),
        Lang::En => format!("{w}, {} {m}", d.day()),
    }
}

/// «sábado 14/11» (para los avisos).
pub fn short_date(lang: Lang, d: NaiveDate) -> String {
    let w = WEEKDAYS[lang.index()][d.weekday().num_days_from_monday() as usize];
    match lang {
        Lang::En => format!("{w} {}", d.format("%d/%m")),
        _ => format!("{w} {}", d.format("%d/%m")),
    }
}

/// Importe en euros: «1.234,50 €» (y «€1,234.50» en inglés).
pub fn money(lang: Lang, cents: i64) -> String {
    let neg = cents < 0;
    let c = cents.unsigned_abs();
    let (eur, dec) = (c / 100, c % 100);
    let digits = eur.to_string();
    let sep = if lang == Lang::En { ',' } else { '.' };
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(sep);
        }
        grouped.push(ch);
    }
    let sign = if neg { "-" } else { "" };
    match lang {
        Lang::En => format!("{sign}€{grouped}.{dec:02}"),
        _ => format!("{sign}{grouped},{dec:02} €"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s.split('[').skip(1).filter_map(|x| x.split_once(']').map(|(a, _)| format!("[{a}]"))).collect();
        v.sort();
        v
    }

    /// Cobertura: todas las claves en los cinco idiomas, sin repetir, con los
    /// mismos comodines y sin quedarse en castellano por error.
    #[test]
    fn the_catalog_is_complete_with_the_same_placeholders() {
        let mut keys: Vec<&str> = CATALOG.iter().map(|(k, _)| *k).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n, "claves repetidas");
        for (es, tr) in CATALOG {
            for (i, x) in tr.iter().enumerate() {
                let lang = Lang::ALL[i + 1];
                assert!(!x.is_empty(), "falta {lang:?}: {es}");
                assert_eq!(placeholders(x), placeholders(es), "comodines distintos en {lang:?}: {es}");
            }
        }
        // las de la navegación (botones y pestañas) existen todas
        for (_, _, name) in crate::app::Screen::NAV {
            assert!(CATALOG.iter().any(|(k, _)| *k == name), "sin traducir: {name}");
        }
        for (_, name) in crate::app::PatTab::ALL {
            assert!(CATALOG.iter().any(|(k, _)| *k == name), "sin traducir: {name}");
        }
        // y las plantillas de aviso de siempre
        for t in crate::comms::default_msg_templates() {
            for x in [t.text.as_str(), t.subject.as_str()].into_iter().filter(|x| !x.is_empty()) {
                assert!(CATALOG.iter().any(|(k, _)| *k == x), "plantilla sin traducir: {x}");
            }
        }
    }

    /// Cada texto que el código pasa a `t()` está en el catálogo (si no,
    /// saldría siempre en castellano sin que nadie lo notara).
    #[test]
    fn every_text_used_with_t_is_in_the_catalog() {
        let mut code = String::new();
        let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                    code.push_str(&std::fs::read_to_string(&p).unwrap());
                }
            }
        }
        let mut used = 0;
        let mut missing = vec![];
        for pat in ["t(lang, \"", "i18n::t(self.db.config.ui_lang, \""] {
            for part in code.split(pat).skip(1) {
                let lit = part.split('"').next().unwrap_or("");
                used += 1;
                if !CATALOG.iter().any(|(k, _)| *k == lit) {
                    missing.push(lit.to_string());
                }
            }
        }
        assert!(used > 20, "textos con t(): {used}");
        assert!(missing.is_empty(), "no están en el catálogo: {missing:#?}");
    }

    #[test]
    fn missing_text_falls_back_to_spanish_and_formats_follow_the_language() {
        assert_eq!(t(Lang::Ca, "Agenda"), "Agenda");
        assert_eq!(t(Lang::En, "Pacientes"), "Patients");
        assert_eq!(t(Lang::Eu, "Texto que no está en el catálogo"), "Texto que no está en el catálogo");
        assert_eq!(tr(Lang::Gl, "Texto propio de la clínica"), "Texto propio de la clínica", "lo que no se conoce no se traduce");
        let d = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        assert_eq!(long_date(Lang::Es, d), "sábado, 14 de noviembre");
        assert_eq!(long_date(Lang::Ca, d), "dissabte, 14 de novembre");
        assert_eq!(long_date(Lang::Ca, NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()), "divendres, 2 d'octubre");
        assert_eq!(long_date(Lang::Gl, d), "sábado, 14 de novembro");
        assert_eq!(long_date(Lang::En, d), "Saturday, 14 November");
        assert_eq!(long_date(Lang::Eu, d), "larunbata, azaroaren 14a");
        assert_eq!(money(Lang::Es, 123_450), "1.234,50 €");
        assert_eq!(money(Lang::En, 123_450), "€1,234.50");
        assert_eq!(money(Lang::Ca, -5), "-0,05 €");
        assert_eq!(Lang::from_accept("en-GB,en;q=0.9,es;q=0.8"), Some(Lang::En));
        assert_eq!(Lang::from_accept("fr-FR,ca;q=0.5"), Some(Lang::Ca));
        assert_eq!(Lang::from_code("de"), None);
        const { assert!(!REVIEWED) };
    }

    /// Avisos en el idioma del paciente (con la fecha en su idioma); un
    /// texto propio de la clínica, tal cual; y un consentimiento solo con una
    /// traducción validada.
    #[test]
    fn messages_and_documents_follow_the_patient_language() {
        use crate::comms::MsgKind;
        use crate::model::*;
        let day = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        let mut db = Db::sample(day);
        db.messages.clear();
        for t in db.msg_templates.iter_mut() {
            t.active = t.kind == MsgKind::RecordatorioCita && t.channel == crate::comms::Channel::WhatsApp;
        }
        let p = db.patients.iter_mut().find(|p| p.id == 101).unwrap();
        (p.lang, p.accepts_gdpr, p.accepts_whatsapp, p.mobile) = (Some(Lang::Ca), true, true, "600000001".into());
        let mut a = db.appointments.iter().find(|a| a.patient_id == Some(101)).unwrap().clone();
        (a.id, a.date, a.state) = (900_001, day + chrono::Duration::days(1), ApptState::Confirmada);
        db.appointments.push(a);
        db.wa_sender.clear();
        db.generate_messages(day);
        let m = db.messages.iter().find(|m| m.patient_id == 101 && m.kind == MsgKind::RecordatorioCita).expect("aviso");
        assert!(m.text.contains("us recordem") && !m.text.contains("le recordamos"), "{}", m.text);
        assert!(WEEKDAYS[1].iter().any(|w| m.text.contains(&format!("{w} "))), "la fecha en catalán: {}", m.text);
        // un texto propio de la clínica no se traduce
        db.messages.clear();
        for a in db.appointments.iter_mut() {
            a.reminder_sent = false;
        }
        let tpl = db.msg_templates.iter_mut().find(|t| t.active).unwrap();
        tpl.text = "Mañana tiene cita en [CLINICA].".into();
        db.generate_messages(day);
        let m = db.messages.iter().find(|m| m.patient_id == 101).expect("aviso");
        assert!(m.text.starts_with("Mañana tiene cita"), "{}", m.text);
        // consentimiento: sin traducción validada, en castellano; con ella, en su idioma
        let orig = db.doc_templates.iter().find(|t| t.id == 2).unwrap().clone();
        let mut ca = orig.clone();
        (ca.id, ca.body, ca.translation_of) = (999, "Consentiment en català [NOMBRE]".into(), Some((2, Lang::Ca)));
        db.doc_templates.push(ca);
        let d = db.new_document(101, 2, &[38]).unwrap();
        assert_eq!(db.documents.iter().find(|x| x.id == d).unwrap().template_id, 2);
        assert!(!db.documents.iter().find(|x| x.id == d).unwrap().body.contains("català"), "sin validar, no");
        db.doc_templates.iter_mut().find(|t| t.id == 999).unwrap().validated = Some((day, "Traductora jurada".into()));
        let d = db.new_document(101, 2, &[38]).unwrap();
        let doc = db.documents.iter().find(|x| x.id == d).unwrap();
        assert!(doc.body.contains("Consentiment en català") && doc.template_id == 2, "validada, en su idioma (y de la misma plantilla)");
        // las páginas del paciente
        let page = crate::encuestas::survey_page("Clínica", &"a".repeat(32), &crate::encuestas::PubSurvey { lang: Lang::Eu, first_name: "Ane".into(), answered: false, review_url: String::new() }, false);
        assert!(page.contains("lang=\"eu\"") && page.contains("Bidali") && page.contains("Kaixo"));
    }
}

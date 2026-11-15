//! The active keyboard layout: KDE Plasma over D-Bus, else XKB_DEFAULT_LAYOUT, else "us".
pub fn current() -> (String, String) {
    kde().unwrap_or_else(|| {
        let layout = std::env::var("XKB_DEFAULT_LAYOUT").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "us".into());
        let first = layout.split(',').next().unwrap_or("us").to_string();
        (first, std::env::var("XKB_DEFAULT_VARIANT").unwrap_or_default().split(',').next().unwrap_or("").to_string())
    })
}

fn kde() -> Option<(String, String)> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let call = |method: &str| {
        conn.call_method(Some("org.kde.keyboard"), "/Layouts", Some("org.kde.KeyboardLayouts"), method, &())
    };
    let index: u32 = call("getLayout").ok()?.body().deserialize().ok()?;
    let list: Vec<(String, String, String)> = call("getLayoutsList").ok()?.body().deserialize().ok()?;
    let (short, variant, _) = list.get(index as usize)?.clone();
    Some((short, variant))
}

//! Can we read keyboards and write the virtual keyboard? If not, say exactly how to fix it.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PermissionReport {
    pub input_ok: bool,
    pub uinput_ok: bool,
    pub fix: Option<String>,
}

pub fn report(input_readable: bool, uinput_writable: bool) -> PermissionReport {
    let fix = if input_readable && uinput_writable {
        None
    } else {
        Some(
            "Run: sudo usermod -aG input $USER — then log out and back in. \
(The .deb installs a rule that lets the input group use /dev/uinput.)"
                .to_string(),
        )
    };
    PermissionReport { input_ok: input_readable, uinput_ok: uinput_writable, fix }
}

pub fn check() -> PermissionReport {
    let input_readable = std::fs::read_dir("/dev/input")
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
                .any(|e| std::fs::File::open(e.path()).is_ok())
        })
        .unwrap_or(false);
    let uinput_writable = std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_ok();
    report(input_readable, uinput_writable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_report_lists_fixes() {
        assert_eq!(report(true, true).fix, None);
        let r = report(false, true);
        assert!(!r.input_ok);
        assert!(r.fix.unwrap().contains("usermod -aG input"));
        assert!(report(true, false).fix.is_some());
    }
}

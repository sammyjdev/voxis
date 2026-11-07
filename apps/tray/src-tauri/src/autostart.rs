//! "Iniciar com o Windows": a quoted HKCU Run value. Quoted because a
//! profile folder with a space in it ("C:\Users\Joao Silva") would otherwise
//! leave Windows guessing where the program path ends.

/// What the menu needs; a trait so the menu logic is testable without the
/// registry.
pub trait Launcher {
    fn is_enabled(&self) -> Result<bool, String>;
    fn enable(&self) -> Result<(), String>;
    fn disable(&self) -> Result<(), String>;
}

/// The Run value `name` that starts `exe`.
pub struct RunKey {
    name: String,
    exe: std::path::PathBuf,
}

impl RunKey {
    pub fn new(name: &str, exe: &std::path::Path) -> Self {
        Self { name: name.to_owned(), exe: exe.to_path_buf() }
    }

    /// What the Run value holds: the exe path in double quotes.
    pub fn command(&self) -> String {
        format!("\"{}\"", self.exe.display())
    }
}

#[cfg(windows)]
const RUN: windows::core::PCWSTR = windows::core::w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

#[cfg(windows)]
impl Launcher for RunKey {
    fn is_enabled(&self) -> Result<bool, String> {
        use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
        let name = windows::core::HSTRING::from(self.name.as_str());
        // size query only: whether the value exists is all the menu needs
        let mut size = 0u32;
        let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, &name, RRF_RT_REG_SZ, None, None, Some(&mut size)) };
        match rc {
            ERROR_SUCCESS => Ok(true),
            ERROR_FILE_NOT_FOUND => Ok(false),
            e => Err(format!("Run key: {}", windows::core::Error::from(e.to_hresult()).message())),
        }
    }

    fn enable(&self) -> Result<(), String> {
        use windows::Win32::Foundation::ERROR_SUCCESS;
        use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
        let name = windows::core::HSTRING::from(self.name.as_str());
        let data: Vec<u16> = self.command().encode_utf16().chain(std::iter::once(0)).collect();
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN,
                &name,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        };
        if rc == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("Run key: {}", windows::core::Error::from(rc.to_hresult()).message()))
        }
    }

    fn disable(&self) -> Result<(), String> {
        use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
        let name = windows::core::HSTRING::from(self.name.as_str());
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, &name) } {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            e => Err(format!("Run key: {}", windows::core::Error::from(e.to_hresult()).message())),
        }
    }
}

/// Whether the menu item shows checked; an unreadable Run key reads as off.
pub fn menu_checked(l: &dyn Launcher) -> bool {
    l.is_enabled().unwrap_or(false)
}

/// A click on the menu item: flips the Run value, returns the new state.
pub fn toggle(l: &dyn Launcher) -> Result<bool, String> {
    if l.is_enabled()? {
        l.disable()?;
        Ok(false)
    } else {
        l.enable()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fake(Cell<bool>);
    impl Launcher for Fake {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(self.0.get())
        }
        fn enable(&self) -> Result<(), String> {
            self.0.set(true);
            Ok(())
        }
        fn disable(&self) -> Result<(), String> {
            self.0.set(false);
            Ok(())
        }
    }

    #[test]
    fn the_autostart_menu_item_mirrors_the_run_value() {
        let run = Fake(Cell::new(false));
        assert!(!menu_checked(&run));

        let now = toggle(&run).unwrap();

        assert!(now);
        assert!(run.0.get());
        assert!(menu_checked(&run));
        assert!(!toggle(&run).unwrap());
        assert!(!menu_checked(&run));
    }

    /// The Run value `name`, as `reg query` prints it; None when absent.
    #[cfg(windows)]
    fn run_value(name: &str) -> Option<String> {
        let out = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", name])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        out.status.success().then_some(text)
    }

    #[cfg(windows)]
    #[test]
    fn enabling_autostart_writes_the_run_value_and_disabling_removes_it() {
        // a name of its own, so the test never touches a real VOXIS entry
        let name = format!("VOXIS-test-{}", std::process::id());
        let exe = std::path::Path::new(r"C:\Users\Joao Silva\AppData\Local\VOXIS\VOXIS.exe");
        let key = RunKey::new(&name, exe);

        key.enable().unwrap();
        let written = run_value(&name);
        let enabled = key.is_enabled().unwrap();
        key.disable().unwrap();

        let written = written.expect("no Run value after enable");
        assert!(written.contains(&format!("\"{}\"", exe.display())), "{written}");
        assert!(enabled);
        assert!(run_value(&name).is_none(), "Run value left after disable");
        assert!(!key.is_enabled().unwrap());
    }
}

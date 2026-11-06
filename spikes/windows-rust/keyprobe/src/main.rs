//! Prints every key event Windows delivers, before any app sees it, so we can
//! learn what a dedicated key (the Keychron C2 Pro mic key) actually sends.
//! Observes only: every event is passed on. Ctrl+C quits.

use std::time::Instant;
use std::sync::OnceLock;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static START: OnceLock<Instant> = OnceLock::new();

fn vk_name(vk: u32) -> String {
    match vk {
        0x10 => "SHIFT".into(),
        0x11 => "CTRL".into(),
        0x12 => "ALT".into(),
        0x5B => "LWIN".into(),
        0x5C => "RWIN".into(),
        0xA0 => "LSHIFT".into(),
        0xA1 => "RSHIFT".into(),
        0xA2 => "LCTRL".into(),
        0xA3 => "RCTRL".into(),
        0xA4 => "LALT".into(),
        0xA5 => "RALT".into(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        _ => format!("VK_0x{vk:02X}"),
    }
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let dir = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => "down",
            WM_KEYUP | WM_SYSKEYUP => "up  ",
            _ => "?   ",
        };
        let ms = START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0);
        println!(
            "{ms:>8} ms  {dir}  vk=0x{:02X} {:<8} scan=0x{:02X}{}{}",
            k.vkCode,
            vk_name(k.vkCode),
            k.scanCode,
            if k.flags.0 & LLKHF_EXTENDED.0 != 0 { " extended" } else { "" },
            if k.flags.0 & LLKHF_INJECTED.0 != 0 { " injected" } else { "" },
        );
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn main() -> windows::core::Result<()> {
    START.set(Instant::now()).ok();
    unsafe {
        let module = GetModuleHandleW(None)?;
        let _hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(module.into()), 0)?;
        println!("keyprobe: press the key (tap, then hold ~1 s). Ctrl+C quits.");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

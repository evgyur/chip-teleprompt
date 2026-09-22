use std::{mem::size_of, ptr::null};

use windows_sys::{
    core::w,
    Win32::{
        Foundation::{HWND, POINT, RECT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP,
                NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETFOCUS, NIM_SETVERSION, NOTIFYICONDATAW,
                NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
                GetSystemMetrics, LoadImageW, PostMessageW, SetForegroundWindow, TrackPopupMenu,
                HICON, IMAGE_ICON, LR_DEFAULTCOLOR, MF_SEPARATOR, MF_STRING, SM_CXSMICON,
                SM_CYSMICON, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_NULL,
            },
        },
    },
};

pub const CALLBACK: u32 = WM_APP + 1;
pub const SHOW: u16 = 101;
pub const HIDE: u16 = 102;
pub const TOGGLE: u16 = 103;
pub const QUIT: u16 = 104;

const ICON_ID: u32 = 1;

pub struct Tray {
    pub icon: HICON,
    data: NOTIFYICONDATAW,
    added: bool,
}

impl Tray {
    /// The owner window must remain alive until the tray is dropped.
    pub unsafe fn new(hwnd: HWND) -> Result<Self, String> {
        if hwnd.is_null() {
            return Err("Tray owner window is null".into());
        }
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(format!(
                "Cannot get application module: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Without LR_SHARED this icon belongs to us and is released in Drop.
        let icon = LoadImageW(
            instance,
            ICON_ID as usize as *const u16,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON).max(16),
            GetSystemMetrics(SM_CYSMICON).max(16),
            LR_DEFAULTCOLOR,
        ) as HICON;
        if icon.is_null() {
            return Err(format!(
                "Cannot load application icon: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: icon,
            ..Default::default()
        };
        let mut tooltip = [0u16; 128];
        for (slot, value) in tooltip.iter_mut().zip("Chip Teleprompt".encode_utf16()) {
            *slot = value;
        }
        data.szTip = tooltip;
        let mut tray = Self {
            icon,
            data,
            added: false,
        };
        if !tray.restore() {
            return Err("Windows could not register the notification-area icon".into());
        }
        Ok(tray)
    }

    /// Call again when the registered TaskbarCreated message is received.
    pub unsafe fn restore(&mut self) -> bool {
        if self.registered() {
            self.added = true;
            return true;
        }
        self.added = Shell_NotifyIconW(NIM_ADD, &self.data) != 0;
        if !self.added {
            return false;
        }
        self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if Shell_NotifyIconW(NIM_SETVERSION, &self.data) == 0 {
            // The owner decodes version-4 callbacks, so do not leave a legacy icon.
            Shell_NotifyIconW(NIM_DELETE, &self.data);
            self.added = false;
            return false;
        }
        true
    }

    pub unsafe fn menu(&self, running: bool, visible: bool) -> Option<u16> {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }
        let (visibility_id, visibility_text) = if visible {
            (HIDE, w!("Скрыть"))
        } else {
            (SHOW, w!("Показать"))
        };
        let playback_text = if running {
            w!("Пауза")
        } else {
            w!("Продолжить")
        };
        let populated = AppendMenuW(menu, MF_STRING, visibility_id as usize, visibility_text) != 0
            && AppendMenuW(menu, MF_STRING, TOGGLE as usize, playback_text) != 0
            && AppendMenuW(menu, MF_SEPARATOR, 0, null()) != 0
            && AppendMenuW(menu, MF_STRING, QUIT as usize, w!("Выход")) != 0;
        let mut point = POINT::default();
        if !populated || GetCursorPos(&mut point) == 0 {
            DestroyMenu(menu);
            return None;
        }
        // Foreground ownership plus WM_NULL ensures clicking outside dismisses
        // the menu and subsequent invocations continue to work.
        SetForegroundWindow(self.data.hWnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            point.x,
            point.y,
            0,
            self.data.hWnd,
            null(),
        );
        PostMessageW(self.data.hWnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        Shell_NotifyIconW(NIM_SETFOCUS, &self.data);
        match command as u32 {
            value
                if value == SHOW as u32
                    || value == HIDE as u32
                    || value == TOGGLE as u32
                    || value == QUIT as u32 =>
            {
                Some(value as u16)
            }
            _ => None,
        }
    }

    /// Queries Explorer rather than merely returning a cached registration flag.
    pub unsafe fn registered(&self) -> bool {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.data.hWnd,
            uID: ICON_ID,
            ..Default::default()
        };
        let mut rect = RECT::default();
        Shell_NotifyIconGetRect(&identifier, &mut rect) >= 0
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            if self.added {
                Shell_NotifyIconW(NIM_DELETE, &self.data);
            }
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}

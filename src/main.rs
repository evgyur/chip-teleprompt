#![cfg_attr(not(test), windows_subsystem = "windows")]

mod model;
mod tray;

use model::{Playback, DEFAULT_TEXT};
use std::{
    cell::RefCell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{DataExchange::*, LibraryLoader::*, Memory::*},
    UI::{
        Controls::Dialogs::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const CLASS: &str = "ChipTelepromptRustWindow";
const WHITE: u32 = rgb(245, 247, 250);
const ACCENT: u32 = rgb(0, 170, 255);
const TOOLBAR: u32 = rgb(18, 20, 27);
const MUTED: u32 = rgb(155, 170, 189);
const TIMER_ID: usize = 1;
const WM_EXECUTE: u32 = WM_APP + 2;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const PASTE: u16 = 1;
const NO_BLANKS: u16 = 2;
const FONT: u16 = 3;
const PLAY: u16 = 4;
const TOP: u16 = 5;
const SNAP: u16 = 6;
const CLEAN: u16 = 7;
const HIDE: u16 = 8;
const EXIT: u16 = 9;

thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn contains(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
fn signed_low(value: isize) -> i32 {
    value as i16 as i32
}
fn signed_high(value: isize) -> i32 {
    (value >> 16) as i16 as i32
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    None,
    Stage,
    Move,
    Button(u16),
    FontSlider,
    SpeedSlider,
    Link(u8),
}
#[derive(Clone, Copy)]
enum Drag {
    Text { mouse_y: i32, initial_y: f64 },
    Font,
    Speed,
}
#[derive(Clone, Copy)]
enum Action {
    None,
    FontDialog,
    Snap,
    Hide,
    Show,
    Quit,
    Move,
    TrayMenu,
    Link(u8),
}

struct App {
    hwnd: HWND,
    text: String,
    text_wide: Vec<u16>,
    playback: Playback,
    font: LOGFONTW,
    text_font: HFONT,
    ui_font: HFONT,
    small_font: HFONT,
    font_size: u32,
    color: u32,
    speed: u32,
    clean: bool,
    drag: Option<Drag>,
    hover: Hit,
    focus: Option<usize>,
    text_height: f64,
    last_tick: Instant,
    dpi: u32,
    tray: Option<tray::Tray>,
    taskbar_created: u32,
    status: String,
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.text_font);
            DeleteObject(self.ui_font);
            DeleteObject(self.small_font);
        }
    }
}

impl App {
    unsafe fn new(hwnd: HWND) -> Self {
        let mut font: LOGFONTW = zeroed();
        font.lfHeight = -20; // 15 points at the logical 96 DPI drawing scale.
        font.lfWeight = FW_NORMAL as i32;
        font.lfCharSet = DEFAULT_CHARSET;
        font.lfQuality = CLEARTYPE_QUALITY;
        let face = wide("Ubuntu");
        font.lfFaceName[..face.len()].copy_from_slice(&face);
        let text_font = CreateFontIndirectW(&font);
        let mut ui = font;
        ui.lfFaceName = [0; 32];
        let ui_face = wide("Segoe UI");
        ui.lfFaceName[..ui_face.len()].copy_from_slice(&ui_face);
        ui.lfHeight = -12;
        ui.lfWeight = FW_SEMIBOLD as i32;
        let ui_font = CreateFontIndirectW(&ui);
        ui.lfHeight = -11;
        ui.lfWeight = FW_NORMAL as i32;
        let small_font = CreateFontIndirectW(&ui);
        Self {
            hwnd,
            text: DEFAULT_TEXT.into(),
            text_wide: wide(DEFAULT_TEXT),
            playback: Playback::new(8.0),
            font,
            text_font,
            ui_font,
            small_font,
            font_size: 15,
            color: WHITE,
            speed: 28,
            clean: false,
            drag: None,
            hover: Hit::None,
            focus: None,
            text_height: 0.0,
            last_tick: Instant::now(),
            dpi: GetDpiForWindow(hwnd).max(96),
            tray: None,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            status: String::new(),
        }
    }
    fn top(&self) -> f64 {
        if self.clean {
            6.0
        } else {
            8.0
        }
    }
    fn bottom(&self) -> f64 {
        if self.clean {
            10.0
        } else {
            12.0
        }
    }
    unsafe fn logical_size(&self) -> (i32, i32) {
        let mut r: RECT = zeroed();
        GetClientRect(self.hwnd, &mut r);
        (
            ((r.right as f64 * 96.0 / self.dpi as f64).round() as i32).max(1),
            ((r.bottom as f64 * 96.0 / self.dpi as f64).round() as i32).max(1),
        )
    }
    fn stage(&self, width: i32, height: i32) -> RECT {
        rect(
            6,
            6,
            width - 12,
            height - 6 - if self.clean { 6 } else { 132 },
        )
    }
    fn buttons(&self, width: i32, height: i32) -> Vec<(u16, RECT, &'static str)> {
        let specs = [
            (PASTE, 62, "Paste"),
            (NO_BLANKS, 80, "No blanks"),
            (FONT, 52, "Font"),
            (
                PLAY,
                74,
                if self.playback.running {
                    "Pause"
                } else {
                    "Start"
                },
            ),
            (TOP, 44, "Top"),
            (SNAP, 84, "Sticky top"),
            (CLEAN, 58, "Clean"),
            (HIDE, 44, "Tray"),
            (EXIT, 46, "Exit"),
        ];
        let total = 544 + 8 * 6;
        let ratio = ((width - 32) as f64 / total as f64).min(1.0);
        let mut x = 16;
        specs
            .into_iter()
            .map(|(id, w, label)| {
                let bw = (w as f64 * ratio).floor() as i32;
                let r = rect(x, height - 117, bw, 30);
                x += bw + (6.0 * ratio).floor() as i32;
                (id, r, label)
            })
            .collect()
    }
    fn font_track(height: i32) -> RECT {
        rect(64, height - 51, 135, 32)
    }
    fn speed_track(height: i32) -> RECT {
        rect(328, height - 51, 154, 32)
    }
    fn focus_hit(&self) -> Hit {
        match self.focus {
            Some(i @ 0..=8) => Hit::Button(i as u16 + 1),
            Some(i @ 9..=11) => Hit::Link((i - 9) as u8),
            Some(12) => Hit::FontSlider,
            Some(13) => Hit::SpeedSlider,
            _ => Hit::None,
        }
    }
    unsafe fn focus_key(&mut self, key: u16, shift: bool) -> Option<Action> {
        if self.clean {
            return None;
        }
        if key == VK_TAB {
            self.focus = Some(match self.focus {
                Some(i) => (i + if shift { 13 } else { 1 }) % 14,
                None => {
                    if shift {
                        13
                    } else {
                        0
                    }
                }
            });
            self.refresh();
            return Some(Action::None);
        }
        match self.focus_hit() {
            Hit::Button(id) if key == VK_RETURN => return Some(self.command(id)),
            Hit::Link(id) if key == VK_RETURN => return Some(Action::Link(id)),
            hit @ (Hit::FontSlider | Hit::SpeedSlider) => {
                let font = hit == Hit::FontSlider;
                let current = if font { self.font_size } else { self.speed };
                let (minimum, maximum) = if font { (14, 96) } else { (0, 100) };
                let value = match key {
                    VK_LEFT | VK_DOWN => current.saturating_sub(1).max(minimum),
                    VK_RIGHT | VK_UP => (current + 1).min(maximum),
                    VK_HOME => minimum,
                    VK_END => maximum,
                    _ => return None,
                };
                if font {
                    self.set_font_size(value);
                } else {
                    self.speed = value;
                    self.refresh();
                }
                return Some(Action::None);
            }
            _ => (),
        }
        None
    }
    fn hit(&self, x: i32, y: i32, width: i32, height: i32) -> Hit {
        if contains(self.stage(width, height), x, y) {
            return Hit::Stage;
        }
        if !self.clean {
            for (id, r, _) in self.buttons(width, height) {
                if contains(r, x, y) {
                    return Hit::Button(id);
                }
            }
            if contains(Self::font_track(height), x, y) {
                return Hit::FontSlider;
            }
            if contains(Self::speed_track(height), x, y) {
                return Hit::SpeedSlider;
            }
            if y >= height - 81 && y < height - 60 {
                if (62..112).contains(&x) {
                    return Hit::Link(0);
                }
                if (184..250).contains(&x) {
                    return Hit::Link(1);
                }
                if (306..width - 16).contains(&x) {
                    return Hit::Link(2);
                }
            }
        }
        Hit::Move
    }
    unsafe fn mapped_dc(&self, dc: HDC, dpi: u32) {
        SetMapMode(dc, MM_ANISOTROPIC);
        SetWindowExtEx(dc, 96, 96, null_mut());
        SetViewportExtEx(dc, dpi as i32, dpi as i32, null_mut());
        SetBkMode(dc, TRANSPARENT as i32);
    }
    unsafe fn reflow(&mut self, clamp: bool) {
        let (w, h) = self.logical_size();
        let stage = self.stage(w, h);
        let dc = GetDC(self.hwnd);
        if dc.is_null() {
            return;
        }
        let saved = SaveDC(dc);
        self.mapped_dc(dc, self.dpi);
        SelectObject(dc, self.text_font);
        let mut r = rect(0, 0, (stage.right - stage.left - 32).max(1), 0);
        DrawTextW(
            dc,
            self.text_wide.as_ptr(),
            (self.text_wide.len() - 1) as i32,
            &mut r,
            DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT | DT_EDITCONTROL,
        );
        self.text_height = (r.bottom - r.top).max(1) as f64;
        RestoreDC(dc, saved);
        ReleaseDC(self.hwnd, dc);
        if clamp {
            self.playback.clamp(
                (stage.bottom - stage.top) as f64,
                self.text_height,
                self.top(),
                self.bottom(),
            );
        }
    }
    unsafe fn refresh(&self) {
        InvalidateRect(self.hwnd, null(), 0);
    }
    unsafe fn set_text(&mut self, text: String) {
        self.text_wide = wide(&text);
        self.text = text;
        self.reflow(false);
        self.playback.reset(self.top());
        self.status.clear();
        self.refresh();
    }
    unsafe fn set_font_size(&mut self, size: u32) {
        self.font_size = size.clamp(14, 96);
        self.font.lfHeight = -(self.font_size as f64 * 96.0 / 72.0).round() as i32;
        let replacement = CreateFontIndirectW(&self.font);
        if !replacement.is_null() {
            DeleteObject(self.text_font);
            self.text_font = replacement;
        }
        self.reflow(false);
        self.refresh();
    }
    unsafe fn toggle(&mut self) {
        self.playback.toggle();
        self.last_tick = Instant::now();
        self.sync_timer();
        self.refresh();
    }
    unsafe fn sync_timer(&self) {
        if self.playback.running {
            SetTimer(self.hwnd, TIMER_ID, 16, None);
        } else {
            KillTimer(self.hwnd, TIMER_ID);
        }
    }
    unsafe fn command(&mut self, id: u16) -> Action {
        match id {
            PASTE => match clipboard_text(self.hwnd) {
                Ok(Some(text)) => self.set_text(text),
                Ok(None) => {
                    self.status = "Clipboard has no text".into();
                    self.refresh();
                }
                Err(err) => {
                    self.status = err;
                    self.refresh();
                }
            },
            NO_BLANKS => self.set_text(model::remove_blank_lines(&self.text)),
            FONT => return Action::FontDialog,
            PLAY | tray::TOGGLE => self.toggle(),
            TOP => {
                self.playback.reset(self.top());
                self.refresh();
            }
            SNAP => return Action::Snap,
            CLEAN => {
                self.clean = !self.clean;
                self.focus = None;
                self.reflow(false);
                self.refresh();
            }
            HIDE | tray::HIDE => return Action::Hide,
            EXIT | tray::QUIT => return Action::Quit,
            tray::SHOW => return Action::Show,
            _ => (),
        }
        Action::None
    }
    unsafe fn slide(&mut self, font: bool, x: i32, height: i32) {
        let r = if font {
            Self::font_track(height)
        } else {
            Self::speed_track(height)
        };
        let fraction = ((x - r.left) as f64 / (r.right - r.left) as f64).clamp(0.0, 1.0);
        if font {
            self.set_font_size(14 + (fraction * 82.0).round() as u32);
        } else {
            self.speed = (fraction * 100.0).round() as u32;
            self.refresh();
        }
    }
    unsafe fn render(&self, dc: HDC, width: i32, height: i32, dpi: u32) {
        let saved = SaveDC(dc);
        self.mapped_dc(dc, dpi);
        fill(dc, rect(0, 0, width, height), rgb(5, 6, 10));
        let stage = self.stage(width, height);
        fill(dc, stage, rgb(0, 0, 0));
        let clip_saved = SaveDC(dc);
        IntersectClipRect(dc, stage.left, stage.top, stage.right, stage.bottom);
        SelectObject(dc, self.text_font);
        SetTextColor(dc, self.color);
        let y = stage.top + self.playback.y.round() as i32;
        let mut text_rect = rect(
            stage.left + 16,
            y,
            (stage.right - stage.left - 32).max(1),
            self.text_height.ceil() as i32 + 50,
        );
        DrawTextW(
            dc,
            self.text_wide.as_ptr(),
            (self.text_wide.len() - 1) as i32,
            &mut text_rect,
            DT_WORDBREAK | DT_NOPREFIX | DT_EDITCONTROL,
        );
        RestoreDC(dc, clip_saved);
        if !self.clean {
            fill(dc, rect(6, height - 132, width - 12, 126), TOOLBAR);
            fill(dc, rect(13, height - 127, width - 26, 2), ACCENT);
            SelectObject(dc, self.ui_font);
            for (id, r, label) in self.buttons(width, height) {
                let color = if id == PLAY {
                    if self.playback.running {
                        rgb(206, 134, 0)
                    } else {
                        rgb(0, 118, 212)
                    }
                } else if id == EXIT {
                    rgb(90, 36, 36)
                } else if self.hover == Hit::Button(id) {
                    rgb(55, 70, 90)
                } else {
                    rgb(35, 40, 50)
                };
                fill(dc, r, color);
                frame(dc, r, rgb(70, 80, 95));
                label_text(dc, label, r, WHITE, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }
            SelectObject(dc, self.small_font);
            label_text(
                dc,
                "Author:",
                rect(18, height - 80, 44, 18),
                MUTED,
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "@chipcr",
                rect(62, height - 80, 54, 18),
                rgb(105, 190, 255),
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "Channel:",
                rect(126, height - 80, 58, 18),
                MUTED,
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "@human20",
                rect(184, height - 80, 68, 18),
                rgb(105, 190, 255),
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "Updates:",
                rect(258, height - 80, 48, 18),
                MUTED,
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "github.com/evgyur/chip-teleprompt",
                rect(306, height - 80, width - 322, 18),
                rgb(105, 190, 255),
                DT_VCENTER | DT_SINGLELINE,
            );
            SelectObject(dc, self.ui_font);
            label_text(
                dc,
                "Font",
                rect(18, height - 51, 40, 32),
                WHITE,
                DT_VCENTER | DT_SINGLELINE,
            );
            slider(
                dc,
                Self::font_track(height),
                (self.font_size - 14) as f64 / 82.0,
            );
            label_text(
                dc,
                &format!("{} pt", self.font_size),
                rect(214, height - 51, 65, 32),
                WHITE,
                DT_VCENTER | DT_SINGLELINE,
            );
            label_text(
                dc,
                "Speed",
                rect(278, height - 51, 45, 32),
                WHITE,
                DT_VCENTER | DT_SINGLELINE,
            );
            slider(dc, Self::speed_track(height), self.speed as f64 / 100.0);
            label_text(
                dc,
                &format!("{} px/s", model::speed_px(self.speed)),
                rect(497, height - 51, 85, 32),
                WHITE,
                DT_VCENTER | DT_SINGLELINE,
            );
            if !self.status.is_empty() {
                SelectObject(dc, self.small_font);
                label_text(
                    dc,
                    &self.status,
                    rect(18, height - 19, width - 50, 13),
                    rgb(255, 185, 90),
                    DT_SINGLELINE,
                );
            }
            let focus_rect = match self.focus_hit() {
                Hit::Button(id) => self
                    .buttons(width, height)
                    .into_iter()
                    .find(|b| b.0 == id)
                    .map(|b| b.1),
                Hit::FontSlider => Some(Self::font_track(height)),
                Hit::SpeedSlider => Some(Self::speed_track(height)),
                Hit::Link(0) => Some(rect(62, height - 80, 54, 18)),
                Hit::Link(1) => Some(rect(184, height - 80, 68, 18)),
                Hit::Link(2) => Some(rect(306, height - 80, width - 322, 18)),
                _ => None,
            };
            if let Some(r) = focus_rect {
                DrawFocusRect(dc, &r);
            }
        }
        // Visible, generously hit-tested bottom-right resize grip.
        for i in 0..3 {
            fill(dc, rect(width - 8 - i * 4, height - 9, 2, 2), MUTED);
        }
        RestoreDC(dc, saved);
    }
    unsafe fn paint(&self) {
        let mut ps: PAINTSTRUCT = zeroed();
        let dc = BeginPaint(self.hwnd, &mut ps);
        if dc.is_null() {
            return;
        }
        let mut r: RECT = zeroed();
        GetClientRect(self.hwnd, &mut r);
        let buffer = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
        if !buffer.is_null() && !bitmap.is_null() {
            let old = SelectObject(buffer, bitmap);
            let (w, h) = self.logical_size();
            self.render(buffer, w, h, self.dpi);
            BitBlt(dc, 0, 0, r.right, r.bottom, buffer, 0, 0, SRCCOPY);
            SelectObject(buffer, old);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !buffer.is_null() {
            DeleteDC(buffer);
        }
        EndPaint(self.hwnd, &ps);
    }
}

unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &r, brush);
    DeleteObject(brush);
}
unsafe fn frame(dc: HDC, r: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FrameRect(dc, &r, brush);
    DeleteObject(brush);
}
unsafe fn label_text(dc: HDC, text: &str, mut r: RECT, color: u32, flags: u32) {
    SetTextColor(dc, color);
    let text = wide(text);
    DrawTextW(
        dc,
        text.as_ptr(),
        (text.len() - 1) as i32,
        &mut r,
        flags | DT_NOPREFIX,
    );
}
unsafe fn slider(dc: HDC, r: RECT, fraction: f64) {
    let y = r.top + 12;
    fill(dc, rect(r.left, y, r.right - r.left, 3), rgb(195, 201, 209));
    let x = r.left + ((r.right - r.left) as f64 * fraction).round() as i32;
    fill(dc, rect(r.left, y, x - r.left, 3), ACCENT);
    for i in 0..11 {
        fill(
            dc,
            rect(r.left + i * (r.right - r.left) / 10, y + 12, 1, 3),
            MUTED,
        );
    }
    fill(dc, rect(x - 5, y - 6, 10, 17), rgb(0, 132, 230));
}

unsafe fn clipboard_text(hwnd: HWND) -> Result<Option<String>, String> {
    const UNICODE_TEXT: u32 = 13;
    if IsClipboardFormatAvailable(UNICODE_TEXT) == 0 {
        return Ok(None);
    }
    if OpenClipboard(hwnd) == 0 {
        return Err("Clipboard is busy. Try Paste again.".into());
    }
    let result = (|| {
        let data = GetClipboardData(UNICODE_TEXT);
        if data.is_null() {
            return Err("Cannot read clipboard text".into());
        }
        let capacity = GlobalSize(data) / 2;
        let ptr = GlobalLock(data) as *const u16;
        if ptr.is_null() {
            return Err("Cannot access clipboard text".into());
        }
        let units = std::slice::from_raw_parts(ptr, capacity);
        let length = units.iter().position(|&u| u == 0).unwrap_or(capacity);
        let text = String::from_utf16_lossy(&units[..length]);
        GlobalUnlock(data);
        Ok(Some(text))
    })();
    CloseClipboard();
    result
}

fn with_app<T>(f: impl FnOnce(&mut App) -> T) -> Option<T> {
    APP.with(|slot| slot.try_borrow_mut().ok()?.as_mut().map(f))
}

unsafe fn execute(hwnd: HWND, action: Action) {
    // Actions which enter Windows modal/nested message loops run after releasing RefCell.
    match action {
        Action::None => (),
        Action::Quit => {
            DestroyWindow(hwnd);
        }
        Action::Hide => {
            let can_hide = with_app(|a| {
                a.tray
                    .as_mut()
                    .is_some_and(|t| t.registered() || t.restore())
            })
            .unwrap_or(false);
            if can_hide {
                with_app(|a| {
                    a.playback.running = false;
                    a.sync_timer();
                });
                ShowWindow(hwnd, SW_HIDE);
            } else {
                with_app(|a| {
                    a.status = "Tray unavailable; keeping window visible".into();
                    a.refresh();
                });
                ShowWindow(hwnd, SW_SHOWNORMAL);
            }
        }
        Action::Show => {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
        }
        Action::Move => {
            ReleaseCapture();
            SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
        }
        Action::Snap => {
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                // Existing window DPI is used; WM_DPICHANGED adjusts on a monitor transition.
                let dpi = GetDpiForWindow(hwnd).max(96);
                let width = (700 * dpi / 96) as i32;
                let mut bounds: RECT = zeroed();
                GetWindowRect(hwnd, &mut bounds);
                let height = (bounds.bottom - bounds.top).max((320 * dpi / 96) as i32);
                let x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - width) / 2;
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    x,
                    info.rcWork.top,
                    width,
                    height,
                    SWP_SHOWWINDOW,
                );
                SetForegroundWindow(hwnd);
            }
        }
        Action::FontDialog => {
            if let Some((mut font, color, dpi, size)) =
                with_app(|a| (a.font, a.color, a.dpi, a.font_size))
            {
                font.lfHeight = -(size as f64 * dpi as f64 / 72.0).round() as i32;
                let mut dialog: CHOOSEFONTW = zeroed();
                dialog.lStructSize = size_of::<CHOOSEFONTW>() as u32;
                dialog.hwndOwner = hwnd;
                dialog.lpLogFont = &mut font;
                dialog.rgbColors = color;
                dialog.Flags = CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT | CF_EFFECTS | CF_LIMITSIZE;
                dialog.nSizeMin = 14;
                dialog.nSizeMax = 96;
                if ChooseFontW(&mut dialog) != 0 {
                    with_app(|a| {
                        a.font = font;
                        a.color = dialog.rgbColors;
                        a.set_font_size(((dialog.iPointSize as f64) / 10.0).round() as u32);
                    });
                }
            }
        }
        Action::TrayMenu => {
            // Remove the tray temporarily from RefCell to allow nested paint/timer messages.
            let data = with_app(|a| (a.tray.take(), a.playback.running));
            if let Some((Some(tray), running)) = data {
                let command = tray.menu(running, IsWindowVisible(hwnd) != 0);
                with_app(|a| {
                    a.tray = Some(tray);
                });
                if let Some(id) = command {
                    SendMessageW(hwnd, WM_EXECUTE, id as usize, 0);
                }
            }
        }
        Action::Link(which) => {
            let url = match which {
                0 => "https://t.me/chipcr",
                1 => "https://t.me/human20",
                _ => "https://github.com/evgyur/chip-teleprompt",
            };
            ShellExecuteW(
                hwnd,
                wide("open").as_ptr(),
                wide(url).as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_NCCALCSIZE {
        return 0;
    }
    // Keep native resize hit-testing, but never let Windows paint a frame
    // over our full-window client area, including during activation changes.
    if msg == WM_NCPAINT {
        return 0;
    }
    if msg == WM_NCACTIVATE {
        return 1;
    }
    if msg == WM_GETMINMAXINFO {
        let dpi = GetDpiForWindow(hwnd).max(96);
        let info = &mut *(lp as *mut MINMAXINFO);
        info.ptMinTrackSize = POINT {
            x: (620 * dpi / 96) as i32,
            y: (320 * dpi / 96) as i32,
        };
        return 0;
    }
    if msg == WM_NCHITTEST {
        let mut bounds: RECT = zeroed();
        GetWindowRect(hwnd, &mut bounds);
        let x = signed_low(lp) - bounds.left;
        let y = signed_high(lp) - bounds.top;
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        let edge = (7 * GetDpiForWindow(hwnd).max(96) / 96) as i32;
        let grip = (22 * GetDpiForWindow(hwnd).max(96) / 96) as i32;
        if x >= width - grip && y >= height - grip {
            return HTBOTTOMRIGHT as isize;
        }
        let l = x < edge;
        let r = x >= width - edge;
        let t = y < edge;
        let b = y >= height - edge;
        return match (l, r, t, b) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, _, _, _) => HTLEFT,
            (_, true, _, _) => HTRIGHT,
            (_, _, true, _) => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => HTCLIENT,
        } as isize;
    }
    if msg == WM_DPICHANGED {
        let dpi = (wp & 0xffff) as u32;
        with_app(|a| {
            a.dpi = dpi.max(96);
        });
        let r = &*(lp as *const RECT);
        SetWindowPos(
            hwnd,
            null_mut(),
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        with_app(|a| {
            a.reflow(!a.playback.running);
            a.refresh();
        });
        return 0;
    }
    if msg == WM_CLOSE {
        DestroyWindow(hwnd);
        return 0;
    }
    if msg == WM_DESTROY {
        KillTimer(hwnd, TIMER_ID);
        APP.with(|slot| {
            slot.borrow_mut().take();
        });
        PostQuitMessage(0);
        return 0;
    }
    let mut handled = true;
    let action = with_app(|a| {
        if msg == a.taskbar_created {
            if let Some(tray) = a.tray.as_mut() {
                if !tray.restore() {
                    return Action::Show;
                }
            }
            return Action::None;
        }
        match msg {
            WM_PAINT => a.paint(),
            WM_ERASEBKGND => (),
            WM_SIZE => {
                if wp == SIZE_MINIMIZED as usize {
                    return Action::Hide;
                }
                a.reflow(!a.playback.running);
                a.refresh();
            }
            WM_EXECUTE => return a.command(wp as u16),
            WM_TIMER if wp == TIMER_ID => {
                let now = Instant::now();
                let dt = now.duration_since(a.last_tick).as_secs_f64();
                a.last_tick = now;
                if a.playback.tick(dt, model::speed_px(a.speed), a.text_height) {
                    a.refresh();
                }
                a.sync_timer();
            }
            WM_KEYDOWN => {
                if wp == VK_RETURN as usize && lp & (1 << 30) != 0 {
                    return Action::None;
                }
                if let Some(action) = a.focus_key(wp as u16, GetKeyState(VK_SHIFT as i32) < 0) {
                    return action;
                }
                if lp & (1 << 30) != 0 {
                    return Action::None;
                }
                let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
                let id = if ctrl && wp == b'V' as usize {
                    PASTE
                } else if ctrl && wp == b'T' as usize {
                    SNAP
                } else if wp == VK_SPACE as usize {
                    PLAY
                } else if wp == VK_F11 as usize || (wp == VK_ESCAPE as usize && a.clean) {
                    CLEAN
                } else {
                    0
                };
                return a.command(id);
            }
            WM_LBUTTONDOWN => {
                let x = (signed_low(lp) as f64 * 96.0 / a.dpi as f64).round() as i32;
                let y = (signed_high(lp) as f64 * 96.0 / a.dpi as f64).round() as i32;
                let (w, h) = a.logical_size();
                match a.hit(x, y, w, h) {
                    Hit::Stage => {
                        a.playback.running = false;
                        a.sync_timer();
                        a.drag = Some(Drag::Text {
                            mouse_y: y,
                            initial_y: a.playback.y,
                        });
                        SetCapture(hwnd);
                        a.refresh();
                    }
                    Hit::FontSlider => {
                        a.drag = Some(Drag::Font);
                        SetCapture(hwnd);
                        a.slide(true, x, h);
                    }
                    Hit::SpeedSlider => {
                        a.drag = Some(Drag::Speed);
                        SetCapture(hwnd);
                        a.slide(false, x, h);
                    }
                    Hit::Button(id) => return a.command(id),
                    Hit::Link(id) => return Action::Link(id),
                    Hit::Move => return Action::Move,
                    Hit::None => (),
                }
            }
            WM_MOUSEMOVE => {
                let x = (signed_low(lp) as f64 * 96.0 / a.dpi as f64).round() as i32;
                let y = (signed_high(lp) as f64 * 96.0 / a.dpi as f64).round() as i32;
                let (w, h) = a.logical_size();
                if let Some(drag) = a.drag {
                    match drag {
                        Drag::Text { mouse_y, initial_y } => {
                            let stage = a.stage(w, h);
                            a.playback.drag_to(
                                initial_y + (y - mouse_y) as f64,
                                (stage.bottom - stage.top) as f64,
                                a.text_height,
                                a.top(),
                                a.bottom(),
                            );
                            a.refresh();
                        }
                        Drag::Font => a.slide(true, x, h),
                        Drag::Speed => a.slide(false, x, h),
                    }
                } else {
                    let hit = a.hit(x, y, w, h);
                    if hit != a.hover {
                        a.hover = hit;
                        a.refresh();
                    }
                    let cursor = match hit {
                        Hit::Stage => IDC_SIZEALL,
                        Hit::Move => IDC_SIZEALL,
                        _ => IDC_HAND,
                    };
                    SetCursor(LoadCursorW(null_mut(), cursor));
                }
            }
            WM_LBUTTONUP | WM_CAPTURECHANGED => {
                a.drag = None;
                if msg == WM_LBUTTONUP {
                    ReleaseCapture();
                }
            }
            WM_MOUSEWHEEL => {
                let steps = (wp >> 16) as i16 as f64 / 120.0;
                let (w, h) = a.logical_size();
                let stage = a.stage(w, h);
                a.playback.drag_to(
                    a.playback.y + steps * 40.0,
                    (stage.bottom - stage.top) as f64,
                    a.text_height,
                    a.top(),
                    a.bottom(),
                );
                a.sync_timer();
                a.refresh();
            }
            tray::CALLBACK => match (lp as u32) & 0xffff {
                NIN_SELECT | NIN_KEYSELECT => return Action::Show,
                WM_CONTEXTMENU => return Action::TrayMenu,
                _ => (),
            },
            _ => handled = false,
        }
        Action::None
    });
    if let Some(action) = action {
        execute(hwnd, action);
        if handled {
            return if msg == WM_ERASEBKGND { 1 } else { 0 };
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe fn run(smoke_dir: Option<&std::path::Path>) -> Result<(), String> {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let class = wide(CLASS);
    if smoke_dir.is_none() {
        let existing = FindWindowW(class.as_ptr(), null());
        if !existing.is_null() {
            execute(existing, Action::Show);
            return Ok(());
        }
    }
    let instance = GetModuleHandleW(null());
    let icon = LoadIconW(instance, std::ptr::without_provenance(1));
    let mut wc: WNDCLASSEXW = zeroed();
    wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = Some(wndproc);
    wc.hInstance = instance;
    wc.hIcon = icon;
    wc.hIconSm = icon;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.lpszClassName = class.as_ptr();
    if RegisterClassExW(&wc) == 0 {
        return Err(format!("Cannot register window: {}", GetLastError()));
    }
    let dpi = GetDpiForSystem().max(96);
    let width = (700 * dpi / 96) as i32;
    let height = (320 * dpi / 96) as i32;
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0);
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_APPWINDOW,
        class.as_ptr(),
        wide("Chip Teleprompt").as_ptr(),
        WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX,
        work.left + (work.right - work.left - width) / 2,
        work.top,
        width,
        height,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err(format!("Cannot create window: {}", GetLastError()));
    }
    let policy = DWMNCRP_DISABLED;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_NCRENDERING_POLICY as u32,
        &policy as *const _ as _,
        size_of_val(&policy) as u32,
    );
    // Refresh Windows' cached frame metrics after taking over non-client layout.
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
    let mut app = App::new(hwnd);
    app.tray = Some(tray::Tray::new(hwnd)?);
    app.reflow(true);
    APP.with(|slot| {
        *slot.borrow_mut() = Some(app);
    });
    if let Some(dir) = smoke_dir {
        let result = smoke_test(hwnd, dir);
        DestroyWindow(hwnd);
        return result;
    }
    ShowWindow(hwnd, SW_SHOWNORMAL);
    UpdateWindow(hwnd);
    let mut message: MSG = zeroed();
    loop {
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result == 0 {
            break;
        }
        if result < 0 {
            return Err(format!("Message loop failed: {}", GetLastError()));
        }
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let smoke = args.get(1).is_some_and(|s| s == "--smoke-test");
    let dir = if smoke {
        args.get(2).map(std::path::Path::new)
    } else {
        None
    };
    let result = unsafe { run(dir) };
    if let Err(error) = result {
        if let Some(dir) = dir {
            let _ = std::fs::write(dir.join("FAILED.txt"), &error);
        } else {
            unsafe {
                MessageBoxW(
                    null_mut(),
                    wide(&error).as_ptr(),
                    wide("Chip Teleprompt").as_ptr(),
                    MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}

// In-process smoke tests exercise the actual Win32 message handlers without touching the user's clipboard.
unsafe fn smoke_test(hwnd: HWND, dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut checks = Vec::new();
    macro_rules! check {
        ($condition:expr,$label:expr) => {
            if !$condition {
                return Err(format!("FAIL: {}", $label));
            }
            checks.push($label);
        };
    }
    check!(
        with_app(|a| a.logical_size() == (700, 320) && a.font_size == 15 && a.speed == 28)
            .unwrap_or(false),
        "defaults: 700x320, 15 pt, 17 px/s"
    );
    check!(
        with_app(|a| a.tray.as_ref().is_some_and(|t| t.registered())).unwrap_or(false),
        "tray icon registered with Windows Shell"
    );
    check!(
        with_app(|a| {
            let dc = GetDC(hwnd);
            let old = SelectObject(dc, a.text_font);
            let mut face = [0u16; 64];
            GetTextFaceW(dc, face.len() as i32, face.as_mut_ptr());
            SelectObject(dc, old);
            ReleaseDC(hwnd, dc);
            String::from_utf16_lossy(&face).starts_with("Ubuntu")
        })
        .unwrap_or(false),
        "installed Ubuntu font selected by GDI"
    );
    check!(
        with_app(|a| a
            .buttons(620, 320)
            .iter()
            .all(|(_, r, _)| r.left >= 6 && r.right <= 614 && r.bottom < 320))
        .unwrap_or(false),
        "toolbar fits minimum window width"
    );
    with_app(|a| {
        a.render_snapshot(dir.join("preview-100.bmp"), 96)?;
        a.render_snapshot(dir.join("preview-150.bmp"), 144)
    })
    .ok_or("app unavailable")??;
    SendMessageW(hwnd, WM_KEYDOWN, VK_SPACE as usize, 0);
    check!(
        with_app(|a| a.playback.running).unwrap_or(false),
        "Space starts playback"
    );
    with_app(|a| a.set_text("  Привет, мир!  \r\n\r\n  Second line\t\r\n   \r\n".into()));
    check!(
        with_app(|a| a.playback.running && a.playback.y == 8.0).unwrap_or(false),
        "Unicode text replacement retains playback and resets position"
    );
    SendMessageW(hwnd, WM_EXECUTE, NO_BLANKS as usize, 0);
    check!(
        with_app(|a| a.text == "  Привет, мир!\r\n  Second line" && a.playback.running)
            .unwrap_or(false),
        "No blanks preserves Cyrillic, indentation, playback"
    );
    SendMessageW(hwnd, WM_EXECUTE, TOP as usize, 0);
    check!(
        with_app(|a| a.playback.running && a.playback.y == 8.0).unwrap_or(false),
        "Top retains playback"
    );
    SendMessageW(hwnd, WM_KEYDOWN, VK_F11 as usize, 0);
    check!(
        with_app(|a| a.clean).unwrap_or(false),
        "F11 enters clean mode"
    );
    SendMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0);
    check!(
        with_app(|a| !a.clean).unwrap_or(false),
        "Escape exits clean mode"
    );
    for _ in 0..13 {
        SendMessageW(hwnd, WM_KEYDOWN, VK_TAB as usize, 0);
    }
    check!(
        with_app(|a| a.focus_hit() == Hit::FontSlider).unwrap_or(false),
        "Tab reaches font slider"
    );
    SendMessageW(hwnd, WM_KEYDOWN, VK_RIGHT as usize, 0);
    check!(
        with_app(|a| a.font_size == 16).unwrap_or(false),
        "arrow key adjusts font slider"
    );
    SendMessageW(hwnd, WM_KEYDOWN, VK_TAB as usize, 0);
    SendMessageW(hwnd, WM_KEYDOWN, VK_END as usize, 0);
    check!(
        with_app(|a| a.speed == 100).unwrap_or(false),
        "End sets speed maximum"
    );
    SendMessageW(hwnd, WM_KEYDOWN, VK_HOME as usize, 0);
    check!(
        with_app(|a| a.speed == 0).unwrap_or(false),
        "Home sets speed minimum"
    );
    with_app(|a| {
        a.speed = 28;
        a.focus = None;
    });
    let dpi = GetDpiForWindow(hwnd).max(96);
    let mut bounds: RECT = zeroed();
    GetWindowRect(hwnd, &mut bounds);
    let grip_x = bounds.right - (12 * dpi / 96) as i32;
    let grip_y = bounds.bottom - (12 * dpi / 96) as i32;
    let grip_point = (((grip_y as u16 as u32) << 16) | (grip_x as u16 as u32)) as isize;
    check!(
        SendMessageW(hwnd, WM_NCHITTEST, 0, grip_point) == HTBOTTOMRIGHT as isize,
        "visible corner grip resizes window"
    );
    let point = ((40 * dpi / 96) as isize) << 16 | ((40 * dpi / 96) as isize);
    SendMessageW(hwnd, WM_LBUTTONDOWN, 0, point);
    SendMessageW(hwnd, WM_LBUTTONUP, 0, point);
    check!(
        with_app(|a| !a.playback.running).unwrap_or(false),
        "manual drag pauses playback"
    );
    with_app(|a| {
        a.font.lfItalic = 1;
        a.color = rgb(110, 220, 255);
        a.set_font_size(24);
    });
    check!(
        with_app(|a| a.font_size == 24 && a.font.lfItalic == 1 && a.color == rgb(110, 220, 255))
            .unwrap_or(false),
        "font size preserves style and color"
    );
    with_app(|a| {
        a.font.lfItalic = 0;
        a.color = WHITE;
        a.set_font_size(15);
        a.set_text(DEFAULT_TEXT.into());
    });
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        (820 * dpi / 96) as i32,
        (400 * dpi / 96) as i32,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    SendMessageW(hwnd, WM_EXECUTE, SNAP as usize, 0);
    check!(
        with_app(|a| a.logical_size() == (700, 400)).unwrap_or(false),
        "Sticky top resets width and preserves resized height"
    );
    SetWindowPos(
        hwnd,
        null_mut(),
        0,
        0,
        (700 * dpi / 96) as i32,
        (320 * dpi / 96) as i32,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    SendMessageW(hwnd, WM_EXECUTE, tray::SHOW as usize, 0);
    check!(IsWindowVisible(hwnd) != 0, "tray Show restores window");
    SendMessageW(hwnd, WM_EXECUTE, tray::HIDE as usize, 0);
    check!(IsWindowVisible(hwnd) == 0, "tray Hide hides window");
    SendMessageW(
        hwnd,
        tray::CALLBACK,
        0,
        ((1u32 << 16) | NIN_SELECT) as isize,
    );
    check!(
        IsWindowVisible(hwnd) != 0,
        "tray left-click callback restores window"
    );
    SendMessageW(hwnd, WM_EXECUTE, tray::TOGGLE as usize, 0);
    check!(
        with_app(|a| a.playback.running).unwrap_or(false),
        "tray Continue resumes playback"
    );
    SendMessageW(hwnd, WM_EXECUTE, tray::TOGGLE as usize, 0);
    check!(
        with_app(|a| !a.playback.running).unwrap_or(false),
        "tray Pause stops playback"
    );
    // Simulate Explorer forgetting this icon, without restarting the user's shell.
    let mut missing: NOTIFYICONDATAW = zeroed();
    missing.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    missing.hWnd = hwnd;
    missing.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &missing);
    let taskbar_message = with_app(|a| a.taskbar_created).ok_or("app unavailable")?;
    SendMessageW(hwnd, taskbar_message, 0, 0);
    check!(
        with_app(|a| a.tray.as_ref().is_some_and(|t| t.registered())).unwrap_or(false),
        "TaskbarCreated recreates missing tray icon"
    );
    ShowWindow(hwnd, SW_HIDE);
    with_app(|a| a.render_snapshot(dir.join("preview-restored.bmp"), 144))
        .ok_or("app unavailable")??;
    let report = format!(
        "Chip Teleprompt {} native Windows smoke: {} checks passed\n{}\n",
        env!("CARGO_PKG_VERSION"),
        checks.len(),
        checks.join("\n")
    );
    std::fs::write(dir.join("smoke-results.txt"), report).map_err(|e| e.to_string())?;
    Ok(())
}

impl App {
    unsafe fn render_snapshot(&self, path: std::path::PathBuf, dpi: u32) -> Result<(), String> {
        let width = (700 * dpi / 96) as i32;
        let height = (320 * dpi / 96) as i32;
        let screen = GetDC(self.hwnd);
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if dc.is_null() || bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !dc.is_null() {
                DeleteDC(dc);
            }
            ReleaseDC(self.hwnd, screen);
            return Err("Cannot create screenshot bitmap".into());
        }
        let old = SelectObject(dc, bitmap);
        self.render(dc, 700, 320, dpi);
        GdiFlush();
        let size = (width * height * 4) as usize;
        let pixels = std::slice::from_raw_parts(bits as *const u8, size);
        let mut bytes = Vec::with_capacity(size + 54);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&((size + 54) as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&(-height).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(pixels);
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(self.hwnd, screen);
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

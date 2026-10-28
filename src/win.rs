//! The Windows desktop widget: a per-pixel-alpha layered window, tray icon,
//! right-click menu, start-with-Windows and settings-file handling.
//!
//! Plain Win32 through `windows-sys`, so the exe stays small and the widget
//! uses a few MB of memory and almost no CPU (it redraws once a second).

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_ANYCOLOR, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use world_clock_widget::cities::{self, CITIES, REGIONS};
use world_clock_widget::clock::{self, ResolvedClock};
use world_clock_widget::config::{
    self, ClockConfig, ColorField, Config, DialStyle, Layer, Size, Theme, LOCAL_ZONE, MAX_CLOCKS,
};
use world_clock_widget::render::{self, Palette};
use world_clock_widget::text::Fonts;

const APP_NAME: &str = "World Clock Widget";
const CLASS_NAME: &str = "WorldClockWidgetWindow";
const MUTEX_NAME: &str = "Local\\WorldClockWidget.SingleInstance";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "WorldClockWidget";

const TIMER_TICK: usize = 1;
const WM_APP_TRAY: u32 = WM_APP + 1;
const WM_APP_FOREGROUND: u32 = WM_APP + 2;
const WM_APP_ERROR: u32 = WM_APP + 3;
const TRAY_ID: u32 = 1;

// Menu command ids.
const ID_MAKE_MASTER: u32 = 1;
const ID_REMOVE: u32 = 2;
const ID_24H: u32 = 10;
const ID_SECOND_HAND: u32 = 11;
const ID_LOCK: u32 = 12;
const ID_AUTOSTART: u32 = 13;
const ID_SIZE_BASE: u32 = 20; // + 0..3
const ID_THEME_BASE: u32 = 30; // + 0..3
const ID_LAYER_BASE: u32 = 40; // + 0..3
const ID_EDIT_SETTINGS: u32 = 50;
const ID_COLOR_BASE: u32 = 60; // + 0..7, opens the colour picker
const ID_COLOR_RESET: u32 = 70;
const ID_DIAL_BASE: u32 = 80; // + 0..6, every clock
const ID_EXIT: u32 = 99;
const ID_CLOCK_DIAL_SAME: u32 = 110; // the clicked clock follows the others
const ID_CLOCK_DIAL_BASE: u32 = 111; // + 0..6, the clicked clock only
const ID_SET_ZONE: u32 = 1000; // + city index
const ID_SET_LOCAL: u32 = 1900;
const ID_SET_UTC: u32 = 1901;
const ID_ADD_ZONE: u32 = 2000; // + city index
const ID_ADD_LOCAL: u32 = 2900;
const ID_ADD_UTC: u32 = 2901;

const SIZES: [(Size, &str); 3] = [(Size::Small, "Small"), (Size::Medium, "Medium"), (Size::Large, "Large")];
const LAYERS: [(Layer, &str); 3] = [
    (Layer::Desktop, "On the desktop (behind windows)"),
    (Layer::Normal, "Like a normal window"),
    (Layer::Top, "Always on top"),
];

// State the window procedure needs without borrowing the app (it can be
// called re-entrantly from inside Win32 calls the app makes).
static MAIN_HWND: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static LAYER: AtomicU8 = AtomicU8::new(0);
static RAISED: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut app| app.as_mut().map(f)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn layer_code(layer: Layer) -> u8 {
    match layer {
        Layer::Desktop => 0,
        Layer::Normal => 1,
        Layer::Top => 2,
    }
}

fn message_box(hwnd: HWND, text: &str, flags: MESSAGEBOX_STYLE) {
    let (text, title) = (wide(text), wide(APP_NAME));
    unsafe { MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), flags) };
}

/// tiny-skia is premultiplied RGBA; Windows bitmaps want premultiplied BGRA.
fn rgba_to_bgra(dst: &mut [u8], src: &[u8]) {
    let (dst, _) = dst.as_chunks_mut::<4>();
    let (src, _) = src.as_chunks::<4>();
    for (d, s) in dst.iter_mut().zip(src) {
        *d = [s[2], s[1], s[0], s[3]];
    }
}

/// Off-screen bitmap the widget is drawn into before being handed to Windows.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Surface {
    unsafe fn new(width: i32, height: i32) -> Option<Surface> {
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height; // top-down rows
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut c_void = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, bitmap);
        Some(Surface { dc, bitmap, old, bits: bits as *mut u8, width, height })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

struct App {
    hwnd: HWND,
    cfg: Config,
    cfg_path: PathBuf,
    cfg_mtime: Option<SystemTime>,
    fonts: Fonts,
    clocks: Vec<ResolvedClock>,
    dpi: u32,
    surface: Option<Surface>,
    last_frame: String,
    base: Option<(String, tiny_skia::Pixmap)>,
    position_dirty: bool,
    icon: HICON,
    tray_added: bool,
    hook: HWINEVENTHOOK,
    ticks: u32,
    pending_error: Option<String>,
    /// The colour picker's 16 "custom colours" boxes, seeded from the theme.
    swatches: [COLORREF; 16],
}

impl App {
    fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0 * self.cfg.size.factor()
    }

    fn resolve(&mut self) {
        self.clocks = world_clock_widget::resolve_clocks(&self.cfg);
    }

    /// Draws the widget if anything visible changed since the last frame.
    fn redraw(&mut self, force: bool) {
        let now = jiff::Timestamp::now();
        let mut readings = clock::read_all(now, &self.clocks, self.cfg.master_index());
        let scale = self.scale();
        if !self.cfg.second_hand {
            for r in &mut readings {
                r.second = 0;
            }
        }
        let key = format!("{readings:?}{:?}{scale}", self.cfg);
        if !force && key == self.last_frame {
            return;
        }
        self.last_frame = key;

        // The face, text and panel only change once a minute: keep them and
        // draw just the hands on top each second.
        let mut still = readings.clone();
        for r in &mut still {
            r.second = 0;
        }
        let base_key = format!("{still:?}{:?}{scale}", self.cfg);
        if force || self.base.as_ref().is_none_or(|(k, _)| *k != base_key) {
            let base = render::render_base(&readings, &self.cfg, scale, &self.fonts);
            self.base = Some((base_key, base));
        }
        let Some((_, base)) = &self.base else { return };
        let mut pixmap = base.clone();
        render::draw_hands(&mut pixmap, &readings, &self.cfg, scale);
        unsafe { self.present(&pixmap) };
        if self.position_dirty {
            self.position_dirty = false;
            self.save();
        }
    }

    unsafe fn present(&mut self, pixmap: &tiny_skia::Pixmap) {
        let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
        let first_frame = self.surface.is_none();
        let resized = self.surface.as_ref().is_none_or(|s| s.width != w || s.height != h);
        if resized {
            self.surface = Surface::new(w, h);
        }
        let Some(surface) = &self.surface else { return };

        let dst = std::slice::from_raw_parts_mut(surface.bits, (w * h * 4) as usize);
        rgba_to_bgra(dst, pixmap.data());

        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(self.hwnd, &mut rect);
        let mut pos = POINT { x: rect.left, y: rect.top };
        if resized && !first_frame {
            // Grow towards the middle of the screen: if the widget sits on the
            // right half of its monitor, keep its right edge where it is.
            let old_w = rect.right - rect.left;
            if let Some(work) = monitor_work_area(&rect) {
                let centre = rect.left + old_w / 2;
                if centre > (work.left + work.right) / 2 {
                    pos.x = rect.right - w;
                }
            }
            if self.cfg.x != Some(pos.x) || self.cfg.y != Some(pos.y) {
                self.cfg.x = Some(pos.x);
                self.cfg.y = Some(pos.y);
                self.position_dirty = true;
            }
        }
        let size = SIZE { cx: w, cy: h };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(self.hwnd, null_mut(), &pos, &size, surface.dc, &src, 0, &blend, ULW_ALPHA);
    }

    fn on_tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        // Pick up a changed PC time zone (e.g. after travelling) once a minute.
        if self.ticks.is_multiple_of(60) && self.cfg.clocks.iter().any(|c| c.zone.eq_ignore_ascii_case(LOCAL_ZONE)) {
            self.resolve();
        }
        self.check_settings_file();
        self.redraw(false);
        schedule_tick(self.hwnd);
    }

    /// Reloads the settings if the file was edited by hand.
    fn check_settings_file(&mut self) {
        let mtime = std::fs::metadata(&self.cfg_path).and_then(|m| m.modified()).ok();
        if mtime.is_none() || mtime == self.cfg_mtime {
            return;
        }
        self.cfg_mtime = mtime;
        match Config::load(&self.cfg_path) {
            Ok(cfg) => {
                let moved = cfg.x != self.cfg.x || cfg.y != self.cfg.y;
                self.cfg = cfg;
                self.apply(false);
                if moved {
                    self.move_to_saved_position();
                }
            }
            Err(e) => {
                self.pending_error = Some(format!(
                    "The settings file has a mistake, so the last good settings are still in use.\n\n{e}\n\nFile: {}",
                    self.cfg_path.display()
                ));
                unsafe { PostMessageW(self.hwnd, WM_APP_ERROR, 0, 0) };
            }
        }
    }

    fn save(&mut self) {
        let _ = self.cfg.save(&self.cfg_path);
        self.cfg_mtime = std::fs::metadata(&self.cfg_path).and_then(|m| m.modified()).ok();
    }

    /// Applies the current settings to the window, optionally saving them.
    fn apply(&mut self, save: bool) {
        self.cfg = std::mem::take(&mut self.cfg).normalized();
        self.resolve();
        LAYER.store(layer_code(self.cfg.layer), Ordering::Relaxed);
        apply_layer(self.hwnd, self.cfg.layer);
        self.redraw(true);
        if save {
            self.save();
        }
    }

    fn move_to_saved_position(&mut self) {
        if let (Some(x), Some(y)) = (self.cfg.x, self.cfg.y) {
            unsafe {
                SetWindowPos(self.hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        self.ensure_on_screen();
    }

    /// Puts the widget back on a screen if a monitor was unplugged.
    fn ensure_on_screen(&mut self) {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut rect);
            if !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_null() {
                return;
            }
            let (x, y) = default_position(rect.right - rect.left);
            SetWindowPos(self.hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            self.cfg.x = Some(x);
            self.cfg.y = Some(y);
            self.save();
        }
    }

    fn remember_position(&mut self) {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(self.hwnd, &mut rect);
            if self.cfg.x != Some(rect.left) || self.cfg.y != Some(rect.top) {
                self.cfg.x = Some(rect.left);
                self.cfg.y = Some(rect.top);
                self.save();
            }
        }
    }

    fn add_tray_icon(&mut self) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = TRAY_ID;
            nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            nid.uCallbackMessage = WM_APP_TRAY;
            nid.hIcon = self.icon;
            for (dst, src) in nid.szTip.iter_mut().zip(APP_NAME.encode_utf16()) {
                *dst = src;
            }
            self.tray_added = Shell_NotifyIconW(NIM_ADD, &nid) != 0;
        }
    }

    fn remove_tray_icon(&mut self) {
        if !self.tray_added {
            return;
        }
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = TRAY_ID;
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        self.tray_added = false;
    }

    fn menu_state(&self, click_x: Option<i32>) -> MenuState {
        let clicked = click_x.and_then(|x| render::column_at(x as f32, self.cfg.clocks.len(), self.scale()));
        MenuState {
            clicked,
            clicked_title: clicked.map(|i| {
                let label = &self.clocks[i].label;
                let zone = &self.cfg.clocks[i].zone;
                if zone.eq_ignore_ascii_case(LOCAL_ZONE) {
                    format!("{label}  ·  this PC's time zone")
                } else {
                    format!("{label}  ·  {zone}")
                }
            }),
            clicked_zone: clicked.map(|i| self.cfg.clocks[i].clone()),
            clock_count: self.cfg.clocks.len(),
            master: self.cfg.master_index(),
            cfg: self.cfg.clone(),
            autostart: autostart_enabled(),
        }
    }

    /// Runs a menu command. Returns what to do once the app is no longer borrowed.
    fn command(&mut self, id: u32, clicked: Option<usize>) -> AfterMenu {
        let city_clock = |city: &cities::City| ClockConfig::new(city.label, city.zone);
        // Changing a clock's city keeps its own face design.
        let rezone = |clock: &mut ClockConfig, new: ClockConfig| {
            clock.label = new.label;
            clock.zone = new.zone;
        };
        match id {
            ID_MAKE_MASTER => {
                if let Some(i) = clicked {
                    self.cfg.master = i + 1;
                }
            }
            ID_REMOVE => {
                if let Some(i) = clicked {
                    self.cfg.remove_clock(i);
                }
            }
            ID_24H => self.cfg.use_24_hour = !self.cfg.use_24_hour,
            ID_SECOND_HAND => self.cfg.second_hand = !self.cfg.second_hand,
            ID_LOCK => self.cfg.locked = !self.cfg.locked,
            ID_AUTOSTART => {
                set_autostart(!autostart_enabled());
                return AfterMenu::Nothing;
            }
            ID_EDIT_SETTINGS => {
                self.save();
                return AfterMenu::OpenSettings(self.cfg_path.clone());
            }
            ID_EXIT => return AfterMenu::Exit,
            id if (ID_SIZE_BASE..ID_SIZE_BASE + 3).contains(&id) => {
                self.cfg.size = SIZES[(id - ID_SIZE_BASE) as usize].0;
            }
            id if (ID_THEME_BASE..ID_THEME_BASE + Theme::ALL.len() as u32).contains(&id) => {
                self.cfg.theme = Theme::ALL[(id - ID_THEME_BASE) as usize];
            }
            id if (ID_COLOR_BASE..ID_COLOR_BASE + ColorField::ALL.len() as u32).contains(&id) => {
                return AfterMenu::PickColor(ColorField::ALL[(id - ID_COLOR_BASE) as usize]);
            }
            ID_COLOR_RESET => self.cfg.colors = Default::default(),
            id if (ID_DIAL_BASE..ID_DIAL_BASE + DialStyle::ALL.len() as u32).contains(&id) => {
                self.cfg.dial = DialStyle::ALL[(id - ID_DIAL_BASE) as usize];
            }
            id if (ID_CLOCK_DIAL_SAME..=ID_CLOCK_DIAL_BASE + DialStyle::ALL.len() as u32).contains(&id) => {
                if let Some(i) = clicked {
                    self.cfg.clocks[i].dial =
                        id.checked_sub(ID_CLOCK_DIAL_BASE).map(|k| DialStyle::ALL[k as usize]);
                }
            }
            id if (ID_LAYER_BASE..ID_LAYER_BASE + 3).contains(&id) => {
                self.cfg.layer = LAYERS[(id - ID_LAYER_BASE) as usize].0;
                RAISED.store(false, Ordering::Relaxed);
            }
            ID_SET_LOCAL | ID_SET_UTC => {
                if let Some(i) = clicked {
                    let new = if id == ID_SET_LOCAL { ClockConfig::local() } else { city_clock(&cities::UTC) };
                    rezone(&mut self.cfg.clocks[i], new);
                }
            }
            ID_ADD_LOCAL => {
                self.cfg.add_clock(ClockConfig::local());
            }
            ID_ADD_UTC => {
                self.cfg.add_clock(city_clock(&cities::UTC));
            }
            id if (ID_SET_ZONE..ID_SET_ZONE + CITIES.len() as u32).contains(&id) => {
                if let Some(i) = clicked {
                    rezone(&mut self.cfg.clocks[i], city_clock(&CITIES[(id - ID_SET_ZONE) as usize]));
                }
            }
            id if (ID_ADD_ZONE..ID_ADD_ZONE + CITIES.len() as u32).contains(&id) => {
                self.cfg.add_clock(city_clock(&CITIES[(id - ID_ADD_ZONE) as usize]));
            }
            _ => return AfterMenu::Nothing,
        }
        self.apply(true);
        AfterMenu::Nothing
    }

    fn on_foreground(&mut self, foreground: HWND) {
        if LAYER.load(Ordering::Relaxed) != layer_code(Layer::Desktop) || foreground == self.hwnd {
            return;
        }
        // "Show desktop" (Win+D) or clicking the wallpaper focuses the desktop
        // window, which would otherwise cover a widget kept behind everything.
        let desktop = matches!(class_name(foreground).as_str(), "WorkerW" | "Progman");
        let raised = RAISED.load(Ordering::Relaxed);
        unsafe {
            if desktop && !raised {
                RAISED.store(true, Ordering::Relaxed);
                SetWindowPos(self.hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            } else if !desktop && raised {
                RAISED.store(false, Ordering::Relaxed);
                SetWindowPos(self.hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    fn shutdown(&mut self) {
        unsafe {
            KillTimer(self.hwnd, TIMER_TICK);
            if !self.hook.is_null() {
                UnhookWinEvent(self.hook);
                self.hook = null_mut();
            }
        }
        self.remove_tray_icon();
        self.remember_position();
    }
}

struct MenuState {
    clicked: Option<usize>,
    clicked_title: Option<String>,
    clicked_zone: Option<ClockConfig>,
    clock_count: usize,
    master: usize,
    cfg: Config,
    autostart: bool,
}

enum AfterMenu {
    Nothing,
    Exit,
    OpenSettings(PathBuf),
    PickColor(ColorField),
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn apply_layer(hwnd: HWND, layer: Layer) {
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    unsafe {
        match layer {
            Layer::Top => {
                SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
            }
            Layer::Normal => {
                SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
            }
            Layer::Desktop => {
                SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
                SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags);
            }
        }
    }
}

fn monitor_work_area(rect: &RECT) -> Option<RECT> {
    unsafe {
        let monitor = MonitorFromRect(rect, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then_some(info.rcWork)
    }
}

/// Top-right corner of the main screen, clear of the taskbar.
fn default_position(width: i32) -> (i32, i32) {
    unsafe {
        let mut work: RECT = std::mem::zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as *mut c_void, 0);
        (work.right - width - 24, work.top + 24)
    }
}

/// Fires the timer just after the next whole second, so the second hand ticks on time.
fn schedule_tick(hwnd: HWND) {
    let ms_into_second = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_millis());
    let delay = 1000 - ms_into_second + 15;
    unsafe { SetTimer(hwnd, TIMER_TICK, delay, None) };
}

// ---------------------------------------------------------------- autostart

fn exe_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

fn autostart_value() -> Option<String> {
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn autostart_enabled() -> bool {
    autostart_value().is_some()
}

fn set_autostart(on: bool) {
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    unsafe {
        if on {
            let Some(cmd) = exe_command() else { return };
            let data = wide(&cmd);
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr() as *const c_void,
                (data.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr());
        }
    }
}

// ---------------------------------------------------------------- menus

fn escape_menu_text(s: &str) -> Vec<u16> {
    wide(&s.replace('&', "&&"))
}

unsafe fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: u32, text: &str) {
    let text = escape_menu_text(text);
    AppendMenuW(menu, MF_STRING | flags, id as usize, text.as_ptr());
}

unsafe fn append_submenu(menu: HMENU, sub: HMENU, text: &str, enabled: bool) {
    let text = escape_menu_text(text);
    let flags = MF_POPUP | if enabled { MF_ENABLED } else { MF_GRAYED };
    AppendMenuW(menu, flags, sub as usize, text.as_ptr());
}

unsafe fn separator(menu: HMENU) {
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
}

fn check(on: bool) -> MENU_ITEM_FLAGS {
    if on {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    }
}

/// A "pick a time zone" menu. `current` gets a tick next to it.
unsafe fn zone_menu(base: u32, local_id: u32, utc_id: u32, current: Option<&ClockConfig>) -> HMENU {
    let menu = CreatePopupMenu();
    let is = |zone: &str, label: Option<&str>| {
        current.is_some_and(|c| c.zone == zone && (label.is_none() || c.label.as_deref() == label))
    };
    append(menu, check(is(LOCAL_ZONE, None)), local_id, "This PC's time zone");
    append(menu, check(is(cities::UTC.zone, None)), utc_id, "UTC");
    separator(menu);
    for region in REGIONS {
        let sub = CreatePopupMenu();
        for (i, city) in cities::in_region(region) {
            append(sub, check(is(city.zone, Some(city.label))), base + i as u32, city.label);
        }
        append_submenu(menu, sub, region, true);
    }
    menu
}

unsafe fn build_menu(state: &MenuState) -> HMENU {
    let menu = CreatePopupMenu();
    let cfg = &state.cfg;

    if let (Some(i), Some(title)) = (state.clicked, &state.clicked_title) {
        append(menu, MF_GRAYED, 0, title);
        append_submenu(
            menu,
            zone_menu(ID_SET_ZONE, ID_SET_LOCAL, ID_SET_UTC, state.clicked_zone.as_ref()),
            "Change time zone",
            true,
        );
        let own = CreatePopupMenu();
        let own_dial = cfg.clocks.get(i).and_then(|c| c.dial);
        append(own, check(own_dial.is_none()), ID_CLOCK_DIAL_SAME, "Same as the other clocks");
        separator(own);
        for (k, style) in DialStyle::ALL.iter().enumerate() {
            append(own, check(own_dial == Some(*style)), ID_CLOCK_DIAL_BASE + k as u32, style.name());
        }
        append_submenu(menu, own, "This clock's face", true);
        let master = i == state.master;
        append(
            menu,
            check(master) | if master { MF_GRAYED } else { MF_ENABLED },
            ID_MAKE_MASTER,
            "Master clock (differences measured from here)",
        );
        append(menu, if state.clock_count > 1 { MF_ENABLED } else { MF_GRAYED }, ID_REMOVE, "Remove this clock");
        separator(menu);
    }

    let add_label = format!("Add a clock ({} of {MAX_CLOCKS})", state.clock_count);
    append_submenu(
        menu,
        zone_menu(ID_ADD_ZONE, ID_ADD_LOCAL, ID_ADD_UTC, None),
        &add_label,
        state.clock_count < MAX_CLOCKS,
    );
    separator(menu);

    append(menu, check(cfg.use_24_hour), ID_24H, "24-hour time");
    append(menu, check(cfg.second_hand), ID_SECOND_HAND, "Second hand");

    let size = CreatePopupMenu();
    for (i, (s, name)) in SIZES.iter().enumerate() {
        append(size, check(cfg.size == *s), ID_SIZE_BASE + i as u32, name);
    }
    append_submenu(menu, size, "Size", true);

    let dial = CreatePopupMenu();
    for (k, style) in DialStyle::ALL.iter().enumerate() {
        append(dial, check(cfg.dial == *style), ID_DIAL_BASE + k as u32, style.name());
    }
    append_submenu(menu, dial, "Clock face", true);

    let theme = CreatePopupMenu();
    for (k, t) in Theme::ALL.iter().enumerate() {
        append(theme, check(cfg.theme == *t), ID_THEME_BASE + k as u32, t.name());
    }
    append_submenu(menu, theme, "Theme", true);

    // Each item opens the Windows colour palette; a tick means it's customised.
    let colors = CreatePopupMenu();
    for (k, field) in ColorField::ALL.iter().enumerate() {
        let custom = cfg.colors.get(*field).is_some();
        append(colors, check(custom), ID_COLOR_BASE + k as u32, &format!("{}…", field.name()));
    }
    separator(colors);
    let any_custom = !cfg.colors.is_empty();
    append(colors, if any_custom { MF_ENABLED } else { MF_GRAYED }, ID_COLOR_RESET, "Reset to the theme's colours");
    append_submenu(menu, colors, "Colours", true);

    let layer = CreatePopupMenu();
    for (i, (l, name)) in LAYERS.iter().enumerate() {
        append(layer, check(cfg.layer == *l), ID_LAYER_BASE + i as u32, name);
    }
    append_submenu(menu, layer, "Placement", true);

    append(menu, check(cfg.locked), ID_LOCK, "Lock position");
    append(menu, check(state.autostart), ID_AUTOSTART, "Start with Windows");
    separator(menu);
    append(menu, MF_ENABLED, ID_EDIT_SETTINGS, "Edit settings file…");
    append(menu, MF_ENABLED, ID_EXIT, "Exit");
    menu
}

fn show_menu(hwnd: HWND, click_x: Option<i32>) {
    let Some(state) = with_app(|app| app.menu_state(click_x)) else { return };
    let id = unsafe {
        let menu = build_menu(&state);
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Needed so the menu closes when you click somewhere else.
        SetForegroundWindow(hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        id as u32
    };
    if id == 0 {
        return;
    }
    match with_app(|app| app.command(id, state.clicked)) {
        Some(AfterMenu::Exit) => unsafe {
            DestroyWindow(hwnd);
        },
        Some(AfterMenu::OpenSettings(path)) => unsafe {
            let (verb, exe) = (wide("open"), wide("notepad.exe"));
            let args = wide(&format!("\"{}\"", path.display()));
            ShellExecuteW(hwnd, verb.as_ptr(), exe.as_ptr(), args.as_ptr(), null(), SW_SHOWNORMAL);
        },
        Some(AfterMenu::PickColor(field)) => pick_color(hwnd, field),
        _ => {}
    }
}

fn colorref((r, g, b): (u8, u8, u8)) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn from_colorref(c: COLORREF) -> (u8, u8, u8) {
    ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8)
}

/// Opens the standard Windows colour palette for one of the widget's colours.
fn pick_color(hwnd: HWND, field: ColorField) {
    let Some((initial, mut swatches)) = with_app(|app| {
        let pal = Palette::for_config(&app.cfg);
        (pal.color_of(field), app.swatches)
    }) else {
        return;
    };
    unsafe {
        // An invisible owner in the middle of the widget, so the dialog opens
        // over the clocks, in front of other windows, even when the widget
        // itself is pinned behind them.
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        let class = wide("STATIC");
        let owner = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            null(),
            WS_POPUP,
            (rect.left + rect.right) / 2,
            (rect.top + rect.bottom) / 2,
            0,
            0,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        let mut cc: CHOOSECOLORW = std::mem::zeroed();
        cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
        cc.hwndOwner = owner;
        cc.rgbResult = colorref(initial);
        cc.lpCustColors = swatches.as_mut_ptr();
        cc.Flags = CC_RGBINIT | CC_FULLOPEN | CC_ANYCOLOR;
        let picked = ChooseColorW(&mut cc) != 0;
        if !owner.is_null() {
            DestroyWindow(owner);
        }
        with_app(|app| {
            app.swatches = swatches;
            if picked {
                app.cfg.colors.set(field, from_colorref(cc.rgbResult));
                app.apply(true);
            }
        });
    }
}

// ---------------------------------------------------------------- window

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let main = MAIN_HWND.load(Ordering::Relaxed);
    if !main.is_null() {
        PostMessageW(main, WM_APP_FOREGROUND, 0, hwnd as LPARAM);
    }
}

fn x_from_lparam(lparam: LPARAM) -> i32 {
    (lparam & 0xFFFF) as i16 as i32
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam == TIMER_TICK => {
            if with_app(|app| app.on_tick()).is_none() {
                schedule_tick(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if with_app(|app| app.cfg.locked) == Some(false) {
                // Let Windows run its normal window-drag loop.
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
                with_app(|app| app.remember_position());
            }
            0
        }
        WM_RBUTTONUP => {
            show_menu(hwnd, Some(x_from_lparam(lparam)));
            0
        }
        WM_APP_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_CONTEXTMENU {
                show_menu(hwnd, None);
            }
            0
        }
        WM_APP_FOREGROUND => {
            with_app(|app| app.on_foreground(lparam as HWND));
            0
        }
        WM_APP_ERROR => {
            if let Some(Some(text)) = with_app(|app| app.pending_error.take()) {
                message_box(hwnd, &text, MB_OK | MB_ICONWARNING);
            }
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_WINDOWPOSCHANGING => {
            // Keep a desktop-pinned widget underneath every other window.
            // HWND_NOTOPMOST is let through: it's how a widget that was
            // "always on top" drops back down before being sent to the bottom.
            let pos = lparam as *mut WINDOWPOS;
            if !pos.is_null()
                && (*pos).hwndInsertAfter != HWND_NOTOPMOST
                && LAYER.load(Ordering::Relaxed) == layer_code(Layer::Desktop)
                && !RAISED.load(Ordering::Relaxed)
            {
                (*pos).hwndInsertAfter = HWND_BOTTOM;
                (*pos).flags &= !SWP_NOZORDER;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DPICHANGED => {
            let dpi = (wparam & 0xFFFF) as u32;
            with_app(|app| {
                app.dpi = dpi.max(96);
                app.redraw(true);
            });
            0
        }
        WM_DISPLAYCHANGE => {
            with_app(|app| app.ensure_on_screen());
            0
        }
        WM_TIMECHANGE => {
            with_app(|app| {
                app.resolve();
                app.redraw(true);
            });
            0
        }
        WM_ENDSESSION => {
            with_app(|app| app.remember_position());
            0
        }
        WM_DESTROY => {
            with_app(|app| app.shutdown());
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            // Explorer restarted: the tray icon has to be added again.
            with_app(|app| {
                app.tray_added = false;
                app.add_tray_icon();
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Builds the tray icon from the same drawing code as the widget.
unsafe fn make_icon() -> HICON {
    let size = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
    let pixmap = render::render_icon(size);
    let Some(surface) = Surface::new(size as i32, size as i32) else { return null_mut() };
    let dst = std::slice::from_raw_parts_mut(surface.bits, (size * size * 4) as usize);
    rgba_to_bgra(dst, pixmap.data());
    let mask = CreateBitmap(size as i32, size as i32, 1, 1, null());
    let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: surface.bitmap };
    // The color bitmap can't be selected into a DC while the icon is created.
    SelectObject(surface.dc, surface.old);
    let icon = CreateIconIndirect(&info);
    SelectObject(surface.dc, surface.bitmap);
    DeleteObject(mask);
    icon
}

pub fn run() {
    unsafe {
        let mutex_name = wide(MUTEX_NAME);
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            message_box(
                null_mut(),
                "World Clock Widget is already running.\n\nRight-click the clocks or the tray icon for options.",
                MB_OK | MB_ICONINFORMATION,
            );
            return;
        }

        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let fonts = match Fonts::load_system() {
            Ok(f) => f,
            Err(e) => {
                message_box(null_mut(), &e, MB_OK | MB_ICONERROR);
                return;
            }
        };

        // First run: write the default settings and start with Windows from now on.
        let cfg_path = config::default_path();
        let first_run = !cfg_path.exists();
        let mut startup_error = None;
        let cfg = if first_run {
            Config::default()
        } else {
            Config::load(&cfg_path).unwrap_or_else(|e| {
                startup_error = Some(format!(
                    "The settings file has a mistake, so the default settings are being used.\n\n{e}\n\nFile: {}",
                    cfg_path.display()
                ));
                Config::default()
            })
        };
        if first_run {
            set_autostart(true);
        } else if let (Some(current), Some(registered)) = (exe_command(), autostart_value()) {
            // The exe was moved: point start-with-Windows at its new home.
            if current != registered {
                set_autostart(true);
            }
        }

        let hinstance = GetModuleHandleW(null());
        let class_name = wide(CLASS_NAME);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);

        let title = wide(APP_NAME);
        let ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let (x, y) = match (cfg.x, cfg.y) {
            (Some(x), Some(y)) => (x, y),
            _ => (CW_USEDEFAULT, CW_USEDEFAULT),
        };
        let hwnd = CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            1,
            1,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        if hwnd.is_null() {
            message_box(null_mut(), "Couldn't create the widget window.", MB_OK | MB_ICONERROR);
            return;
        }
        MAIN_HWND.store(hwnd, Ordering::Relaxed);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);

        let icon = make_icon();
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );

        let app = App {
            hwnd,
            cfg_mtime: std::fs::metadata(&cfg_path).and_then(|m| m.modified()).ok(),
            cfg,
            cfg_path,
            fonts,
            clocks: Vec::new(),
            dpi: GetDpiForWindow(hwnd).max(96),
            surface: None,
            last_frame: String::new(),
            base: None,
            position_dirty: false,
            icon,
            tray_added: false,
            hook,
            ticks: 0,
            pending_error: startup_error,
            swatches: [0x00FF_FFFF; 16],
        };
        APP.with(|cell| *cell.borrow_mut() = Some(app));

        with_app(|app| {
            for (slot, rgb) in app.swatches.iter_mut().zip(Palette::for_config(&app.cfg).swatches()) {
                *slot = colorref(rgb);
            }
            let has_position = app.cfg.x.is_some() && app.cfg.y.is_some();
            app.resolve();
            app.redraw(true);
            if !has_position {
                let mut rect: RECT = std::mem::zeroed();
                GetWindowRect(app.hwnd, &mut rect);
                let (x, y) = default_position(rect.right - rect.left);
                app.cfg.x = Some(x);
                app.cfg.y = Some(y);
            }
            app.move_to_saved_position();
            ShowWindow(app.hwnd, SW_SHOWNOACTIVATE);
            app.apply(first_run);
            app.add_tray_icon();
            if app.pending_error.is_some() {
                PostMessageW(app.hwnd, WM_APP_ERROR, 0, 0);
            }
        });
        schedule_tick(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        APP.with(|cell| cell.borrow_mut().take());
        if !icon.is_null() {
            DestroyIcon(icon);
        }
    }
}

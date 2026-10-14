// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::borrow::Cow;
use std::ffi::CString;
use std::os::raw::c_char;
use std::os::raw::c_int;

use super::i18n::tr;

pub const TOP_WIDTH: f32 = 400.0;
pub const BOTTOM_WIDTH: f32 = 320.0;

#[inline]
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24)
}

pub const COLOR_BG_TOP: u32 = rgba(246, 246, 242, 255);
pub const COLOR_BG_BOTTOM: u32 = rgba(236, 238, 240, 255);
pub const COLOR_PANEL_WHITE: u32 = rgba(254, 254, 252, 255);
pub const COLOR_PANEL_BORDER: u32 = rgba(214, 214, 208, 255);
pub const COLOR_TEXT_DARK: u32 = rgba(30, 30, 30, 255);
pub const COLOR_TEXT_MUTED: u32 = rgba(100, 105, 115, 255);
pub const COLOR_COUNT_DUE: u32 = rgba(36, 130, 56, 255);
pub const COLOR_COUNT_NEW: u32 = rgba(20, 100, 200, 255);
pub const COLOR_COUNT_LEARN: u32 = rgba(210, 60, 40, 255);

extern "C" {
    fn ui_keyboard(
        hint: *const c_char,
        initial: *const c_char,
        output: *mut c_char,
        capacity: usize,
        secret: bool,
        cancel: *const c_char,
        accept: *const c_char,
    ) -> c_int;
    fn ui_init() -> bool;
    fn ui_fini();
    fn ui_frame_begin();
    fn ui_frame_end();
    fn ui_target_top(bg_color: u32);
    fn ui_target_bottom(bg_color: u32);

    fn ui_draw_rect(x: f32, y: f32, w: f32, h: f32, color: u32);
    fn ui_draw_line(x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: u32);

    fn ui_draw_text(x: f32, y: f32, scale: f32, color: u32, str: *const c_char);
    fn ui_draw_text_centered(x_center: f32, y: f32, scale: f32, color: u32, str: *const c_char);
    fn ui_draw_text_right(x_right: f32, y: f32, scale: f32, color: u32, str: *const c_char);
    fn ui_get_text_width(scale: f32, str: *const c_char) -> f32;

    fn ui_draw_status_bar(screen_w: f32, title: *const c_char);
    fn ui_draw_nav_tab(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: *const c_char,
        icon_type: c_int,
        active: bool,
    );
    fn ui_draw_deck_row(
        y: f32,
        w: f32,
        name: *const c_char,
        level: c_int,
        has_children: bool,
        expanded: bool,
        due_count: u32,
        new_count: u32,
        learn_count: u32,
        selected: bool,
    );
    fn ui_draw_scrollbar(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        total_items: c_int,
        visible_items: c_int,
        scroll_pos: c_int,
    );
    fn ui_draw_button(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: *const c_char,
        subtext: *const c_char,
        style: c_int,
        pressed: bool,
    );

}

// Citro2D renders Fluent's invisible direction isolates as missing glyphs.
// Current UI locales are left-to-right; leave translated text and values
// intact.
fn display_text(text: &str) -> Cow<'_, str> {
    if text.chars().any(|c| matches!(c, '\u{2066}'..='\u{2069}')) {
        Cow::Owned(
            text.chars()
                .filter(|c| !matches!(c, '\u{2066}'..='\u{2069}'))
                .collect(),
        )
    } else {
        Cow::Borrowed(text)
    }
}

pub struct Ui;

impl Ui {
    pub fn keyboard(hint: &str, initial: &str) -> Result<Option<String>, String> {
        Self::keyboard_with_mode(hint, initial, false)
    }
    pub fn keyboard_secret(hint: &str) -> Result<Option<String>, String> {
        Self::keyboard_with_mode(hint, "", true)
    }
    fn keyboard_with_mode(
        hint: &str,
        initial: &str,
        secret: bool,
    ) -> Result<Option<String>, String> {
        let hint = CString::new(hint).map_err(|e| e.to_string())?;
        let initial = CString::new(initial).map_err(|e| e.to_string())?;
        let cancel = CString::new(tr("Cancel")).unwrap();
        let accept = CString::new(tr("Accept")).unwrap();
        let mut output = vec![0u8; 8192];
        match unsafe {
            ui_keyboard(
                hint.as_ptr(),
                initial.as_ptr(),
                output.as_mut_ptr().cast(),
                output.len(),
                secret,
                cancel.as_ptr(),
                accept.as_ptr(),
            )
        } {
            1 => {
                let end = output.iter().position(|b| *b == 0).unwrap_or(output.len());
                let value = String::from_utf8(output[..end].to_vec())
                    .map(Some)
                    .map_err(|e| e.to_string());
                if secret {
                    output.fill(0);
                }
                value
            }
            0 => Ok(None),
            _ => Err(tr("Could not open Nintendo 3DS keyboard.").into()),
        }
    }

    pub fn fit_text(center: f32, y: f32, scale: f32, width: f32, color: u32, text: &str) {
        let text = Self::ellipsize(text, scale, width);
        Self::draw_text_centered(center, y, scale, color, &text);
    }

    pub fn ellipsize(text: &str, scale: f32, width: f32) -> String {
        if Self::get_text_width(scale, text) <= width {
            return text.to_owned();
        }
        let ends: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut low = 0;
        let mut high = ends.len() - 1;
        while low < high {
            let mid = (low + high + 1) / 2;
            if Self::get_text_width(scale, &format!("{}...", &text[..ends[mid]])) <= width {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        format!("{}...", &text[..ends[low]])
    }

    pub fn init() -> bool {
        unsafe { ui_init() }
    }

    pub fn fini() {
        unsafe { ui_fini() }
    }

    pub fn frame_begin() {
        unsafe { ui_frame_begin() }
    }

    pub fn frame_end() {
        unsafe { ui_frame_end() }
    }

    pub fn target_top(bg_color: u32) {
        unsafe { ui_target_top(bg_color) }
    }

    pub fn target_bottom(bg_color: u32) {
        unsafe { ui_target_bottom(bg_color) }
    }

    pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, color: u32) {
        unsafe { ui_draw_rect(x, y, w, h, color) }
    }

    pub fn draw_line(x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32, color: u32) {
        unsafe { ui_draw_line(x0, y0, x1, y1, thickness, color) }
    }

    pub fn draw_text(x: f32, y: f32, scale: f32, color: u32, s: &str) {
        if let Ok(c_str) = CString::new(display_text(s).as_bytes()) {
            unsafe { ui_draw_text(x, y, scale, color, c_str.as_ptr()) }
        }
    }

    pub fn draw_text_centered(x_center: f32, y: f32, scale: f32, color: u32, s: &str) {
        if let Ok(c_str) = CString::new(display_text(s).as_bytes()) {
            unsafe { ui_draw_text_centered(x_center, y, scale, color, c_str.as_ptr()) }
        }
    }

    pub fn draw_text_right(x_right: f32, y: f32, scale: f32, color: u32, s: &str) {
        if let Ok(c_str) = CString::new(display_text(s).as_bytes()) {
            unsafe { ui_draw_text_right(x_right, y, scale, color, c_str.as_ptr()) }
        }
    }

    pub fn get_text_width(scale: f32, s: &str) -> f32 {
        if let Ok(c_str) = CString::new(display_text(s).as_bytes()) {
            unsafe { ui_get_text_width(scale, c_str.as_ptr()) }
        } else {
            0.0
        }
    }

    pub fn draw_status_bar(screen_w: f32, title: &str) {
        if let Ok(c_str) = CString::new(display_text(title).as_bytes()) {
            unsafe { ui_draw_status_bar(screen_w, c_str.as_ptr()) }
        }
    }

    pub fn draw_nav_tab(x: f32, y: f32, w: f32, h: f32, label: &str, icon_type: i32, active: bool) {
        if let Ok(c_str) = CString::new(display_text(label).as_bytes()) {
            unsafe { ui_draw_nav_tab(x, y, w, h, c_str.as_ptr(), icon_type as c_int, active) }
        }
    }

    pub fn draw_deck_row(
        y: f32,
        w: f32,
        name: &str,
        level: i32,
        has_children: bool,
        expanded: bool,
        due_count: u32,
        new_count: u32,
        learn_count: u32,
        selected: bool,
    ) {
        if let Ok(c_str) = CString::new(display_text(name).as_bytes()) {
            unsafe {
                ui_draw_deck_row(
                    y,
                    w,
                    c_str.as_ptr(),
                    level as c_int,
                    has_children,
                    expanded,
                    due_count,
                    new_count,
                    learn_count,
                    selected,
                )
            }
        }
    }

    pub fn draw_scrollbar(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        total_items: usize,
        visible_items: usize,
        scroll_pos: usize,
    ) {
        unsafe {
            ui_draw_scrollbar(
                x,
                y,
                w,
                h,
                total_items as c_int,
                visible_items as c_int,
                scroll_pos as c_int,
            )
        }
    }

    pub fn draw_button(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        subtext: Option<&str>,
        style: i32,
        pressed: bool,
    ) {
        if let Ok(c_text) = CString::new(display_text(text).as_bytes()) {
            let c_sub = subtext.and_then(|s| CString::new(display_text(s).as_bytes()).ok());
            let sub_ptr = c_sub
                .as_ref()
                .map(|c| c.as_ptr())
                .unwrap_or(std::ptr::null());
            unsafe {
                ui_draw_button(
                    x,
                    y,
                    w,
                    h,
                    c_text.as_ptr(),
                    sub_ptr,
                    style as c_int,
                    pressed,
                )
            }
        }
    }
}

// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#include "ui_renderer.h"
#include <3ds.h>
#include <citro2d.h>
#include <citro3d.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

static C3D_RenderTarget* s_top_target = NULL;
static C3D_RenderTarget* s_bottom_target = NULL;
static C2D_TextBuf s_text_buf = NULL;
static C2D_TextBuf s_measure_buf = NULL;
static bool s_initialized = false;

bool ui_init(void) {
    if (s_initialized) return true;

    if (!C3D_Init(C3D_DEFAULT_CMDBUF_SIZE)) {
        return false;
    }
    if (!C2D_Init(C2D_DEFAULT_MAX_OBJECTS)) {
        C3D_Fini();
        return false;
    }
    C2D_Prepare();

    s_top_target = C2D_CreateScreenTarget(GFX_TOP, GFX_LEFT);
    s_bottom_target = C2D_CreateScreenTarget(GFX_BOTTOM, GFX_LEFT);
    s_text_buf = C2D_TextBufNew(4096);
    s_measure_buf = C2D_TextBufNew(1024);

    s_initialized = true;
    return true;
}

void ui_fini(void) {
    if (!s_initialized) return;

    if (s_text_buf) {
        C2D_TextBufDelete(s_text_buf);
        s_text_buf = NULL;
    }
    if (s_measure_buf) {
        C2D_TextBufDelete(s_measure_buf);
        s_measure_buf = NULL;
    }

    C2D_Fini();
    C3D_Fini();
    s_initialized = false;
}

void ui_frame_begin(void) {
    C3D_FrameBegin(C3D_FRAME_SYNCDRAW);
    if (s_text_buf) {
        C2D_TextBufClear(s_text_buf);
    }
}

void ui_frame_end(void) {
    C3D_FrameEnd(0);
}

void ui_target_top(uint32_t bg_color) {
    if (!s_top_target) return;
    C2D_TargetClear(s_top_target, bg_color);
    C2D_SceneBegin(s_top_target);
}

void ui_target_bottom(uint32_t bg_color) {
    if (!s_bottom_target) return;
    C2D_TargetClear(s_bottom_target, bg_color);
    C2D_SceneBegin(s_bottom_target);
}

void ui_draw_rect(float x, float y, float w, float h, uint32_t color) {
    C2D_DrawRectSolid(x, y, 0.5f, w, h, color);
}

void ui_draw_rect_gradient(float x, float y, float w, float h, uint32_t c_top, uint32_t c_bot) {
    C2D_DrawRectangle(x, y, 0.5f, w, h, c_top, c_top, c_bot, c_bot);
}

void ui_draw_line(float x0, float y0, float x1, float y1, float thickness, uint32_t color) {
    C2D_DrawLine(x0, y0, color, x1, y1, color, thickness, 0.5f);
}

void ui_draw_circle(float x, float y, float radius, uint32_t color) {
    C2D_DrawCircleSolid(x, y, 0.5f, radius, color);
}

void ui_draw_triangle(float x0, float y0, float x1, float y1, float x2, float y2, uint32_t color) {
    C2D_DrawTriangle(x0, y0, color, x1, y1, color, x2, y2, color, 0.5f);
}

void ui_draw_text(float x, float y, float scale, uint32_t color, const char* str) {
    if (!str || !str[0] || !s_text_buf) return;
    C2D_Text text;
    C2D_TextParse(&text, s_text_buf, str);
    C2D_TextOptimize(&text);
    C2D_DrawText(&text, C2D_WithColor, x, y, 0.6f, scale, scale, color);
}

float ui_get_text_width(float scale, const char* str) {
    if (!str || !str[0] || !s_measure_buf) return 0.0f;
    C2D_TextBufClear(s_measure_buf);
    C2D_Text text;
    C2D_TextParse(&text, s_measure_buf, str);
    float w = 0.0f, h = 0.0f;
    C2D_TextGetDimensions(&text, scale, scale, &w, &h);
    return w;
}

void ui_draw_text_centered(float x_center, float y, float scale, uint32_t color, const char* str) {
    float w = ui_get_text_width(scale, str);
    ui_draw_text(x_center - (w / 2.0f), y, scale, color, str);
}

void ui_draw_text_right(float x_right, float y, float scale, uint32_t color, const char* str) {
    float w = ui_get_text_width(scale, str);
    ui_draw_text(x_right - w, y, scale, color, str);
}

void ui_draw_status_bar(float screen_w, const char* title) {
    ui_draw_rect_gradient(0, 0, screen_w, 18, C2D_RGBA(248, 248, 246, 255), C2D_RGBA(236, 236, 232, 255));
    ui_draw_line(0, 18, screen_w, 18, 1.0f, C2D_RGBA(210, 210, 204, 255));

    float sig_x = 6.0f;
    float sig_y = 13.0f;
    uint32_t sig_clr = C2D_RGBA(90, 110, 130, 0);
    ui_draw_rect(sig_x, sig_y - 3, 2, 3, sig_clr);
    ui_draw_rect(sig_x + 3, sig_y - 6, 2, 6, sig_clr);
    ui_draw_rect(sig_x + 6, sig_y - 9, 2, 9, sig_clr);

    if (title && title[0]) {
        ui_draw_text(sig_x + 12.0f, 2.0f, 0.45f, C2D_RGBA(40, 40, 40, 255), title);
    }

    extern time_t __real_time(time_t *);
    time_t raw_time = __real_time(NULL);
    struct tm* ti = gmtime(&raw_time);
    char time_str[16];
    if (ti) {
        snprintf(time_str, sizeof(time_str), "%02d:%02d", ti->tm_hour, ti->tm_min);
    } else {
        snprintf(time_str, sizeof(time_str), "--:--");
    }

    static u8 cached_batt = 0;
    static time_t last_poll = 0;
    u8 batt = cached_batt;
    if (raw_time != last_poll && R_SUCCEEDED(ptmuInit())) {
        if (R_SUCCEEDED(PTMU_GetBatteryLevel(&batt))) cached_batt = batt;
        ptmuExit();
        last_poll = raw_time;
    }

    float rx = screen_w - 6.0f;

    float batt_w = 18.0f;
    float batt_h = 9.0f;
    float batt_x = rx - batt_w;
    float batt_y = 5.0f;
    ui_draw_rect(batt_x, batt_y, batt_w, batt_h, C2D_RGBA(120, 120, 120, 255));
    ui_draw_rect(batt_x + 1, batt_y + 1, batt_w - 2, batt_h - 2, C2D_RGBA(250, 250, 250, 255));
    ui_draw_rect(batt_x + batt_w, batt_y + 2, 2, 5, C2D_RGBA(120, 120, 120, 255));

    int bars = (int)batt;
    if (bars > 4) bars = 4;
    uint32_t fill_clr = (bars <= 1) ? C2D_RGBA(220, 50, 40, 255) : C2D_RGBA(60, 160, 60, 255);
    for (int b = 0; b < bars; b++) {
        ui_draw_rect(batt_x + 2 + b * 3.5f, batt_y + 2, 2.5f, batt_h - 4, fill_clr);
    }

    rx = batt_x - 6.0f;
    ui_draw_text_right(rx, 2.0f, 0.45f, C2D_RGBA(40, 40, 40, 255), time_str);
}

void ui_draw_beveled_panel(float x, float y, float w, float h, float radius, uint32_t bg_color, uint32_t border_color) {
    ui_draw_rect(x + 1.0f, y + 1.0f, w, h, C2D_RGBA(0, 0, 0, 25));
    ui_draw_rect(x, y, w, h, bg_color);
    ui_draw_line(x, y, x + w, y, 1.0f, border_color);
    ui_draw_line(x, y + h, x + w, y + h, 1.0f, border_color);
    ui_draw_line(x, y, x, y + h, 1.0f, border_color);
    ui_draw_line(x + w, y, x + w, y + h, 1.0f, border_color);
}

void ui_draw_plaque(float x, float y, float w, float h, const char* title, const char* subtitle) {
    ui_draw_rect(x + 2, y + 3, w, h, C2D_RGBA(0, 0, 0, 40));

    ui_draw_rect_gradient(x, y, w, h, C2D_RGBA(232, 235, 240, 255), C2D_RGBA(178, 184, 192, 255));

    ui_draw_line(x + 1, y + 1, x + w - 1, y + 1, 1.0f, C2D_RGBA(255, 255, 255, 220)); // top highlight
    ui_draw_line(x + 1, y + 1, x + 1, y + h - 1, 1.0f, C2D_RGBA(255, 255, 255, 180)); // left highlight
    ui_draw_line(x + 1, y + h - 1, x + w - 1, y + h - 1, 1.0f, C2D_RGBA(120, 126, 134, 255)); // bot shadow
    ui_draw_line(x + w - 1, y + 1, x + w - 1, y + h - 1, 1.0f, C2D_RGBA(120, 126, 134, 255)); // right shadow

    ui_draw_line(x, y, x + w, y, 1.0f, C2D_RGBA(90, 96, 104, 255));
    ui_draw_line(x, y + h, x + w, y + h, 1.0f, C2D_RGBA(90, 96, 104, 255));
    ui_draw_line(x, y, x, y + h, 1.0f, C2D_RGBA(90, 96, 104, 255));
    ui_draw_line(x + w, y, x + w, y + h, 1.0f, C2D_RGBA(90, 96, 104, 255));

    float rivet_offset = 6.0f;
    float rx[4] = { x + rivet_offset, x + w - rivet_offset, x + rivet_offset, x + w - rivet_offset };
    float ry[4] = { y + rivet_offset, y + rivet_offset, y + h - rivet_offset, y + h - rivet_offset };
    for (int i = 0; i < 4; i++) {
        ui_draw_circle(rx[i], ry[i], 2.5f, C2D_RGBA(110, 116, 124, 255));
        ui_draw_circle(rx[i] - 0.5f, ry[i] - 0.5f, 1.2f, C2D_RGBA(240, 240, 240, 255));
    }

    if (title && title[0]) {
        ui_draw_text_centered(x + (w / 2.0f), y + 12.0f, 0.65f, C2D_RGBA(20, 20, 20, 255), title);
    }
    if (subtitle && subtitle[0]) {
        ui_draw_text_centered(x + (w / 2.0f), y + 36.0f, 0.48f, C2D_RGBA(70, 75, 85, 255), subtitle);
    }
}

void ui_draw_icon_help(float x, float y, float size) {
    ui_draw_rect_gradient(x, y, size, size, C2D_RGBA(50, 140, 230, 255), C2D_RGBA(20, 90, 180, 255));
    ui_draw_line(x, y, x + size, y, 1.0f, C2D_RGBA(120, 190, 255, 255));
    ui_draw_line(x, y + size, x + size, y + size, 1.0f, C2D_RGBA(15, 60, 130, 255));
    ui_draw_line(x, y, x, y + size, 1.0f, C2D_RGBA(15, 60, 130, 255));
    ui_draw_line(x + size, y, x + size, y + size, 1.0f, C2D_RGBA(15, 60, 130, 255));
    ui_draw_text_centered(x + size / 2.0f, y + 2.0f, 0.48f, C2D_RGBA(255, 255, 255, 255), "?");
}

void ui_draw_icon_stats(float x, float y, float size) {
    float bar_w = size / 5.0f;
    ui_draw_rect(x, y + size * 0.6f, bar_w, size * 0.4f, C2D_RGBA(220, 60, 50, 255));
    ui_draw_rect(x + bar_w * 1.2f, y + size * 0.4f, bar_w, size * 0.6f, C2D_RGBA(230, 180, 20, 255));
    ui_draw_rect(x + bar_w * 2.4f, y + size * 0.1f, bar_w, size * 0.9f, C2D_RGBA(40, 180, 60, 255));
    ui_draw_rect(x + bar_w * 3.6f, y + size * 0.3f, bar_w, size * 0.7f, C2D_RGBA(40, 120, 220, 255));
}

void ui_draw_icon_gear(float x, float y, float size) {
    float cx = x + size / 2.0f;
    float cy = y + size / 2.0f;
    uint32_t clr = C2D_RGBA(36, 110, 200, 255);
    ui_draw_rect(cx - size * 0.5f, cy - 2.0f, size, 4.0f, clr);
    ui_draw_rect(cx - 2.0f, cy - size * 0.5f, 4.0f, size, clr);
    ui_draw_circle(cx, cy, size * 0.38f, clr);
    ui_draw_circle(cx, cy, size * 0.16f, C2D_RGBA(248, 248, 246, 255));
}

void ui_draw_nav_tab(float x, float y, float w, float h, const char* label, int icon_type, bool active) {
    if (active) {
        ui_draw_rect_gradient(x, y, w, h, C2D_RGBA(222, 237, 251, 255), C2D_RGBA(208, 227, 245, 255));
        ui_draw_line(x, y, x + w, y, 1.0f, C2D_RGBA(244, 249, 255, 255)); // top highlight
        ui_draw_line(x, y + h, x + w, y + h, 1.0f, C2D_RGBA(135, 170, 205, 255)); // bottom border
        ui_draw_line(x + w, y, x + w, y + h, 1.0f, C2D_RGBA(155, 185, 215, 255));
    } else {
        ui_draw_rect_gradient(x, y, w, h, C2D_RGBA(246, 246, 244, 255), C2D_RGBA(228, 228, 224, 255));
        ui_draw_line(x, y, x + w, y, 1.0f, C2D_RGBA(255, 255, 255, 255));
        ui_draw_line(x, y + h, x + w, y + h, 1.0f, C2D_RGBA(190, 190, 186, 255));
        ui_draw_line(x + w, y, x + w, y + h, 1.0f, C2D_RGBA(200, 200, 196, 255));
    }

    float icon_cx = x + (w / 2.0f);
    float icon_cy = y + 12.0f;
    if (icon_type == 0) {
        ui_draw_rect(icon_cx - 8, icon_cy - 7, 14, 10, C2D_RGBA(180, 110, 60, 255));
        ui_draw_rect(icon_cx - 6, icon_cy - 5, 14, 10, C2D_RGBA(220, 140, 80, 255));
    } else if (icon_type == 1) {
        ui_draw_rect(icon_cx - 2, icon_cy - 6, 4, 12, C2D_RGBA(40, 160, 60, 255));
        ui_draw_rect(icon_cx - 6, icon_cy - 2, 12, 4, C2D_RGBA(40, 160, 60, 255));
    } else if (icon_type == 2) {
        ui_draw_line(icon_cx - 5, icon_cy + 5, icon_cx + 5, icon_cy - 5, 3.0f, C2D_RGBA(180, 120, 40, 255));
        ui_draw_circle(icon_cx + 6, icon_cy - 6, 2.0f, C2D_RGBA(220, 60, 60, 255));
    } else if (icon_type == 3) {
        ui_draw_circle(icon_cx - 2, icon_cy - 2, 5.0f, C2D_RGBA(70, 120, 170, 255));
        ui_draw_circle(icon_cx - 2, icon_cy - 2, 3.0f, active ? C2D_RGBA(220, 140, 40, 255) : C2D_RGBA(240, 240, 238, 255));
        ui_draw_line(icon_cx + 1, icon_cy + 1, icon_cx + 6, icon_cy + 6, 2.0f, C2D_RGBA(70, 120, 170, 255));
    }

    uint32_t text_clr = active ? C2D_RGBA(20, 20, 20, 255) : C2D_RGBA(60, 60, 60, 255);
    ui_draw_text_centered(icon_cx, y + 25.0f, 0.45f, text_clr, label);
}

void ui_draw_deck_row(float y, float w, const char* name, int level, bool has_children, bool expanded,
                     uint32_t due_count, uint32_t new_count, uint32_t learn_count, bool selected) {
    float h = 24.0f;

    if (selected) {
        ui_draw_rect(0, y, w, h, C2D_RGBA(220, 236, 248, 255));
    } else {
        ui_draw_rect(0, y, w, h, C2D_RGBA(250, 250, 248, 255));
    }

    ui_draw_line(0, y + h, w, y + h, 1.0f, C2D_RGBA(224, 224, 220, 255));

    float indent = 8.0f + (float)level * 16.0f;

    if (has_children) {
        float btn_x = indent;
        float btn_y = y + 6.0f;
        ui_draw_rect(btn_x, btn_y, 11, 11, C2D_RGBA(180, 180, 180, 255));
        ui_draw_rect(btn_x + 1, btn_y + 1, 9, 9, C2D_RGBA(255, 255, 255, 255));
        ui_draw_rect(btn_x + 3, btn_y + 5, 5, 1, C2D_RGBA(40, 40, 40, 255));
        if (!expanded) {
            ui_draw_rect(btn_x + 5, btn_y + 3, 1, 5, C2D_RGBA(40, 40, 40, 255));
        }
        indent += 15.0f;
    }

    // Deck Name (with safe truncation if name is long)
    float max_name_w = (w - 126.0f) - indent;
    char disp_name[1024];
    size_t bytes = strlen(name);
    if (bytes > sizeof(disp_name) - 4) bytes = sizeof(disp_name) - 4;
    while (bytes && ((unsigned char)name[bytes] & 0xc0) == 0x80) bytes--;
    memcpy(disp_name, name, bytes);
    disp_name[bytes] = '\0';
    if (ui_get_text_width(0.52f, disp_name) > max_name_w) {
        size_t ends[1024], n = 0;
        for (size_t i = 0; i < bytes; i++) {
            if (((unsigned char)name[i] & 0xc0) != 0x80) ends[n++] = i;
        }
        size_t low = 0, high = n;
        while (low < high) {
            size_t mid = (low + high + 1) / 2;
            size_t cut = mid < n ? ends[mid] : bytes;
            memcpy(disp_name, name, cut);
            strcpy(disp_name + cut, "...");
            if (ui_get_text_width(0.52f, disp_name) <= max_name_w) low = mid;
            else high = mid - 1;
        }
        size_t cut = low < n ? ends[low] : bytes;
        memcpy(disp_name, name, cut); strcpy(disp_name + cut, "...");
    }

    uint32_t name_clr = selected ? C2D_RGBA(0, 40, 90, 255) : C2D_RGBA(20, 20, 20, 255);
    ui_draw_text(indent, y + 4.0f, 0.52f, name_clr, disp_name);

    // Fixed-column alignment for counts and chevron
    // Chevron '>'
    ui_draw_text_right(w - 8.0f, y + 4.0f, 0.50f, C2D_RGBA(150, 150, 150, 255), ">");

    char count_buf[16];
    snprintf(count_buf, sizeof(count_buf), "%u", (unsigned int)new_count);
    uint32_t new_clr = (new_count > 0) ? C2D_RGBA(20, 100, 200, 255) : C2D_RGBA(140, 140, 140, 255);
    ui_draw_text_right(w - 24.0f, y + 4.0f, 0.42f, new_clr, count_buf);

    snprintf(count_buf, sizeof(count_buf), "%u", (unsigned int)learn_count);
    ui_draw_text_right(w - 100.0f, y + 4.0f, 0.42f, C2D_RGBA(190, 55, 45, 255), count_buf);

    snprintf(count_buf, sizeof(count_buf), "%u", (unsigned int)due_count);
    uint32_t due_clr = (due_count > 0) ? C2D_RGBA(36, 130, 56, 255) : C2D_RGBA(140, 140, 140, 255);
    ui_draw_text_right(w - 62.0f, y + 4.0f, 0.42f, due_clr, count_buf);
}

void ui_draw_scrollbar(float x, float y, float w, float h, int total_items, int visible_items, int scroll_pos) {
    ui_draw_rect(x, y, w, h, C2D_RGBA(236, 236, 236, 255));
    ui_draw_line(x, y, x, y + h, 1.0f, C2D_RGBA(210, 210, 210, 255));

    ui_draw_rect(x, y, w, 12, C2D_RGBA(224, 224, 224, 255));
    ui_draw_triangle(x + 2, y + 9, x + w - 2, y + 9, x + w / 2.0f, y + 3, C2D_RGBA(80, 80, 80, 255));

    ui_draw_rect(x, y + h - 12, w, 12, C2D_RGBA(224, 224, 224, 255));
    ui_draw_triangle(x + 2, y + h - 9, x + w - 2, y + h - 9, x + w / 2.0f, y + h - 3, C2D_RGBA(80, 80, 80, 255));

    float track_h = h - 24.0f;
    if (total_items <= visible_items || total_items <= 0) {
        ui_draw_rect(x + 1, y + 12, w - 2, track_h, C2D_RGBA(180, 184, 192, 255));
    } else {
        float thumb_h = track_h * ((float)visible_items / (float)total_items);
        if (thumb_h < 12.0f) thumb_h = 12.0f;
        float max_scroll = (float)(total_items - visible_items);
        float scroll_ratio = (float)scroll_pos / max_scroll;
        if (scroll_ratio > 1.0f) scroll_ratio = 1.0f;
        float thumb_y = y + 12.0f + (track_h - thumb_h) * scroll_ratio;

        ui_draw_rect_gradient(x + 1, thumb_y, w - 2, thumb_h, C2D_RGBA(196, 200, 208, 255), C2D_RGBA(160, 166, 176, 255));
        ui_draw_line(x + 1, thumb_y, x + w - 1, thumb_y, 1.0f, C2D_RGBA(255, 255, 255, 200));
        ui_draw_line(x + 1, thumb_y + thumb_h, x + w - 1, thumb_y + thumb_h, 1.0f, C2D_RGBA(120, 126, 134, 255));
    }
}

void ui_draw_button(float x, float y, float w, float h, const char* text, const char* subtext, int style, bool pressed) {
    uint32_t top_clr, bot_clr, border_clr, shadow_clr;

    switch (style) {
        case 0: // Blue ("Añadir/Exportar" or "Estudiar")
            top_clr = pressed ? C2D_RGBA(30, 96, 180, 255) : C2D_RGBA(62, 146, 232, 255);
            bot_clr = pressed ? C2D_RGBA(20, 70, 140, 255) : C2D_RGBA(32, 110, 196, 255);
            border_clr = C2D_RGBA(18, 68, 136, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 35);
            break;
        case 1: // Amber/Orange ("Sincronizar")
            top_clr = pressed ? C2D_RGBA(200, 110, 14, 255) : C2D_RGBA(238, 140, 26, 255);
            bot_clr = pressed ? C2D_RGBA(160, 80, 8, 255) : C2D_RGBA(200, 106, 8, 255);
            border_clr = C2D_RGBA(140, 70, 6, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 35);
            break;
        case 2: // Red Rating ("Otra vez")
            top_clr = pressed ? C2D_RGBA(170, 40, 36, 255) : C2D_RGBA(216, 72, 66, 255);
            bot_clr = pressed ? C2D_RGBA(140, 28, 24, 255) : C2D_RGBA(182, 44, 38, 255);
            border_clr = C2D_RGBA(120, 24, 20, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 45);
            break;
        case 3: // Orange Rating ("Difícil")
            top_clr = pressed ? C2D_RGBA(196, 110, 16, 255) : C2D_RGBA(238, 144, 32, 255);
            bot_clr = pressed ? C2D_RGBA(160, 80, 8, 255) : C2D_RGBA(198, 108, 8, 255);
            border_clr = C2D_RGBA(140, 70, 6, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 45);
            break;
        case 4: // Green Rating ("Bien")
            top_clr = pressed ? C2D_RGBA(40, 140, 52, 255) : C2D_RGBA(56, 176, 72, 255);
            bot_clr = pressed ? C2D_RGBA(28, 110, 38, 255) : C2D_RGBA(36, 136, 48, 255);
            border_clr = C2D_RGBA(24, 96, 32, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 45);
            break;
        case 5: // Blue Rating ("Fácil")
            top_clr = pressed ? C2D_RGBA(28, 106, 196, 255) : C2D_RGBA(38, 136, 232, 255);
            bot_clr = pressed ? C2D_RGBA(18, 80, 156, 255) : C2D_RGBA(22, 100, 192, 255);
            border_clr = C2D_RGBA(16, 70, 140, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 45);
            break;
        default: // Neutral
            top_clr = pressed ? C2D_RGBA(210, 210, 206, 255) : C2D_RGBA(244, 244, 240, 255);
            bot_clr = pressed ? C2D_RGBA(190, 190, 186, 255) : C2D_RGBA(224, 224, 220, 255);
            border_clr = C2D_RGBA(170, 170, 166, 255);
            shadow_clr = C2D_RGBA(0, 0, 0, 25);
            break;
    }

    ui_draw_rect(x + 1, y + 2, w, h, shadow_clr);

    ui_draw_rect_gradient(x, y, w, h, top_clr, bot_clr);

    ui_draw_line(x + 1, y + 1, x + w - 1, y + 1, 1.0f, C2D_RGBA(255, 255, 255, 120)); // top inner highlight
    ui_draw_line(x, y, x + w, y, 1.0f, border_clr);
    ui_draw_line(x, y + h, x + w, y + h, 1.0f, border_clr);
    ui_draw_line(x, y, x, y + h, 1.0f, border_clr);
    ui_draw_line(x + w, y, x + w, y + h, 1.0f, border_clr);

    uint32_t text_clr = (style >= 0 && style <= 5) ? C2D_RGBA(255, 255, 255, 255) : C2D_RGBA(40, 40, 40, 255);
    if (subtext && subtext[0]) {
        ui_draw_text_centered(x + (w / 2.0f), y + 4.0f, 0.48f, text_clr, text);
        ui_draw_text_centered(x + (w / 2.0f), y + 19.0f, 0.42f, text_clr, subtext);
    } else {
        ui_draw_text_centered(x + (w / 2.0f), y + (h / 2.0f) - 6.0f, 0.50f, text_clr, text);
    }
}

int ui_keyboard(const char* hint, const char* initial, char* output, size_t capacity, bool secret, const char *cancel, const char *accept) {
    SwkbdState keyboard;
    swkbdInit(&keyboard, SWKBD_TYPE_NORMAL, 2, (int)(capacity / 4 - 1));
    if (secret) swkbdSetPasswordMode(&keyboard, SWKBD_PASSWORD_HIDE);
    swkbdSetHintText(&keyboard, hint);
    swkbdSetInitialText(&keyboard, initial);
    swkbdSetButton(&keyboard, SWKBD_BUTTON_LEFT, cancel, false);
    swkbdSetButton(&keyboard, SWKBD_BUTTON_RIGHT, accept, true);
    swkbdSetValidation(&keyboard, SWKBD_ANYTHING, 0, 0);
    SwkbdButton result = swkbdInputText(&keyboard, output, capacity);
    if (result == SWKBD_BUTTON_RIGHT) return 1;
    if (result == SWKBD_BUTTON_LEFT) return 0;
    return -1;
}

bool ui_wifi_connected(void) {
    static uint64_t next_check;
    static bool connected;
    uint64_t now = svcGetSystemTick();
    if (now < next_check) return connected;
    next_check = now + SYSCLOCK_ARM11;
    u32 status = 0;
    if (R_FAILED(acInit())) return osGetWifiStrength() != 0;
    Result result = ACU_GetStatus(&status);
    acExit();
    connected = R_SUCCEEDED(result) && status == 3;
    return connected;
}

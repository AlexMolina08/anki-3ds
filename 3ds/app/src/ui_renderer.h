// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#ifndef ANKI_3DS_UI_RENDERER_H
#define ANKI_3DS_UI_RENDERER_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TOP_WIDTH 400
#define TOP_HEIGHT 240
#define BOTTOM_WIDTH 320
#define BOTTOM_HEIGHT 240

#define C2D_RGBA(r, g, b, a) ((uint32_t)(r) | ((uint32_t)(g) << 8) | ((uint32_t)(b) << 16) | ((uint32_t)(a) << 24))

bool ui_init(void);
void ui_fini(void);
void ui_frame_begin(void);
void ui_frame_end(void);

void ui_target_top(uint32_t bg_color);
void ui_target_bottom(uint32_t bg_color);

void ui_draw_rect(float x, float y, float w, float h, uint32_t color);
void ui_draw_rect_gradient(float x, float y, float w, float h, uint32_t c_top, uint32_t c_bot);
void ui_draw_line(float x0, float y0, float x1, float y1, float thickness, uint32_t color);
void ui_draw_circle(float x, float y, float radius, uint32_t color);
void ui_draw_triangle(float x0, float y0, float x1, float y1, float x2, float y2, uint32_t color);

void ui_draw_text(float x, float y, float scale, uint32_t color, const char* str);
void ui_draw_text_centered(float x_center, float y, float scale, uint32_t color, const char* str);
void ui_draw_text_right(float x_right, float y, float scale, uint32_t color, const char* str);
float ui_get_text_width(float scale, const char* str);

void ui_draw_status_bar(float screen_w, const char* title);

void ui_draw_beveled_panel(float x, float y, float w, float h, float radius, uint32_t bg_color, uint32_t border_color);
void ui_draw_plaque(float x, float y, float w, float h, const char* title, const char* subtitle);
void ui_draw_nav_tab(float x, float y, float w, float h, const char* label, int icon_type, bool active);
void ui_draw_deck_row(float y, float w, const char* name, int level, bool has_children, bool expanded,
                     uint32_t due_count, uint32_t new_count, uint32_t learn_count, bool selected);
void ui_draw_scrollbar(float x, float y, float w, float h, int total_items, int visible_items, int scroll_pos);
void ui_draw_button(float x, float y, float w, float h, const char* text, const char* subtext, int style, bool pressed);

void ui_draw_icon_help(float x, float y, float size);
void ui_draw_icon_stats(float x, float y, float size);
void ui_draw_icon_gear(float x, float y, float size);

#ifdef __cplusplus
}
#endif

#endif // ANKI_3DS_UI_RENDERER_H

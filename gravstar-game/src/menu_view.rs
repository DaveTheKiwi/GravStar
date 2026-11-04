//! Drawing the menus, and working out which row the mouse is over.

use gravstar_core::constants::*;
use gravstar_core::menu::{MenuController, RowKind};
use gravstar_core::settings::Action;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::font::{Anchor, TextRenderer};

fn colour(c: [u8; 3]) -> Color {
    Color::RGB(c[0], c[1], c[2])
}

#[derive(Default)]
pub struct MenuView {
    /// (row number, where it was drawn) for the selectable rows, from the
    /// last draw
    row_rects: Vec<(usize, Rect)>,
}

impl MenuView {
    /// The row under a screen position, if any
    pub fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        self.row_rects.iter().find(|(_, r)| r.contains_point((x, y))).map(|(i, _)| *i)
    }

    /// Draw the open page. With a game behind it (`over_game`) the game is
    /// dimmed instead of covered, and the menu keeps to the part of the
    /// screen above the HUD strip (`free_height` pixels high).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                text: &mut TextRenderer, menus: &MenuController, size: (u32, u32),
                over_game: bool, free_height: u32) -> Result<(), String> {
        let rows = menus.rows();
        if rows.is_empty() {
            return Ok(());
        }
        let (width, height) = (size.0 as i32, size.1 as i32);
        let scale = height as f64 / REFERENCE_SCREEN_HEIGHT;
        let style = menus.style();
        // The lowest the menu may reach, as a fraction of the screen height
        let bottom = if over_game { free_height as f64 / height as f64 } else { 1.0 };

        if over_game {
            fill(canvas, Rect::new(0, 0, size.0, size.1), Color::RGBA(0, 0, 0, style.dim_alpha))?;
        } else {
            canvas.set_draw_color(colour(MENU_BACKGROUND));
            canvas.fill_rect(Rect::new(0, 0, size.0, size.1))?;
        }

        let title = menus.title();
        if !title.is_empty() {
            text.draw(canvas, tc, &title, (width / 2, (height as f64 * MENU_TITLE_Y) as i32),
                      MENU_TITLE_SIZE * scale, MENU_TITLE_COLOUR, Anchor::MidTop, true)?;
        }

        // The rows
        let count = rows.len().max(1) as f64;
        let top = height as f64 * MENU_ROWS_TOP;
        let space = height as f64 * (bottom - MENU_ROWS_BOTTOM_RESERVE - MENU_ROWS_TOP).max(0.1);
        let row_h = (MENU_ROW_HEIGHT * scale).min(space / count);
        let panel_w = (width as f64 * MENU_PANEL_WIDTH_FRACTION).min(MENU_PANEL_MAX_WIDTH * scale);
        let left = (width as f64 - panel_w) / 2.0;
        let font_size = row_h * MENU_ROW_TEXT_FRACTION;
        let pad = (MENU_ROW_PADDING * scale) as i32;
        // A page that is only a list of buttons looks better centred
        let centred = !rows.iter().any(|r| !r.value.is_empty());

        self.row_rects.clear();
        for (index, row) in rows.iter().enumerate() {
            let rect = Rect::new(left as i32, (top + index as f64 * row_h) as i32,
                                 panel_w as u32, row_h.max(1.0) as u32);
            if row.selectable() {
                self.row_rects.push((index, rect));
            }
            let selected = index == menus.selected() && row.selectable();
            if selected {
                canvas.set_draw_color(colour(MENU_SELECTED_COLOUR));
                canvas.fill_rect(rect)?;
                canvas.set_draw_color(colour(MENU_BORDER_COLOUR));
                canvas.draw_rect(rect)?;
                canvas.draw_rect(Rect::new(rect.x() + 1, rect.y() + 1, rect.width() - 2, rect.height() - 2))?;
            }
            let label_colour = match row.kind {
                RowKind::Header => MENU_HEADER_COLOUR,
                RowKind::Info => MENU_DIM_TEXT_COLOUR,
                _ => MENU_TEXT_COLOUR,
            };
            if centred && row.kind != RowKind::Info {
                text.draw(canvas, tc, &row.label, rect.center().into(), font_size, label_colour,
                          Anchor::Centre, true)?;
            } else {
                text.draw(canvas, tc, &row.label, (rect.left() + pad, rect.center().y()), font_size,
                          label_colour, Anchor::MidLeft, true)?;
            }
            if !row.value.is_empty() {
                let value = if selected && row.kind == RowKind::Choice {
                    format!("<  {}  >", row.value)
                } else {
                    row.value.clone()
                };
                text.draw(canvas, tc, &value, (rect.right() - pad, rect.center().y()), font_size,
                          MENU_VALUE_COLOUR, Anchor::MidRight, true)?;
            }
        }

        // Footer: any message, then the key hints
        if !menus.message().is_empty() {
            text.draw(canvas, tc, menus.message(), (width / 2, (height as f64 * (bottom - MENU_MESSAGE_FROM_BOTTOM)) as i32),
                      MENU_MESSAGE_SIZE * scale, MENU_WARNING_COLOUR, Anchor::MidTop, true)?;
        }
        text.draw(canvas, tc, MENU_HINTS, (width / 2, (height as f64 * (bottom - MENU_HINTS_FROM_BOTTOM)) as i32),
                  MENU_HINTS_SIZE * scale, MENU_DIM_TEXT_COLOUR, Anchor::MidTop, true)?;

        if let Some(capture) = menus.capture() {
            self.draw_capture_prompt(canvas, tc, text, menus, capture.action, capture.time_left, size)?;
        }
        Ok(())
    }

    /// The box that says "press a key" while a rebind waits
    #[allow(clippy::too_many_arguments)]
    fn draw_capture_prompt(&self, canvas: &mut Canvas<Window>, tc: &TextureCreator<WindowContext>,
                           text: &mut TextRenderer, menus: &MenuController, action: Action,
                           time_left: f64, size: (u32, u32)) -> Result<(), String> {
        let (width, height) = (size.0 as i32, size.1 as i32);
        let scale = height as f64 / REFERENCE_SCREEN_HEIGHT;
        fill(canvas, Rect::new(0, 0, size.0, size.1), Color::RGBA(0, 0, 0, MODAL_DIM_ALPHA))?;

        let (box_w, box_h) = ((width as f64 * MENU_MODAL_WIDTH_FRACTION) as u32, (MENU_MODAL_HEIGHT * scale) as u32);
        let modal = Rect::new(width / 2 - box_w as i32 / 2, height / 2 - box_h as i32 / 2, box_w, box_h);
        canvas.set_draw_color(colour(MENU_MODAL_COLOUR));
        canvas.fill_rect(modal)?;
        canvas.set_draw_color(colour(MENU_BORDER_COLOUR));
        for inset in 0..(3.0 * scale).max(2.0) as i32 {
            canvas.draw_rect(Rect::new(modal.x() + inset, modal.y() + inset,
                                       modal.width() - 2 * inset as u32, modal.height() - 2 * inset as u32))?;
        }
        text.draw(canvas, tc, &format!("Rebinding: {}", action.label()),
                  (modal.center().x(), modal.top() + (30.0 * scale) as i32), 46.0 * scale,
                  MENU_TITLE_COLOUR, Anchor::MidTop, true)?;
        text.draw(canvas, tc, &menus.capture_prompt(),
                  (modal.center().x(), modal.center().y() + (5.0 * scale) as i32), 44.0 * scale,
                  MENU_TEXT_COLOUR, Anchor::MidTop, true)?;
        text.draw(canvas, tc, &format!("Esc to cancel  ({}s)", (time_left + 0.99).max(0.0) as i32),
                  (modal.center().x(), modal.bottom() - (48.0 * scale) as i32), 28.0 * scale,
                  MENU_DIM_TEXT_COLOUR, Anchor::MidTop, true)?;
        Ok(())
    }
}

/// Fill a rectangle with a see-through colour
fn fill(canvas: &mut Canvas<Window>, rect: Rect, c: Color) -> Result<(), String> {
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(c);
    let result = canvas.fill_rect(rect);
    canvas.set_blend_mode(BlendMode::None);
    result
}

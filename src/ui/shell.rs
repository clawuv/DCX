//! App shell, after the Motrix layout: a navigation sidebar on the window
//! background and, to its right, one big rounded sheet every screen lives on.

use super::app::{App, Modal, Screen, Target};
use super::home::TITLEBAR_INSET;
use super::theme::{lerp_color, with_alpha};
use super::widgets::{self, bold, cr, Icon};
use crate::i18n::tr;
use crate::sound::Sfx;
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Ui, Vec2};
use std::sync::atomic::Ordering;

pub const SIDEBAR_W: f32 = 204.0;
/// Gap between the sheet and the window edges / the sidebar.
pub const INSET_M: f32 = 8.0;
/// Height of the Windows top toolbar: the undecorated window's title bar,
/// drawn on the sidebar colour so the two read as one chrome.
pub const TOOLBAR_H: f32 = 36.0;
/// Height of the floating top toolbar strip (`‹ ›` breadcrumb / rescan / gear).
/// Its vertical centre is where the macOS traffic lights are nudged down to, so
/// the whole row sits on one line.
pub const TOPBAR_H: f32 = 32.0;
/// x (window points) of the sidebar nav icon column's left edge: item inset
/// `12` + `14` pad + icon centred `9` into a `16px` box → `27`. The macOS
/// traffic lights are aligned to this so the header and the menu read as one
/// column. Tune this single number to slide the whole cluster left/right.
pub const NAV_ICON_LEFT: f32 = 27.0;

impl App {
    /// Draws the sidebar and the sheet; returns the rect the screens lay out in.
    pub fn shell_ui(&mut self, ui: &mut Ui, screen: Rect) -> Rect {
        let theme = self.theme.clone();
        let t = ui.ctx().animate_bool_with_time(Id::new("sidebar-open"), self.settings.sidebar, 0.2);
        let w = SIDEBAR_W * t;
        // the whole title-bar strip drags the window; screens draw their own
        // buttons after this, so those still get the clicks
        let drag_h = if cfg!(windows) { TOOLBAR_H } else { TITLEBAR_INSET + 44.0 };
        self.window_drag_area(ui, Rect::from_min_size(screen.min, Vec2::new(screen.width(), drag_h)));

        if w > 1.0 {
            let side = Rect::from_min_max(screen.min, Pos2::new(screen.left() + w, screen.bottom()));
            self.sidebar_ui(ui, side);
        }

        // Windows: the sheet starts below the toolbar strip; everywhere else
        // it reaches to the top of the window (only the sidebar column is
        // the title bar there).
        let sheet_top = if cfg!(windows) { TOOLBAR_H + INSET_M } else { INSET_M };
        let content = Rect::from_min_max(
            Pos2::new(screen.left() + w + INSET_M, screen.top() + sheet_top),
            Pos2::new(screen.right() - INSET_M, screen.bottom() - INSET_M),
        );
        {
            let p = ui.painter();
            widgets::shadow(p, content, 16.0, 0.45, &theme);
            p.rect_filled(content, cr(12.0), theme.inset_bg());
        }

        // title-bar buttons (toggle + new scan), on the same line as the
        // session top bar (`‹ ›` / rescan / gear live at the content sheet's
        // top strip, `bar_h = TOPBAR_H`), not the traffic lights' row above
        // it. Drawn after the sheet so they stay on top when the sidebar collapses.
        let (bx, by) = if cfg!(target_os = "macos") {
            (self.chrome.1 + 28.0, content.top() + TOPBAR_H / 2.0)
        } else if cfg!(windows) {
            (30.0, TOOLBAR_H / 2.0)
        } else {
            (30.0, content.top() + TOPBAR_H / 2.0)
        };
        let tog = Rect::from_center_size(Pos2::new(bx, by), Vec2::splat(28.0));
        if widgets::icon_button_bold(ui, tog, Id::new("sidebar-toggle"), Icon::Panel, &theme, true).clicked() {
            self.settings.sidebar = !self.settings.sidebar;
            self.settings.save();
        }
        let add = Rect::from_center_size(Pos2::new(bx + 34.0, by), Vec2::splat(28.0));
        self.mark("new-scan", add);
        if widgets::icon_button_bold(ui, add, Id::new("shell-add"), Icon::Plus, &theme, true).clicked() {
            self.pick_folder_and_scan();
        }
        // Windows: the toolbar strip is the title bar, so it carries the
        // caption controls; drawn after the drag area so they get its clicks.
        #[cfg(windows)]
        self.caption_buttons(ui, screen.right());
        content
    }

    /// Extra left padding for screen headers so they clear the floating
    /// title-bar buttons when the sidebar is hidden.
    pub fn shell_inset(&self) -> f32 {
        if self.settings.sidebar {
            0.0
        } else if cfg!(target_os = "macos") {
            self.chrome.1 + 60.0
        } else if cfg!(windows) {
            0.0 // the toolbar keeps the buttons off the sheet
        } else {
            80.0
        }
    }

    fn sidebar_ui(&mut self, ui: &mut Ui, side: Rect) {
        let theme = self.theme.clone();
        // No brand row on any platform (matches Windows): the menu starts
        // under the window controls. Give the macOS/Linux toolbar row (whose
        // buttons were nudged down onto `TOPBAR_H`) a clear gap below it.
        let mut y = side.top()
            + if cfg!(target_os = "macos") {
                TOPBAR_H + 26.0
            } else if cfg!(windows) {
                TOOLBAR_H + 12.0
            } else {
                TOPBAR_H + 26.0
            };

        // dashboard
        let item = Rect::from_min_size(Pos2::new(side.left() + 12.0, y), Vec2::new(side.width() - 16.0, 36.0));
        let active = self.screen == Screen::Home;
        let (clicked, _) = nav_item(ui, item, Id::new("nav-dash"), Icon::Grid, tr("Dashboard"), active, false, None, &theme);
        if clicked && !active {
            self.screen = Screen::Home;
            self.sfx(Sfx::BlipDown);
        }
        y += item.height() + 4.0;

        // open scans, under a section title
        y += item.height() + 14.0;
        let menu_ink = if theme.name == "Paper" { with_alpha(Color32::BLACK, 0.55) } else { theme.text_faint };
        ui.painter().text(Pos2::new(side.left() + 24.0, y), Align2::LEFT_TOP, tr("Recent scans"), bold(12.5), menu_ink);
        y += 22.0;
        let sessions: Vec<(usize, String, Icon, bool, Option<f32>)> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let icon = match &s.target {
                    Target::Volume(v) => if v.removable { Icon::External } else { Icon::Disk },
                    Target::Folder(_) => Icon::Folder,
                    Target::Ssh { .. } => Icon::Server,
                    Target::Rclone(_) => Icon::Cloud,
                };
                (i, s.title.clone(), icon, s.is_scanning(), s.progress.fraction())
            })
            .collect();
        for (i, title, icon, scanning, frac) in sessions {
            let item = Rect::from_min_size(Pos2::new(side.left() + 12.0, y), Vec2::new(side.width() - 16.0, 36.0));
            let active = self.screen == Screen::Session(i);
            let (clicked, close) = nav_item(ui, item, Id::new(("nav-sess", i)), icon, &title, active, true, if scanning { frac } else { None }, &theme);
            if close {
                self.cancel_session(i);
                return;
            }
            if clicked && !active {
                self.screen = Screen::Session(i);
                self.sfx(Sfx::Blip);
            }
            y += item.height() + 4.0;
        }

        // bottom: settings
        let item = Rect::from_min_size(Pos2::new(side.left() + 12.0, side.bottom() - 44.0), Vec2::new(side.width() - 16.0, 36.0));
        self.mark("settings-side", item);
        let (clicked, _) = nav_item(ui, item, Id::new("nav-settings"), Icon::Gear, tr("Settings"), false, false, None, &theme);
        if clicked {
            self.open_modal(Modal::Settings);
        }
    }

    /// Min / max / close for the undecorated Windows window, pinned to the
    /// right end of the toolbar. Like every other control here they are
    /// hand-drawn; close hovers in the danger colour, Windows-style.
    #[cfg(windows)]
    fn caption_buttons(&mut self, ui: &mut Ui, right: f32) {
        let theme = self.theme.clone();
        let maxed = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let h = TOOLBAR_H;
        // keep the invisible resize frame at the window's right edge clear
        let edge = super::win::resize_border();
        let mut rect = Rect::from_min_size(Pos2::new(right - edge - 46.0, 0.0), Vec2::new(46.0, h));
        if caption_button(ui, rect, Id::new("cap-close"), Cap::Close, &theme).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        rect = rect.translate(Vec2::new(-46.0, 0.0));
        if caption_button(ui, rect, Id::new("cap-max"), if maxed { Cap::Restore } else { Cap::Max }, &theme).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }
        rect = rect.translate(Vec2::new(-46.0, 0.0));
        if caption_button(ui, rect, Id::new("cap-min"), Cap::Min, &theme).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }
}

/// The caption glyphs, drawn by hand like every other icon.
#[cfg(windows)]
#[derive(Clone, Copy, PartialEq)]
enum Cap {
    Min,
    Max,
    Restore,
    Close,
}

#[cfg(windows)]
fn caption_button(ui: &mut Ui, rect: Rect, id: Id, glyph: Cap, theme: &super::theme::Theme) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.08);
    let p = ui.painter();
    let danger = glyph == Cap::Close;
    if h > 0.0 {
        let bg = if danger {
            lerp_color(Color32::TRANSPARENT, theme.danger, h)
        } else {
            with_alpha(theme.text, 0.08 * h)
        };
        p.rect_filled(rect, 0.0, bg);
    }
    let col = if danger && h > 0.35 { Color32::WHITE } else { lerp_color(theme.text_dim, theme.text, h) };
    let st = Stroke::new(1.6, col);
    let c = rect.center();
    let s = 10.0;
    match glyph {
        Cap::Min => {
            p.line_segment([Pos2::new(c.x - s / 2.0, c.y + 1.0), Pos2::new(c.x + s / 2.0, c.y + 1.0)], st);
        }
        Cap::Max => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::splat(s)), 1.0, st, egui::StrokeKind::Inside);
        }
        Cap::Restore => {
            let d = 3.5;
            p.rect_stroke(Rect::from_center_size(c + Vec2::new(d, -d), Vec2::splat(s - 1.0)), 1.0, st, egui::StrokeKind::Inside);
            p.rect_stroke(Rect::from_center_size(c - Vec2::new(d, -d) / 2.0, Vec2::splat(s - 1.0)), 1.0, st, egui::StrokeKind::Inside);
        }
        Cap::Close => {
            let d = 0.42 * s;
            p.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], st);
            p.line_segment([c + Vec2::new(d, -d), c + Vec2::new(-d, d)], st);
        }
    }
    resp
}

/// One sidebar row. Returns (clicked, close-clicked).
fn nav_item(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    icon: Icon,
    label: &str,
    active: bool,
    closeable: bool,
    progress: Option<f32>,
    theme: &super::theme::Theme,
) -> (bool, bool) {
    let resp = ui.interact(rect, id, Sense::click());
    let hov = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered() && !active, 0.12);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = ui.painter().clone();
    // selected and hover share the same pill; the label colour tells them apart
    let fill = if active { 1.0 } else { hov };
    if fill > 0.0 {
        p.rect_filled(rect, cr(8.0), with_alpha(theme.text, 0.05 * fill));
    }
    let col = if theme.name == "Paper" { Color32::BLACK } else if active { theme.text } else { lerp_color(theme.text_dim, theme.text, hov) };
    let mut tx = rect.left() + 14.0;
    // icon sized to the 13px label so both read as one line
    widgets::draw_icon(&p, icon, Rect::from_center_size(Pos2::new(tx + 9.0, rect.center().y), Vec2::splat(16.0)), col);
    tx += 34.0;
    let max_w = rect.right() - tx - if closeable { 34.0 } else { 12.0 };
    let lbl = widgets::truncate(&p, label, &bold(13.0), max_w);
    // -0.5: half-step optical lift; CJK ink hangs slightly low in the line box
    p.text(Pos2::new(tx, rect.center().y + 1.0), Align2::LEFT_CENTER, lbl, bold(13.0), col);
    let mut close = false;
    if closeable && resp.hovered() {
        let x = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::splat(22.0));
        if widgets::icon_button(ui, x, id.with("x"), Icon::Close, theme, true).clicked() {
            close = true;
        }
    }
    if let Some(f) = progress {
        let bar = Rect::from_min_size(Pos2::new(rect.left() + 14.0, rect.bottom() - 3.0), Vec2::new((rect.width() - 28.0) * f.clamp(0.02, 1.0), 2.0));
        p.rect_filled(Rect::from_min_size(Pos2::new(rect.left() + 14.0, rect.bottom() - 3.0), Vec2::new(rect.width() - 28.0, 2.0)), cr(1.0), with_alpha(theme.stroke, 0.5));
        p.rect_filled(bar, cr(1.0), theme.accent);
    }
    (resp.clicked(), close)
}

impl App {
    /// The dashboard "new scan" action, shared by the sidebar and the home tiles.
    pub fn pick_folder_and_scan(&mut self) {
        if let Some(p) = rfd::FileDialog::new().set_title(tr("Choose a folder to scan")).pick_folder() {
            self.start_session(Target::Folder(p));
        }
    }

    /// Files counted across all running scans, for the dashboard tile.
    pub fn scanning_files(&self) -> u64 {
        self.sessions.iter().filter(|s| s.is_scanning()).map(|s| s.progress.files.load(Ordering::Relaxed)).sum()
    }
}

//! App shell, after the Motrix layout: a navigation sidebar on the window
//! background and, to its right, one big rounded sheet every screen lives on.

use super::app::{App, Modal, Screen, Target};
use super::home::TITLEBAR_INSET;
use super::theme::{lerp_color, with_alpha};
use super::widgets::{self, bold, brand, cr, Icon};
use crate::i18n::tr;
use crate::sound::Sfx;
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Ui, Vec2};
use std::sync::atomic::Ordering;

pub const SIDEBAR_W: f32 = 204.0;
/// Gap between the sheet and the window edges / the sidebar.
pub const INSET_M: f32 = 8.0;

impl App {
    /// Draws the sidebar and the sheet; returns the rect the screens lay out in.
    pub fn shell_ui(&mut self, ui: &mut Ui, screen: Rect) -> Rect {
        let theme = self.theme.clone();
        let t = ui.ctx().animate_bool_with_time(Id::new("sidebar-open"), self.settings.sidebar, 0.2);
        let w = SIDEBAR_W * t;
        // the whole title-bar strip drags the window; screens draw their own
        // buttons after this, so those still get the clicks
        self.window_drag_area(ui, Rect::from_min_size(screen.min, Vec2::new(screen.width(), TITLEBAR_INSET + 44.0)));

        if w > 1.0 {
            let side = Rect::from_min_max(screen.min, Pos2::new(screen.left() + w, screen.bottom()));
            self.sidebar_ui(ui, side);
        }

        let content = Rect::from_min_max(
            Pos2::new(screen.left() + w + INSET_M, screen.top() + INSET_M),
            Pos2::new(screen.right() - INSET_M, screen.bottom() - INSET_M),
        );
        {
            let p = ui.painter();
            widgets::shadow(p, content, 16.0, 0.45, &theme);
            p.rect_filled(content, cr(12.0), theme.inset_bg());
        }

        // title-bar buttons just right of the traffic lights (toggle + new
        // scan); drawn after the sheet so they stay on top when the sidebar
        // is collapsed and the sheet slides beneath them
        let (bx, by) = if cfg!(target_os = "macos") {
            (self.chrome.1 + 28.0, self.chrome.0)
        } else {
            (30.0, 22.0)
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
        content
    }

    /// Extra left padding for screen headers so they clear the floating
    /// title-bar buttons when the sidebar is hidden.
    pub fn shell_inset(&self) -> f32 {
        if self.settings.sidebar {
            0.0
        } else if cfg!(target_os = "macos") {
            self.chrome.1 + 60.0
        } else {
            80.0
        }
    }

    fn sidebar_ui(&mut self, ui: &mut Ui, side: Rect) {
        let theme = self.theme.clone();
        let mut y = side.top() + if cfg!(target_os = "macos") { 46.0 } else { 16.0 };

        // brand
        {
            let logo = Rect::from_min_size(Pos2::new(side.left() + 18.0, y), Vec2::splat(28.0));
            let tex = self.logo.get_or_insert_with(|| {
                let px = 184;
                let pm = crate::icon::render(px as u32, crate::icon::Frame::IDLE, false);
                let img = egui::ColorImage::from_rgba_unmultiplied([px, px], &crate::icon::rgba(&pm));
                ui.ctx().load_texture("app-icon", img, egui::TextureOptions::LINEAR)
            });
            let p = ui.painter();
            p.image(tex.id(), logo, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            p.text(Pos2::new(side.left() + 54.0, y + 14.0), Align2::LEFT_CENTER, "SquirrelDisk", brand(15.0), theme.text);
            y += 46.0;
        }

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

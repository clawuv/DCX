//! Home screen, laid out as a dashboard (Motrix-style tiles): a row of stat
//! tiles, then cards for the disks, recent scans and other sources.

use super::app::{App, Modal, RcloneState, Screen, Target};
use super::shell::INSET_M;
use super::theme::{lerp_color, with_alpha};
use super::widgets::{self, bold, cr, display, font, BtnStyle, Icon};
use crate::i18n::{tr, trf};
use crate::tree::fmt_size;
use crate::update::State as UpState;
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::sync::atomic::Ordering;

pub const TITLEBAR_INSET: f32 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

impl App {
    /// Header shared by all screens; returns the remaining rect below it.
    pub fn window_drag_area(&self, ui: &mut Ui, rect: Rect) {
        let resp = ui.interact(rect, Id::new("window-drag").with(rect.min.x as i32), Sense::click_and_drag());
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
    }

    pub fn home_ui(&mut self, ui: &mut Ui, content: Rect) {
        let theme = self.theme.clone();
        // (the shell already makes the whole title-bar strip drag the window)

        let w = content.width() - 48.0;
        let x0 = content.left() + 24.0;
        // The page scrolls when the window is shorter than the content.
        let max_scroll = (self.home_content_h - content.height()).max(0.0);
        if self.modal.is_none() && ui.rect_contains_pointer(content) {
            self.home_scroll -= ui.input(|i| i.smooth_scroll_delta.y);
        }
        self.home_scroll = self.home_scroll.clamp(0.0, max_scroll);
        let page_top = content.top() - self.home_scroll;
        // when the sidebar is collapsed the floating title-bar buttons sit
        // over the sheet's top-left corner: start the header below them
        let mut y = page_top + if self.settings.sidebar { 22.0 } else { 48.0 };

        // --- title row
        {
            let p = ui.painter();
            p.text(Pos2::new(x0, y), Align2::LEFT_TOP, tr("Dashboard"), display(22.0), theme.text);
        }
        let gear = Rect::from_center_size(Pos2::new(x0 + w - 16.0, y + 16.0), Vec2::splat(32.0));
        self.mark("settings", gear);
        if widgets::icon_button(ui, gear, Id::new("settings"), Icon::Gear, &theme, true).clicked() {
            self.open_modal(Modal::Settings);
        }
        self.update_pill(ui, content, gear.left() - 8.0);
        y += 52.0;

        // --- stat tiles
        let gap = 14.0;
        let tw = (w - gap * 3.0) / 4.0;
        let volumes = self.volumes.clone();
        let used: u64 = volumes.iter().map(|v| v.used()).sum();
        let avail: u64 = volumes.iter().map(|v| v.available).sum();
        let scanning = self.sessions.iter().filter(|s| s.is_scanning()).count();
        let files = self.scanning_files();
        let boot = volumes.iter().find(|v| v.is_boot);
        let tiles: [(String, String, String, Icon, Color32); 4] = [
            (
                tr("Disks").to_string(),
                volumes.len().to_string(),
                boot.map(|v| v.name.clone()).unwrap_or_else(|| tr("No disks found").to_string()),
                Icon::Disk,
                theme.accent,
            ),
            (
                tr("Scanning").to_string(),
                if scanning > 0 { scanning.to_string() } else { tr("Idle").to_string() },
                if scanning > 0 { trf("{files} files found", &[("files", &crate::tree::fmt_count_compact(files))]) } else { tr("Nothing is being scanned").to_string() },
                Icon::Bolt,
                theme.warn,
            ),
            (tr("Used space").to_string(), fmt_size(used), tr("across all disks").to_string(), Icon::Grid, theme.accent2),
            (tr("Free space").to_string(), fmt_size(avail), tr("across all disks").to_string(), Icon::Check, theme.ok),
        ];
        for (k, (label, value, sub, icon, accent)) in tiles.into_iter().enumerate() {
            let rect = Rect::from_min_size(Pos2::new(x0 + k as f32 * (tw + gap), y), Vec2::new(tw, 108.0));
            dash_tile(ui, rect, &label, &value, &sub, icon, accent, &theme);
        }
        y += 122.0;

        // --- disks card
        {
            let rows = volumes.len().max(1) as f32;
            let card = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 46.0 + rows * 56.0 + 8.0));
            card_bg(ui, card, &theme);
            ui.painter().text(card.left_top() + Vec2::new(18.0, 22.0), Align2::LEFT_CENTER, tr("Disks"), bold(12.5), theme.text_dim);
            for (i, v) in volumes.iter().enumerate() {
                let row = Rect::from_min_size(Pos2::new(card.left() + 10.0, card.top() + 40.0 + i as f32 * 56.0), Vec2::new(card.width() - 20.0, 54.0));
                self.volume_row(ui, row, v);
            }
            if volumes.is_empty() {
                ui.painter().text(card.center() + Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, tr("No disks found"), font(13.0), theme.text_faint);
            }
            y += card.height() + 16.0;
        }

        // --- scan something else
        let rclone_state = self.rclone.lock().unwrap().clone();
        let rclone_sub = match &rclone_state {
            RcloneState::Looking => tr("Looking for rclone…").to_string(),
            RcloneState::Missing => tr("S3, Drive, FTP… (needs rclone)").to_string(),
            RcloneState::Ready(v) if v.is_empty() => tr("Add Google Drive, S3, Dropbox…").to_string(),
            RcloneState::Ready(v) => if v.len() == 1 { tr("1 remote configured").to_string() } else { trf("{n} remotes configured", &[("n", &v.len())]) },
        };
        let cards = [
            (Icon::Folder, tr("A folder"), tr("Pick any folder, or drop it here").to_string(), 0usize),
            (Icon::Server, tr("Remote server"), tr("Any machine you can SSH into").to_string(), 1),
            (Icon::Cloud, tr("Cloud storage"), rclone_sub, 2),
        ];
        let cw = (w - gap * 2.0) / 3.0;
        for (k, (icon, title, sub, action)) in cards.into_iter().enumerate() {
            let rect = Rect::from_min_size(Pos2::new(x0 + k as f32 * (cw + gap), y), Vec2::new(cw, 104.0));
            if source_card(ui, rect, Id::new(("src", k)), icon, title, &sub, &theme).clicked() {
                match action {
                    0 => self.pick_folder_and_scan(),
                    1 => {
                        let host = self.settings.ssh_history.first().cloned().unwrap_or_default();
                        self.open_modal(Modal::Ssh { host, path: "/".into() });
                    }
                    _ => self.open_modal(Modal::Rclone { path: String::new(), confirm_remove: None }),
                }
            }
        }
        y += 118.0;

        let ad_rect = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 52.0));
        self.sponsor_banner(ui, ad_rect, "home");
        y += 64.0;

        let p = ui.painter();
        let foot = format!("v{} · {}", env!("CARGO_PKG_VERSION"), tr("drop a folder anywhere to scan it · Esc goes back"));
        let foot_y = (y + 20.0).max(content.bottom() - 26.0 - self.home_scroll);
        p.text(Pos2::new(content.center().x, foot_y), Align2::CENTER_CENTER, foot, font(11.5), theme.text_faint);
        self.home_content_h = y + 44.0 - page_top;
        if max_scroll > 0.0 {
            let track = Rect::from_min_max(Pos2::new(content.right() - INSET_M - 5.0, content.top() + 8.0), Pos2::new(content.right() - INSET_M, content.bottom() - 8.0));
            let thumb_h = (track.height() * content.height() / self.home_content_h).max(32.0);
            let t = self.home_scroll / max_scroll;
            let thumb = Rect::from_min_size(Pos2::new(track.left(), track.top() + (track.height() - thumb_h) * t), Vec2::new(track.width(), thumb_h));
            let resp = ui.interact(thumb.expand2(Vec2::new(4.0, 0.0)), Id::new("home-scroll"), Sense::drag());
            if resp.dragged() {
                self.home_scroll = (self.home_scroll + resp.drag_delta().y * max_scroll / (track.height() - thumb_h).max(1.0)).clamp(0.0, max_scroll);
            }
            let h = ui.ctx().animate_bool_with_time(Id::new("home-scroll-h"), resp.hovered() || resp.dragged(), 0.12);
            let p = ui.painter();
            p.rect_filled(track, cr(3.0), with_alpha(theme.stroke, 0.35));
            p.rect_filled(thumb, cr(3.0), lerp_color(with_alpha(theme.text_faint, 0.8), theme.text_dim, h));
        }
    }

    /// The update button, as a ribbon hanging from the top edge of the
    /// window: it slides down when it appears. `right` is its right edge.
    pub fn update_pill(&mut self, ui: &mut Ui, screen: Rect, right: f32) {
        let theme = self.theme.clone();
        let state = self.updater.state();
        let id = Id::new("update-pill");
        let (label, style, icon) = match &state {
            UpState::Available(r) => (trf("Update to {version}", &[("version", &r.version)]), BtnStyle::Primary, Icon::Download),
            UpState::Downloading => {
                let d = self.updater.downloaded.load(Ordering::Relaxed);
                let tot = self.updater.total.load(Ordering::Relaxed).max(1);
                (trf("Downloading {percent}%", &[("percent", &(d * 100 / tot))]), BtnStyle::Subtle, Icon::Download)
            }
            UpState::Ready(_) => (tr("Restart to update").to_string(), BtnStyle::Primary, Icon::Refresh),
            _ => {
                ui.ctx().animate_bool_with_time(id.with("in"), false, 0.0);
                return;
            }
        };
        let shown = super::sunburst::ease(ui.ctx().animate_bool_with_time(id.with("in"), true, 0.45));
        let (w, h) = (ui.painter().layout_no_wrap(label.clone(), bold(13.0), Color32::WHITE).size().x + 50.0, 30.0);
        let rect = Rect::from_min_size(Pos2::new(right - w, screen.top() - h * (1.0 - shown)), Vec2::new(w, h));
        let corners = egui::CornerRadius { nw: 0, ne: 0, sw: 9, se: 9 };
        if widgets::button_shaped(ui, rect, id, &label, Some(icon), style, &theme, true, corners).clicked() {
            match state {
                UpState::Available(r) => self.updater.install(r, ui.ctx().clone()),
                UpState::Ready(_) => crate::update::restart(),
                _ => {}
            }
        }
    }

    /// One disk inside the dashboard "Disks" card.
    fn volume_row(&mut self, ui: &mut Ui, rect: Rect, v: &crate::disks::Volume) {
        let theme = self.theme.clone();
        let id = Id::new(("vol", &v.mount));
        let resp = ui.interact(rect, id, Sense::click());
        let h = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.15);
        let session = self.sessions.iter().position(|s| matches!(&s.target, Target::Volume(x) if x.mount == v.mount));
        let p = ui.painter();
        if h > 0.0 {
            p.rect_filled(rect, cr(10.0), with_alpha(theme.surface_hi, h * 0.7));
        }
        // icon tile
        let tile = Rect::from_center_size(Pos2::new(rect.left() + 32.0, rect.center().y), Vec2::splat(36.0));
        let tc = if v.is_boot { theme.accent } else if v.removable { theme.warn } else { theme.accent2 };
        p.rect_filled(tile, cr(8.0), with_alpha(tc, if theme.dark { 0.35 } else { 0.18 }));
        widgets::draw_icon(p, if v.removable { Icon::External } else { Icon::Disk }, tile.shrink(9.0), tc);
        // texts
        p.text(Pos2::new(tile.right() + 12.0, rect.center().y - 9.0), Align2::LEFT_CENTER, &v.name, bold(13.5), theme.text);
        let kind = if v.is_boot { tr("startup disk") } else if v.removable { tr("removable") } else { tr("volume") };
        let sub = format!("{} · {} · {}", fmt_size(v.total), kind, v.fs);
        let sub = widgets::truncate(p, &sub, &font(11.5), rect.width() * 0.4);
        p.text(Pos2::new(tile.right() + 12.0, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, font(11.5), theme.text_dim);

        // right side: bar + button(s)
        let btn = Rect::from_min_size(Pos2::new(rect.right() - 102.0, rect.center().y - 15.0), Vec2::new(90.0, 30.0));
        let bar = Rect::from_min_size(Pos2::new(rect.right() - 370.0, rect.center().y - 9.0), Vec2::new(180.0, 8.0));
        let p = ui.painter().clone();
        match session.map(|s| &self.sessions[s]) {
            Some(s) if s.is_scanning() => {
                let frac = s.progress.fraction();
                widgets::progress_bar(&p, bar, frac, &theme, self.time, None);
                let txt = match frac {
                    Some(f) => trf("scanning · {percent}%", &[("percent", &format!("{:.0}", f * 100.0))]),
                    None => tr("scanning").to_string(),
                };
                p.text(Pos2::new(bar.left(), bar.bottom() + 10.0), Align2::LEFT_CENTER, txt, font(11.0), theme.text_dim);
                if widgets::button(ui, btn, id.with("view"), tr("Watch"), Some(Icon::Eye), BtnStyle::Subtle, &theme).clicked() {
                    self.screen = Screen::Session(session.unwrap());
                }
                let x = Rect::from_center_size(Pos2::new(btn.left() - 20.0, btn.center().y), Vec2::splat(26.0));
                if widgets::icon_button(ui, x, id.with("cancel"), Icon::Close, &theme, true).clicked() {
                    self.cancel_session(session.unwrap());
                }
            }
            other => {
                let used = v.used_frac();
                let col = if used > 0.9 { theme.danger } else if used > 0.75 { theme.warn } else { theme.ok };
                widgets::progress_bar(&p, bar, Some(used), &theme, 0.25, Some(col));
                p.text(Pos2::new(bar.right(), bar.bottom() + 10.0), Align2::RIGHT_CENTER, trf("{size} free", &[("size", &fmt_size(v.available))]), bold(11.0), col);
                p.text(Pos2::new(bar.left(), bar.bottom() + 10.0), Align2::LEFT_CENTER, trf("{size} used", &[("size", &fmt_size(v.used()))]), font(11.0), theme.text_faint);
                if other.is_some() {
                    if widgets::button(ui, btn, id.with("view"), tr("View"), Some(Icon::Eye), BtnStyle::Primary, &theme).clicked() || resp.clicked() {
                        self.screen = Screen::Session(session.unwrap());
                        self.sfx(crate::sound::Sfx::Blip);
                    }
                    let rr = Rect::from_center_size(Pos2::new(btn.left() - 20.0, btn.center().y), Vec2::splat(26.0));
                    if widgets::icon_button(ui, rr, id.with("rescan"), Icon::Refresh, &theme, true).clicked() {
                        self.start_session(Target::Volume(v.clone()));
                    }
                } else if {
                    self.mark(format!("scan:{}", v.mount), btn);
                    widgets::button(ui, btn, id.with("scan"), tr("Scan"), Some(Icon::Bolt), BtnStyle::Primary, &theme).clicked() || resp.clicked()
                } {
                    self.start_session(Target::Volume(v.clone()));
                }
            }
        }
    }
}

/// One stat tile: label top-left, big value under it, a coloured underline
/// at the bottom edge (like the Motrix dashboard tiles).
fn dash_tile(ui: &mut Ui, rect: Rect, label: &str, value: &str, sub: &str, icon: Icon, accent: Color32, theme: &super::theme::Theme) {
    let p = ui.painter();
    card_bg(ui, rect, theme);
    p.text(rect.left_top() + Vec2::new(16.0, 22.0), Align2::LEFT_CENTER, label, font(12.0), theme.text_dim);
    widgets::draw_icon(p, icon, Rect::from_center_size(Pos2::new(rect.right() - 24.0, rect.top() + 22.0), Vec2::splat(16.0)), theme.text_faint);
    let g = p.layout_no_wrap(value.to_string(), display(24.0), theme.text);
    let fs = if g.size().x > rect.width() - 32.0 { 24.0 * (rect.width() - 32.0) / g.size().x } else { 24.0 };
    p.text(Pos2::new(rect.left() + 16.0, rect.top() + 54.0), Align2::LEFT_CENTER, value, display(fs), theme.text);
    let sub = widgets::truncate(p, sub, &font(11.0), rect.width() - 32.0);
    p.text(Pos2::new(rect.left() + 16.0, rect.top() + 80.0), Align2::LEFT_CENTER, sub, font(11.0), theme.text_faint);
    let line = Rect::from_min_size(Pos2::new(rect.left() + 14.0, rect.bottom() - 12.0), Vec2::new(rect.width() - 28.0, 4.0));
    p.rect_filled(line, cr(2.0), with_alpha(theme.stroke, 0.5));
    p.rect_filled(Rect::from_min_size(line.min, Vec2::new(line.width() * 0.35, 4.0)), cr(2.0), accent);
}

/// Card chrome shared by dashboard tiles and cards.
pub fn card_bg(ui: &Ui, rect: Rect, theme: &super::theme::Theme) {
    let p = ui.painter();
    widgets::shadow(p, rect, 8.0, 0.35, theme);
    p.rect_filled(rect, cr(10.0), theme.surface);
    p.rect_stroke(rect, cr(10.0), Stroke::new(1.0, with_alpha(theme.stroke, 0.55)), egui::StrokeKind::Inside);
}

fn source_card(ui: &mut Ui, rect: Rect, id: Id, icon: Icon, title: &str, sub: &str, theme: &super::theme::Theme) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool_with_time(id.with("h"), resp.hovered(), 0.15);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let p = ui.painter();
    let r = rect;

    card_bg(ui, r, theme);
    p.rect_filled(r, cr(10.0), lerp_color(Color32::TRANSPARENT, theme.surface_hi, h * 0.5));
    p.rect_stroke(r, cr(10.0), Stroke::new(1.0, lerp_color(with_alpha(theme.stroke, 0.55), theme.accent, h * 0.7)), egui::StrokeKind::Inside);
    let ic = Pos2::new(r.left() + 30.0, r.top() + 32.0);
    p.rect_filled(Rect::from_center_size(ic, Vec2::splat(34.0)), cr(8.0), with_alpha(theme.accent, 0.18 + 0.1 * h));
    widgets::draw_icon(p, icon, Rect::from_center_size(ic, Vec2::splat(20.0)), lerp_color(theme.accent, theme.text, h * 0.5));
    p.text(Pos2::new(r.left() + 16.0, r.top() + 66.0), Align2::LEFT_CENTER, title, bold(14.0), theme.text);
    let s = widgets::truncate(p, sub, &font(11.5), r.width() - 28.0);
    p.text(Pos2::new(r.left() + 16.0, r.top() + 86.0), Align2::LEFT_CENTER, s, font(11.5), theme.text_dim);
    resp
}

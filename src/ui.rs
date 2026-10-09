//! Window layout (compact): header bar, summary, interface details, planned commands,
//! actions, collapsible saved originals and activity, and a status bar.

use eframe::egui;

use crate::net::{self, Family, Interface};
use crate::theme;
use crate::{App, DESCRIPTION, IPV6_MIN_MTU, LogEntry, MTU_CHOICES, SUMMARY};

/// Families whose MTU differs from `target`, i.e. what Apply would change.
/// IPv6 is skipped below its 1280-byte minimum, which Windows would reject.
pub fn planned_changes(iface: &Interface, target: u32) -> Vec<(Family, u32)> {
    [(Family::V4, iface.mtu_v4), (Family::V6, iface.mtu_v6)]
        .into_iter()
        .filter(|&(family, _)| family == Family::V4 || target >= IPV6_MIN_MTU)
        .filter(|(_, mtu)| mtu.is_some_and(|m| m != target))
        .map(|(f, _)| (f, target))
        .collect()
}

pub fn local_time() -> String {
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
    format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
}

fn mtu_cell(mtu: Option<u32>) -> String {
    mtu.map_or("—".to_owned(), |m| m.to_string())
}

/// Bordered container used for every section body.
fn panel(fill: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, theme::p().border))
        .corner_radius(theme::RADIUS)
        .inner_margin(egui::Margin::symmetric(12, 10))
}

/// Resizes the window so the content fits exactly, with no scrollbar.
/// Content height changes as text wraps or the activity log grows.
/// `bottom` is where the central area really ends (the panel's own rect grows
/// with its content, so it can't tell us about overflow).
fn fit_height_to_content(ui: &egui::Ui, ctx: &egui::Context, bottom: f32) {
    let spare = bottom - ui.cursor().top();
    if spare.abs() < 1.0 {
        return;
    }
    if let Some(inner) = ctx.input(|i| i.viewport().inner_rect) {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            inner.width(),
            inner.height() - spare,
        )));
    }
}

/// Sun / moon segmented control. Laid out right to left, so moon comes first.
fn theme_toggle(ui: &mut egui::Ui) {
    let dark = theme::is_dark();
    let c = theme::p();
    let mut pick = None;
    ui.spacing_mut().item_spacing.x = 2.0;
    for (icon, is_dark, tip) in [("🌙", true, "Dark mode"), ("☀", false, "Light mode")] {
        let active = dark == is_dark;
        let button = egui::Button::new(
            egui::RichText::new(icon).size(14.0).color(if active { c.text } else { c.muted }),
        )
        .fill(if active { c.panel } else { egui::Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0, if active { c.border } else { egui::Color32::TRANSPARENT }))
        .min_size(egui::vec2(30.0, 26.0));
        if ui.add(button).on_hover_text(tip).clicked() && !active {
            pick = Some(is_dark);
        }
    }
    if let Some(dark) = pick {
        theme::set_mode(ui.ctx(), dark);
        crate::settings::Settings { dark }.save();
    }
}

fn key(ui: &mut egui::Ui, k: &str) {
    ui.label(egui::RichText::new(k).color(theme::p().muted).size(12.0));
}

/// One-line collapsible row (triangle, title, summary on the right), clickable
/// across its full width. Returns whether it's open.
fn fold(ui: &mut egui::Ui, open: &mut bool, title: &str, summary: &str) -> bool {
    let c = theme::p();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    if response.clicked() {
        *open = !*open;
    }
    let painter = ui.painter();
    painter.hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, c.border));
    let text_color = if response.hovered() { c.accent } else { c.text };

    // Triangle pointing right (closed) or down (open). The glyphs ▸/▾ aren't
    // in Inter or egui's fallback fonts, so it's drawn.
    let center = egui::pos2(rect.left() + 6.0, rect.center().y);
    let points = if *open {
        vec![center + egui::vec2(-4.0, -2.5), center + egui::vec2(4.0, -2.5), center + egui::vec2(0.0, 3.0)]
    } else {
        vec![center + egui::vec2(-2.5, -4.0), center + egui::vec2(3.0, 0.0), center + egui::vec2(-2.5, 4.0)]
    };
    painter.add(egui::Shape::convex_polygon(points, text_color, egui::Stroke::NONE));

    let font = egui::FontId::proportional(12.5);
    painter.text(
        egui::pos2(rect.left() + 18.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        font,
        text_color,
    );
    painter.text(
        egui::pos2(rect.right(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        summary,
        egui::FontId::proportional(12.0),
        c.muted,
    );
    *open
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        if let Some(rx) = &self.pending {
            if let Ok(outcome) = rx.try_recv() {
                self.pending = None;
                self.finish(outcome);
            }
        }
        if let Some(status) = self.status.take() {
            self.log.push(LogEntry { time: local_time(), ok: status.ok, text: status.text });
        }
        let busy = self.pending.is_some();

        let bar = egui::Frame::new()
            .fill(theme::p().bar)
            .stroke(egui::Stroke::new(1.0, theme::p().border));

        egui::Panel::top("header")
            .frame(bar.inner_margin(egui::Margin::symmetric(12, 6)))
            .show(ui, |ui| self.header(ui, busy));

        egui::Panel::bottom("status")
            .frame(bar.inner_margin(egui::Margin::symmetric(12, 4)))
            .show(ui, |ui| self.status_bar(ui, busy));

        let central_bottom = ui.available_rect_before_wrap().bottom() - 4.0; // minus frame margin
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::p().bg).inner_margin(egui::Margin::symmetric(12, 4)))
            .show(ui, |ui| {
                ui.add_space(8.0);
                self.description(ui);
                ui.add_space(6.0);
                self.interface_section(ui, busy);
                ui.add_space(2.0);
                self.commands_section(ui);
                ui.add_space(2.0);
                self.actions(ui, ctx, busy);
                ui.add_space(4.0);
                let originals = if self.store.records.is_empty() {
                    "none".to_owned()
                } else {
                    format!("{} adapter(s)", self.store.records.len())
                };
                if fold(ui, &mut self.show_originals, "Saved originals", &originals) {
                    self.originals_section(ui);
                }
                let entries = match self.log.len() {
                    1 => "1 entry".to_owned(),
                    n => format!("{n} entries"),
                };
                if fold(ui, &mut self.show_activity, "Activity", &entries) {
                    self.activity_section(ui);
                }
                ui.add_space(6.0);
                fit_height_to_content(ui, ctx, central_bottom);
            });

        self.restore_dialog(ctx);
    }
}

impl App {
    /// Confirms what Restore did. Closes with OK, Enter, Escape or a click outside.
    fn restore_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let c = theme::p();
        let mut close = false;
        let frame = egui::Frame::new()
            .fill(c.panel)
            .stroke(egui::Stroke::new(1.0, c.border))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(16));
        let modal = egui::Modal::new(egui::Id::new("restore_dialog"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(320.0);
                ui.horizontal(|ui| {
                    let (icon, color) = if dialog.ok { ("✔", c.good) } else { ("⚠", c.bad) };
                    ui.label(egui::RichText::new(icon).color(color).size(16.0));
                    ui.label(theme::semibold(&dialog.title, 15.0).color(c.text));
                });
                ui.add_space(6.0);
                for line in &dialog.lines {
                    ui.label(egui::RichText::new(line).color(c.text).size(13.0));
                }
                ui.add_space(12.0);
                let row = egui::vec2(ui.available_width(), 28.0);
                ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = egui::Button::new(theme::semibold("OK", 13.0).color(c.text))
                        .min_size(egui::vec2(80.0, 28.0));
                    if ui.add(ok).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.dialog = None;
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, busy: bool) {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(egui::load::SizedTexture::new(
                self.icon.id(),
                [20.0, 20.0],
            )));
            ui.label(theme::semibold("SetMTU", 14.0).color(theme::p().text));
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .color(theme::p().faint)
                    .size(11.5),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(!busy, egui::Button::new("⟳ Rescan")).clicked() {
                    self.refresh();
                }
                ui.add_space(4.0);
                theme_toggle(ui);
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui, busy: bool) {
        ui.horizontal(|ui| {
            if busy {
                ui.add(egui::Spinner::new().color(theme::p().accent).size(12.0));
                ui.label(
                    egui::RichText::new("Waiting for UAC approval…")
                        .color(theme::p().text)
                        .size(11.5),
                );
            } else if let Some(last) = self.log.last() {
                let color = if last.ok { theme::p().good } else { theme::p().bad };
                ui.label(egui::RichText::new("●").color(color).size(10.0));
                ui.label(egui::RichText::new(&last.text).color(theme::p().text).size(11.5));
            } else {
                ui.label(egui::RichText::new("●").color(theme::p().faint).size(10.0));
                ui.label(egui::RichText::new("Ready").color(theme::p().muted).size(11.5));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(theme::mono("persistent · UAC", 11.0).color(theme::p().faint))
                    .on_hover_text(
                        "Changes are stored persistently. Apply and Restore run netsh through \
                         an elevated helper, so Windows asks for administrator approval (UAC) \
                         for each change.",
                    );
            });
        });
    }

    /// One-line summary with a More/Less link to the full explanation.
    fn description(&mut self, ui: &mut egui::Ui) {
        let c = theme::p();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(egui::RichText::new(SUMMARY).color(c.muted).size(12.5));
            let link = if self.show_more { "Less" } else { "More" };
            let label = egui::Label::new(egui::RichText::new(link).color(c.accent).size(12.5))
                .sense(egui::Sense::click());
            if ui.add(label).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.show_more = !self.show_more;
            }
        });
        if self.show_more {
            ui.label(egui::RichText::new(DESCRIPTION).color(c.muted).size(12.5));
        }
    }

    fn interface_section(&mut self, ui: &mut egui::Ui, busy: bool) {
        let c = theme::p();
        if self.adapters.is_empty() {
            ui.label(egui::RichText::new("No network adapters found.").color(c.warn));
            return;
        }
        ui.horizontal(|ui| {
            key(ui, "Interface");
            self.adapter_combo(ui, busy);
        });
        let Some(iface) = self.current() else {
            return;
        };
        let mut subtitle = iface.description.clone();
        if iface.default_route {
            subtitle.push_str("  ·  default route");
        }
        ui.label(egui::RichText::new(subtitle).color(c.muted).size(11.5));

        let target = self.target;
        panel(c.panel).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("iface")
                .num_columns(4)
                .spacing([10.0, 4.0])
                .min_col_width(60.0)
                .show(ui, |ui| {
                    let (state, color) =
                        if iface.up { ("connected", c.good) } else { ("disconnected", c.faint) };
                    key(ui, "State");
                    ui.label(theme::mono(state, 11.5).color(color));
                    key(ui, "Traffic");
                    ui.label(theme::mono(net::format_bytes(iface.traffic), 11.5).color(c.text));
                    ui.end_row();

                    for (family, name, mtu) in [
                        (Family::V4, "IPv4 MTU", iface.mtu_v4),
                        (Family::V6, "IPv6 MTU", iface.mtu_v6),
                    ] {
                        key(ui, name);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(theme::mono(mtu_cell(mtu), 12.0).color(c.text));
                            let (tag, color) = match mtu {
                                None => ("not bound".to_owned(), c.faint),
                                Some(_) if family == Family::V6 && target < IPV6_MIN_MTU => {
                                    (format!("kept (min {IPV6_MIN_MTU})"), c.faint)
                                }
                                Some(m) if m == target => ("at target".to_owned(), c.good),
                                Some(_) => (format!("→ {target}"), c.warn),
                            };
                            ui.label(theme::mono(tag, 11.0).color(color));
                        });
                    }
                    ui.end_row();

                    key(ui, "Index");
                    ui.label(theme::mono(iface.index.to_string(), 11.5).color(c.text));
                    key(ui, "LUID");
                    ui.label(theme::mono(format!("0x{:016X}", iface.luid), 11.5).color(c.text));
                    ui.end_row();
                });
        });
    }

    fn adapter_combo(&mut self, ui: &mut egui::Ui, busy: bool) {
        let label = |i: &Interface| {
            let mut s = format!("{}  ·  {}", i.alias, net::format_bytes(i.traffic));
            if !i.up {
                s.push_str("  ·  disconnected");
            }
            s
        };
        let selected_text = self.current().map(label).unwrap_or_default();
        let width = ui.available_width();
        ui.add_enabled_ui(!busy, |ui| {
            egui::ComboBox::from_id_salt("adapter")
                .width(width)
                .height(480.0)
                .selected_text(theme::semibold(selected_text, 12.5).color(theme::p().text))
                .show_ui(ui, |ui| {
                    for i in &self.adapters {
                        let text = format!("{}\n{}", label(i), i.description);
                        ui.selectable_value(&mut self.selected, Some(i.luid), text);
                    }
                });
        });
    }

    fn commands_section(&self, ui: &mut egui::Ui) {
        let c = theme::p();
        panel(c.bar).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(iface) = self.current() else {
                ui.label(theme::mono("# no interface selected", 11.5).color(c.faint));
                return;
            };
            let changes = planned_changes(iface, self.target);
            if changes.is_empty() {
                ui.label(theme::mono("# nothing to do: already at target", 11.5).color(c.faint));
            }
            for (family, mtu) in changes {
                let cmd = format!("netsh {}", net::netsh_args(iface.index, family, mtu).join(" "));
                ui.label(theme::mono(cmd, 11.5).color(c.text));
            }
            if self.target < IPV6_MIN_MTU && iface.mtu_v6.is_some() {
                ui.label(
                    theme::mono(format!("# IPv6 minimum is {IPV6_MIN_MTU}; IPv6 left unchanged"), 11.5)
                        .color(c.faint),
                );
            }
        });
    }

    fn actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, busy: bool) {
        let c = theme::p();
        let can_apply =
            !busy && self.current().is_some_and(|i| !planned_changes(i, self.target).is_empty());
        let can_restore = !busy && !self.store.records.is_empty();
        ui.horizontal(|ui| {
            key(ui, "Target MTU");
            ui.add_enabled_ui(!busy, |ui| {
                egui::ComboBox::from_id_salt("mtu")
                    .width(76.0)
                    .height(420.0)
                    .selected_text(theme::mono(self.target.to_string(), 12.5).color(c.text))
                    .show_ui(ui, |ui| {
                        for &(size, note) in MTU_CHOICES {
                            let text = egui::RichText::new(format!("{size:<6}{note}"))
                                .font(egui::FontId::monospace(12.0));
                            ui.selectable_value(&mut self.target, size, text);
                        }
                    });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let apply = egui::Button::new(
                    theme::semibold(format!("Apply {}", self.target), 12.5).color(c.on_apply),
                )
                .fill(c.apply)
                .stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(96.0, 28.0));
                if ui.add_enabled(can_apply, apply).clicked() {
                    self.start_set(ctx);
                }
                let restore =
                    egui::Button::new(theme::semibold("Restore original", 12.5).color(c.text))
                        .min_size(egui::vec2(0.0, 28.0));
                if ui.add_enabled(can_restore, restore).clicked() {
                    self.start_undo(ctx);
                }
            });
        });
    }

    fn originals_section(&self, ui: &mut egui::Ui) {
        let c = theme::p();
        panel(c.panel).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.store.records.is_empty() {
                ui.label(
                    egui::RichText::new("Original values are saved the first time you apply.")
                        .color(c.faint)
                        .size(12.0),
                );
                return;
            }
            egui::Grid::new("originals")
                .num_columns(4)
                .spacing([18.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in ["INTERFACE", "IPV4", "IPV6", "LUID"] {
                        ui.label(theme::section(h));
                    }
                    ui.end_row();
                    for r in &self.store.records {
                        ui.label(egui::RichText::new(&r.alias).color(c.text).size(12.0));
                        ui.label(theme::mono(mtu_cell(r.mtu_v4), 11.5).color(c.text));
                        ui.label(theme::mono(mtu_cell(r.mtu_v6), 11.5).color(c.text));
                        ui.label(theme::mono(format!("0x{:016X}", r.luid), 11.5).color(c.muted));
                        ui.end_row();
                    }
                });
        });
    }

    fn activity_section(&self, ui: &mut egui::Ui) {
        let c = theme::p();
        panel(c.bar).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.log.is_empty() {
                ui.label(theme::mono("No activity this session.", 11.5).color(c.faint));
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for e in &self.log {
                        let (tag, color) = if e.ok { ("OK ", c.good) } else { ("ERR", c.bad) };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::mono(&e.time, 11.5).color(c.faint));
                            ui.label(theme::mono(tag, 11.5).color(color));
                            ui.label(theme::mono(&e.text, 11.5).color(c.text));
                        });
                    }
                });
        });
    }
}

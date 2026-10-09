#![windows_subsystem = "windows"]

mod net;
mod settings;
mod theme;
mod ui;
mod undo;

use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use net::{Family, Interface};
use ui::planned_changes;
use undo::{Record, UndoStore};

const DEFAULT_MTU: u32 = 1428;
/// Windows rejects IPv6 MTUs below this (RFC 8200 minimum link MTU).
const IPV6_MIN_MTU: u32 = 1280;

/// Sizes offered in the MTU dropdown, with notes for the well-known ones.
const MTU_CHOICES: &[(u32, &str)] = &[
    (1500, "Ethernet, Windows default"),
    (1492, "PPPoE"),
    (1480, ""),
    (1472, ""),
    (1460, ""),
    (1454, ""),
    (1440, ""),
    (1428, "recommended"),
    (1420, "WireGuard"),
    (1400, ""),
    (1380, ""),
    (1360, ""),
    (1340, ""),
    (1320, ""),
    (1300, ""),
    (1280, "IPv6 minimum"),
    (1250, "IPv4 only"),
    (1200, "IPv4 only"),
];

/// Always visible.
const SUMMARY: &str = "Sets a network interface's IP MTU (1428 bytes by default) for paths \
that can't carry 1500 bytes.";

/// Shown under "More".
const DESCRIPTION: &str = "The MTU (Maximum Transmission Unit) is the largest packet the \
interface sends. The Windows default of 1500 bytes is too large for many PPPoE, fibre and VPN \
links, which shows up as stalled page loads, slow transfers or dropped calls. The change is \
persistent and touches nothing else. The original values are saved, and Restore puts them back.";

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--apply") {
        std::process::exit(net::run_helper(&args[2..]));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([540.0, 520.0])
            // Height is fitted to the content at runtime (see ui::fit_height_to_content).
            .with_min_inner_size([480.0, 200.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("bundled icon is a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "SetMTU",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::set_mode(&cc.egui_ctx, settings::Settings::load().dark);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

enum Outcome {
    Set {
        record: Record,
        /// The MTU that was applied (the dropdown may change while UAC is up).
        target: u32,
        result: Result<(), String>,
    },
    Undo {
        results: Vec<(Record, Result<(), String>)>,
    },
}

struct Status {
    ok: bool,
    text: String,
}

struct LogEntry {
    time: String,
    ok: bool,
    text: String,
}

/// Modal confirmation shown after a restore.
struct Dialog {
    ok: bool,
    title: String,
    lines: Vec<String>,
}

struct App {
    /// Busiest first.
    adapters: Vec<Interface>,
    /// LUID of the adapter picked in the dropdown.
    selected: Option<u64>,
    /// MTU picked in the dropdown.
    target: u32,
    store: UndoStore,
    pending: Option<Receiver<Outcome>>,
    /// Latest result; moved into `log` with a timestamp on the next frame.
    status: Option<Status>,
    log: Vec<LogEntry>,
    dialog: Option<Dialog>,
    icon: egui::TextureHandle,
    // Expanded/collapsed sections.
    show_more: bool,
    show_originals: bool,
    show_activity: bool,
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let png = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon_128.png"))
            .expect("bundled icon is a valid PNG");
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [png.width as usize, png.height as usize],
            &png.rgba,
        );
        let mut app = Self {
            adapters: Vec::new(),
            selected: None,
            target: DEFAULT_MTU,
            store: UndoStore::load(),
            pending: None,
            status: None,
            log: Vec::new(),
            dialog: None,
            icon: ctx.load_texture("app-icon", image, egui::TextureOptions::LINEAR),
            show_more: false,
            show_originals: false,
            show_activity: false,
        };
        app.refresh();
        app
    }

    fn current(&self) -> Option<&Interface> {
        self.adapters.iter().find(|i| Some(i.luid) == self.selected)
    }

    /// Re-reads the adapter list. Keeps the user's pick if it still exists;
    /// otherwise selects the busiest adapter, most likely the active connection.
    fn refresh(&mut self) {
        self.adapters = net::list_interfaces();
        if self.current().is_none() {
            self.selected = self.adapters.first().map(|i| i.luid);
        }
    }

    fn start_set(&mut self, ctx: &egui::Context) {
        let Some(iface) = self.current().cloned() else {
            return;
        };
        let target = self.target;
        let changes = planned_changes(&iface, target);
        if changes.is_empty() {
            self.status = Some(Status {
                ok: true,
                text: format!("{}: MTU is already {target}. No change made.", iface.alias),
            });
            return;
        }
        let record = Record {
            luid: iface.luid,
            alias: iface.alias.clone(),
            mtu_v4: iface.mtu_v4,
            mtu_v6: iface.mtu_v6,
        };
        // Save before applying: if only some of the change goes through
        // (e.g. IPv4 succeeds, IPv6 fails), Undo must still be possible.
        self.store.remember(record.clone());
        if let Err(e) = self.store.save() {
            self.status = Some(Status {
                ok: false,
                text: format!("No change made: could not save original values ({e})."),
            });
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = net::set_mtus(iface.index, &changes);
            let _ = tx.send(Outcome::Set { record, target, result });
            ctx.request_repaint();
        });
        self.pending = Some(rx);
        self.status = None;
    }

    fn start_undo(&mut self, ctx: &egui::Context) {
        let records = self.store.records.clone();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let results = records
                .into_iter()
                .map(|r| {
                    let result = match net::index_from_luid(r.luid) {
                        None => Err(format!("{} is not connected right now.", r.alias)),
                        Some(index) => {
                            let changes: Vec<(Family, u32)> =
                                [(Family::V4, r.mtu_v4), (Family::V6, r.mtu_v6)]
                                    .into_iter()
                                    .filter_map(|(f, m)| m.map(|m| (f, m)))
                                    .collect();
                            net::set_mtus(index, &changes)
                        }
                    };
                    (r, result)
                })
                .collect();
            let _ = tx.send(Outcome::Undo { results });
            ctx.request_repaint();
        });
        self.pending = Some(rx);
        self.status = None;
    }

    fn finish(&mut self, outcome: Outcome) {
        self.refresh();
        self.status = Some(match outcome {
            Outcome::Set { record, target, result } => {
                // Trust what the adapter reports now, not just netsh's exit code.
                let now = self.adapters.iter().find(|i| i.luid == record.luid);
                let unchanged =
                    now.is_some_and(|i| i.mtu_v4 == record.mtu_v4 && i.mtu_v6 == record.mtu_v6);
                if unchanged {
                    // Nothing actually changed, so there's nothing to undo.
                    self.store.records.retain(|r| r.luid != record.luid);
                    let _ = self.store.save();
                }
                let was = record.mtu_v4.or(record.mtu_v6).unwrap_or(0);
                let is = now.and_then(|i| i.mtu_v4.or(i.mtu_v6));
                match (result, is) {
                    (Ok(()), Some(m)) if m == target => Status {
                        ok: true,
                        text: format!("{}: MTU set to {target} (was {was}).", record.alias),
                    },
                    (Ok(()), _) => Status {
                        ok: false,
                        text: format!(
                            "netsh reported success, but {} still reports {}.",
                            record.alias,
                            mtu_text(is)
                        ),
                    },
                    (Err(e), _) if unchanged => Status { ok: false, text: e },
                    (Err(e), _) => Status {
                        ok: false,
                        text: format!("{e} The change may be partial; Restore reverts it."),
                    },
                }
            }
            Outcome::Undo { results } => {
                let mut restored = Vec::new();
                let mut lines = Vec::new();
                let mut errors = Vec::new();
                for (record, result) in results {
                    match result {
                        Ok(()) => {
                            self.store.records.retain(|r| r.luid != record.luid);
                            // Show what the adapter reports now, not just what was requested.
                            let now = self.adapters.iter().find(|i| i.luid == record.luid);
                            let v4 = now.and_then(|i| i.mtu_v4).or(record.mtu_v4);
                            let v6 = now.and_then(|i| i.mtu_v6).or(record.mtu_v6);
                            let mut families = Vec::new();
                            if let Some(m) = v4 {
                                families.push(format!("IPv4 {m}"));
                            }
                            if let Some(m) = v6 {
                                families.push(format!("IPv6 {m}"));
                            }
                            lines.push(format!("{}: {}", record.alias, families.join(", ")));
                            // Make the controls reflect the restored value for the
                            // adapter on screen, instead of the target that was applied.
                            if Some(record.luid) == self.selected {
                                if let Some(m) = v4.or(v6) {
                                    self.target = m;
                                }
                            }
                            restored.push(record.alias);
                        }
                        Err(e) => errors.push(e),
                    }
                }
                if let Err(e) = self.store.save() {
                    errors.push(format!("Could not update saved originals: {e}"));
                }
                self.dialog = Some(if errors.is_empty() {
                    Dialog { ok: true, title: "Original MTU restored".into(), lines }
                } else {
                    lines.extend(errors.iter().cloned());
                    Dialog { ok: false, title: "Restore didn't complete".into(), lines }
                });
                if errors.is_empty() {
                    Status {
                        ok: true,
                        text: format!("Restored original MTU: {}.", restored.join(", ")),
                    }
                } else {
                    Status { ok: false, text: errors.join(" ") }
                }
            }
        });
    }
}

fn mtu_text(mtu: Option<u32>) -> String {
    match mtu {
        Some(m) => format!("{m} bytes"),
        None => "not in use".into(),
    }
}

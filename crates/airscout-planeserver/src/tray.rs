//! The tray icon (C# `ni_Main`, `MainDlg.cs:431-469`): menu `Show window`, `Quit` and a
//! disabled line with the plane count.
//!
//! macOS and Windows: [`Tray::new`] runs on the main thread inside the eframe app creator,
//! after the event loop is set up; the icon lives as long as the [`Tray`]. Linux: the icon
//! lives on its own thread with a GTK main loop (the `tray-icon` README pattern); every change
//! to the menu and the final quit go to that thread through a channel, so GTK is never
//! touched from the egui thread.
//!
//! Menu and icon events go into one channel; the handlers wake egui, so the events reach
//! [`Tray::poll`] also while the window is hidden.

use eframe::egui;
use std::sync::mpsc::{channel, Receiver};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";
const TOOLTIP: &str = "AirScout Plane Server";
const ICON_SIZE: u32 = 32;

/// What the user chose in the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Quit,
}

/// The text of the disabled plane count line.
pub fn planes_text(n: usize) -> String {
    format!("Planes: {n}")
}

/// A generated 32 × 32 RGBA icon: a blue disc with a white cross (the app embeds no icon).
pub fn icon_rgba() -> Vec<u8> {
    let n = ICON_SIZE as i32;
    let c = (n - 1) as f32 / 2.0;
    let mut v = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let inside = dx * dx + dy * dy <= c * c;
            // a plane seen from above: fuselage and wings
            let cross =
                (dx.abs() <= 1.5 && dy.abs() <= 11.0) || (dy.abs() <= 1.5 && dx.abs() <= 10.0);
            let px = match (inside, cross) {
                (true, true) => [255, 255, 255, 255],
                (true, false) => [30, 90, 200, 255],
                (false, _) => [0, 0, 0, 0],
            };
            v.extend_from_slice(&px);
        }
    }
    v
}

/// The menu items and the icon. Built on the thread that owns them.
struct Parts {
    _icon: TrayIcon,
    count: MenuItem,
}

fn build() -> Result<Parts, String> {
    let show = MenuItem::with_id(SHOW_ID, "Show window", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    let count = MenuItem::new(planes_text(0), false, None);
    let menu = Menu::new();
    menu.append_items(&[&count, &PredefinedMenuItem::separator(), &show, &quit])
        .map_err(|e| e.to_string())?;
    let icon = Icon::from_rgba(icon_rgba(), ICON_SIZE, ICON_SIZE).map_err(|e| e.to_string())?;
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(TOOLTIP)
        .with_icon(icon)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Parts { _icon: icon, count })
}

/// Sends menu and icon events into a channel and wakes egui.
fn install_handlers(ctx: &egui::Context) -> Receiver<TrayCommand> {
    let (tx, rx) = channel();
    let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let cmd = match e.id().as_ref() {
            SHOW_ID => TrayCommand::Show,
            QUIT_ID => TrayCommand::Quit,
            _ => return,
        };
        let _ = menu_tx.send(cmd);
        menu_ctx.request_repaint();
    }));
    let icon_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::DoubleClick { .. } = e {
            let _ = tx.send(TrayCommand::Show);
            icon_ctx.request_repaint();
        }
    }));
    rx
}

/// The tray as the window sees it; tests use a fake.
pub trait TrayHandle {
    /// The commands chosen since the last call.
    fn poll(&self) -> Vec<TrayCommand>;
    /// Updates the plane count line.
    fn set_planes(&mut self, n: usize);
}

impl TrayHandle for Tray {
    fn poll(&self) -> Vec<TrayCommand> {
        Tray::poll(self)
    }
    fn set_planes(&mut self, n: usize) {
        Tray::set_planes(self, n)
    }
}

/// The tray icon of the window.
pub struct Tray {
    rx: Receiver<TrayCommand>,
    shown: Option<usize>,
    #[cfg(not(target_os = "linux"))]
    parts: Parts,
    #[cfg(target_os = "linux")]
    gtk: std::sync::mpsc::Sender<GtkCmd>,
}

impl Tray {
    /// Creates the icon. An `Err` (no tray host, no display) is shown in the Status tab;
    /// the window then works without a tray.
    #[cfg(not(target_os = "linux"))]
    pub fn new(ctx: &egui::Context) -> Result<Tray, String> {
        let parts = build()?;
        Ok(Tray {
            rx: install_handlers(ctx),
            shown: None,
            parts,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn new(ctx: &egui::Context) -> Result<Tray, String> {
        let gtk = linux::spawn()?;
        Ok(Tray {
            rx: install_handlers(ctx),
            shown: None,
            gtk,
        })
    }

    /// The commands chosen since the last call.
    pub fn poll(&self) -> Vec<TrayCommand> {
        self.rx.try_iter().collect()
    }

    /// Updates the plane count line (only when it changed).
    pub fn set_planes(&mut self, n: usize) {
        if self.shown == Some(n) {
            return;
        }
        self.shown = Some(n);
        #[cfg(not(target_os = "linux"))]
        self.parts.count.set_text(planes_text(n));
        #[cfg(target_os = "linux")]
        let _ = self.gtk.send(GtkCmd::Planes(n));
    }
}

#[cfg(target_os = "linux")]
impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.gtk.send(GtkCmd::Quit);
    }
}

/// A change for the GTK thread.
#[cfg(target_os = "linux")]
enum GtkCmd {
    Planes(usize),
    Quit,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{build, planes_text, GtkCmd};
    use gtk::glib;
    use std::sync::mpsc::{channel, Sender};
    use std::time::Duration;

    /// How often the GTK thread looks for changes.
    const POLL: Duration = Duration::from_millis(100);

    /// Starts the GTK thread and waits until the icon is built or failed.
    pub(super) fn spawn() -> Result<Sender<GtkCmd>, String> {
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let (tx, rx) = channel::<GtkCmd>();
        std::thread::Builder::new()
            .name("tray-gtk".into())
            .spawn(move || {
                if let Err(e) = gtk::init() {
                    let _ = ready_tx.send(Err(format!("cannot start GTK: {e}")));
                    return;
                }
                let parts = match build() {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                // The closure owns the icon: it goes when the loop ends.
                glib::timeout_add_local(POLL, move || {
                    loop {
                        match rx.try_recv() {
                            Ok(GtkCmd::Planes(n)) => parts.count.set_text(planes_text(n)),
                            Ok(GtkCmd::Quit) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                gtk::main_quit();
                                return glib::ControlFlow::Break;
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        }
                    }
                    glib::ControlFlow::Continue
                });
                gtk::main();
            })
            .map_err(|e| format!("cannot start the tray thread: {e}"))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(tx),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the tray thread ended".to_owned()),
        }
    }
}

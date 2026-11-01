//! Petite fenêtre : on colle l'URL d'une page Notion, le PDF est généré dans
//! Téléchargements puis ouvert directement.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Layout, Margin,
    RichText, Shadow, Stroke, Vec2,
};

use crate::generate::{self, Generated};
use crate::notion;

// Palette inspirée de Notion : fond crème, encre presque noire, gris chauds.
const BACKGROUND: Color32 = Color32::from_rgb(247, 246, 243);
const CARD: Color32 = Color32::WHITE;
const BORDER: Color32 = Color32::from_rgb(233, 231, 226);
const INK: Color32 = Color32::from_rgb(25, 25, 25);
const MUTED: Color32 = Color32::from_rgb(120, 119, 116);
const FIELD: Color32 = Color32::from_rgb(251, 251, 250);
const SUCCESS_BG: Color32 = Color32::from_rgb(237, 246, 239);
const SUCCESS_BORDER: Color32 = Color32::from_rgb(203, 228, 210);
const SUCCESS_INK: Color32 = Color32::from_rgb(28, 110, 60);
const ERROR_BG: Color32 = Color32::from_rgb(253, 238, 236);
const ERROR_BORDER: Color32 = Color32::from_rgb(245, 205, 200);
const ERROR_INK: Color32 = Color32::from_rgb(180, 40, 30);
const WARNING_INK: Color32 = Color32::from_rgb(160, 100, 10);

const BOLD: &str = "bold";

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("printNotion")
            .with_inner_size([600.0, 580.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "printNotion",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
    .map_err(|e| anyhow!("Impossible d'ouvrir la fenêtre : {e}"))
}

/// Police Avenir Next (macOS) si disponible, thème clair, coins arrondis.
fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut bold_family = fonts.families[&FontFamily::Proportional].clone();
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/Avenir Next.ttc") {
        let bytes: Arc<[u8]> = bytes.into();
        for (name, index) in [("avenir", 7), ("avenir-demi", 2)] {
            let mut data = FontData::from_owned(bytes.to_vec());
            data.index = index;
            fonts.font_data.insert(name.into(), Arc::new(data));
        }
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "avenir".into());
        bold_family.insert(0, "avenir-demi".into());
    }
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold_family);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Light);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        let visuals = &mut style.visuals;
        visuals.panel_fill = BACKGROUND;
        visuals.override_text_color = Some(INK);
        visuals.selection.bg_fill = Color32::from_rgb(200, 220, 250);
        visuals.selection.stroke = Stroke::new(1.0, INK);
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.noninteractive,
        ] {
            widget.corner_radius = CornerRadius::same(10);
        }
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(240, 239, 236);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 231, 227);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(224, 222, 218);
    });
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

enum State {
    Idle,
    /// La génération tourne dans un thread, pour ne pas figer la fenêtre.
    Running(Receiver<Result<Generated>>),
    Done(Generated),
    Failed(String),
}

struct App {
    url: String,
    state: State,
}

impl App {
    fn new() -> Self {
        Self { url: notion_link_in_clipboard().unwrap_or_default(), state: State::Idle }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        let page = self.url.trim().to_owned();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = output_dir()
                .and_then(|dir| generate::generate(&page, Some(dir)))
                .and_then(|generated| open_file(&generated.path).map(|()| generated));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        self.state = State::Running(receiver);
    }

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Petit logo : carré noir arrondi avec un « N ».
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(10), INK);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "N", bold(22.0), Color32::WHITE);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new("printNotion").font(bold(22.0)));
                ui.label(RichText::new("Colle un lien Notion, récupère un PDF prêt à imprimer.").size(14.0).color(MUTED));
            });
        });
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        let running = matches!(self.state, State::Running(_));
        card(CARD, BORDER).show(ui, |ui| {
            ui.label(RichText::new("LIEN DE LA PAGE").font(bold(11.0)).color(MUTED));

            let mut entered = false;
            ui.horizontal(|ui| {
                let paste_width = 90.0;
                let field = ui.add_enabled(
                    !running,
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("https://www.notion.so/…")
                        .font(FontId::proportional(15.0))
                        .margin(Margin::symmetric(12, 11))
                        .background_color(FIELD)
                        .desired_width(ui.available_width() - paste_width - 10.0),
                );
                entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let paste = egui::Button::new(RichText::new("Coller").size(14.0)).min_size(Vec2::new(paste_width, 42.0));
                if ui.add_enabled(!running, paste).clicked() {
                    if let Some(text) = clipboard_text() {
                        self.url = text;
                    }
                }
            });

            ui.add_space(4.0);
            let ready = !running && !self.url.trim().is_empty();
            let label = if running { "Génération en cours…" } else { "Générer le PDF" };
            let response = primary_button(ui, label, ready, running);
            if ready && (response.clicked() || entered) {
                self.start(ui.ctx());
            }
        });
    }

    fn result(&self, ui: &mut egui::Ui) {
        match &self.state {
            State::Idle | State::Running(_) => {}
            State::Done(generated) => {
                card(SUCCESS_BG, SUCCESS_BORDER).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        check_badge(ui);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new("PDF prêt et ouvert").font(bold(15.0)).color(SUCCESS_INK));
                            let name = generated.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                            ui.label(RichText::new(name).size(13.0).color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(RichText::new("Afficher dans le Finder").size(13.0)).clicked() {
                                let _ = Command::new("open").arg("-R").arg(&generated.path).status();
                            }
                            if ui.button(RichText::new("Ouvrir").size(13.0)).clicked() {
                                let _ = open_file(&generated.path);
                            }
                        });
                    });
                    if generated.access_denied > 0 {
                        ui.add_space(4.0);
                        access_notice(ui, generated.access_denied);
                    }
                    if !generated.warnings.is_empty() {
                        ui.add_space(2.0);
                        let title = RichText::new(format!("⚠ {} élément(s) non imprimé(s)", generated.warnings.len()))
                            .size(13.0)
                            .color(WARNING_INK);
                        egui::CollapsingHeader::new(title).id_salt("warnings").show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                                for warning in &generated.warnings {
                                    ui.label(RichText::new(format!("• {warning}")).size(12.5).color(MUTED));
                                }
                            });
                        });
                    }
                });
            }
            State::Failed(message) => {
                card(ERROR_BG, ERROR_BORDER).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new("Impossible de générer le PDF").font(bold(15.0)).color(ERROR_INK));
                    ui.label(RichText::new(message).size(13.0).color(ERROR_INK));
                });
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let State::Running(receiver) = &self.state {
            if let Ok(result) = receiver.try_recv() {
                self.state = match result {
                    Ok(generated) => State::Done(generated),
                    Err(e) => State::Failed(format!("{e:#}")),
                };
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(BACKGROUND).inner_margin(Margin::same(28)))
            .show(ui, |ui| {
                self.header(ui);
                ui.add_space(14.0);
                self.form(ui);
                ui.add_space(4.0);
                self.result(ui);
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.label(RichText::new("Les PDF sont enregistrés dans Téléchargements").size(12.0).color(MUTED));
                });
            });
    }
}

/// Gros bouton noir, libellé centré ; pendant la génération, une roue précède le libellé.
fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool, running: bool) -> egui::Response {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), sense);
    let fill = match (enabled, running, response.hovered()) {
        (_, true, _) => INK,
        (false, _, _) => Color32::from_rgb(196, 195, 191),
        (true, _, true) => Color32::from_rgb(55, 55, 53),
        (true, _, false) => INK,
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);

    let galley = ui.painter().layout_no_wrap(label.to_owned(), bold(15.0), Color32::WHITE);
    let spinner_width = if running { 28.0 } else { 0.0 };
    let left = rect.center().x - (galley.size().x + spinner_width) / 2.0;
    if running {
        let spinner_rect = egui::Rect::from_center_size(egui::pos2(left + 9.0, rect.center().y), Vec2::splat(18.0));
        egui::Spinner::new().color(Color32::WHITE).paint_at(ui, spinner_rect);
    }
    let text_pos = egui::pos2(left + spinner_width, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, Color32::WHITE);

    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Encadré ambré : des bases n'ont pas pu être lues, bouton pour ouvrir la gestion des accès dans Notion.
fn access_notice(ui: &mut egui::Ui, count: usize) {
    Frame::new()
        .fill(Color32::from_rgb(253, 246, 228))
        .stroke(Stroke::new(1.0, Color32::from_rgb(240, 222, 175)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let title = if count == 1 {
                        "1 base n'est pas partagée avec l'intégration".to_owned()
                    } else {
                        format!("{count} bases ne sont pas partagées avec l'intégration")
                    };
                    ui.label(RichText::new(title).font(bold(14.0)).color(WARNING_INK));
                    ui.label(
                        RichText::new("Donnez-lui accès aux bases d'origine, puis régénérez le PDF.")
                            .size(12.5)
                            .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let button = egui::Button::new(RichText::new("Donner l'accès").font(bold(13.0)).color(Color32::WHITE))
                        .fill(WARNING_INK)
                        .corner_radius(CornerRadius::same(8));
                    if ui.add(button).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        let _ = open_url(generate::INTEGRATIONS_URL);
                    }
                });
            });
        });
}

/// Pastille verte avec une coche, dessinée pour ne pas dépendre des polices emoji.
fn check_badge(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
    let c = rect.center();
    ui.painter().circle_filled(c, 14.0, SUCCESS_INK);
    let stroke = Stroke::new(2.5, Color32::WHITE);
    ui.painter().line_segment([c + Vec2::new(-6.0, 0.5), c + Vec2::new(-1.5, 5.0)], stroke);
    ui.painter().line_segment([c + Vec2::new(-1.5, 5.0), c + Vec2::new(6.5, -4.5)], stroke);
}

fn card(fill: Color32, border: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20))
        .shadow(Shadow { offset: [0, 2], blur: 12, spread: 0, color: Color32::from_black_alpha(10) })
}

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .map(|text| text.trim().to_owned())
        .ok()
}

/// Lien Notion présent dans le presse-papiers, pour pré-remplir le champ.
fn notion_link_in_clipboard() -> Option<String> {
    clipboard_text().filter(|text| text.contains("notion") && notion::parse_page_id(text).is_ok())
}

/// Les PDF générés depuis l'interface vont dans Téléchargements.
fn output_dir() -> Result<PathBuf> {
    let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("Impossible de créer {}", dir.display()))?;
    Ok(dir)
}

fn open_url(url: &str) -> Result<()> {
    open_file(Path::new(url))
}

/// Ouvre un fichier avec l'application par défaut du système (Aperçu sur macOS).
fn open_file(path: &Path) -> Result<()> {
    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(path).status()
    } else if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", ""]).arg(path).status()
    } else {
        Command::new("xdg-open").arg(path).status()
    }
    .context("Impossible d'ouvrir le PDF")?;
    if !status.success() {
        bail!("Impossible d'ouvrir le PDF {}", path.display());
    }
    Ok(())
}


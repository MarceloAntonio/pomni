//! Native frontend. All device access runs on one worker, serialized with writes.
#[path = "pomni-gui/palettes.rs"]
mod palettes;
#[path = "pomni-gui/preferences.rs"]
mod preferences;
use palettes::PaletteBook;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use pomni::{cli::Brightness, device, identify, protocol, rgb};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

const BG: Color32 = Color32::from_rgb(22, 22, 26);
const PANEL: Color32 = Color32::from_rgb(30, 30, 36);
const ACCENT: Color32 = Color32::from_rgb(14, 95, 168);
const MUTED: Color32 = Color32::from_rgb(150, 155, 170);
const OK: Color32 = Color32::from_rgb(76, 209, 122);
const ERROR: Color32 = Color32::from_rgb(255, 107, 107);
const COLOR_SIZE: Vec2 = Vec2::splat(28.0);
const SWATCHES: [[u8; 3]; 8] = [
    [255, 70, 90],
    [255, 150, 45],
    [255, 225, 80],
    [70, 220, 140],
    [76, 181, 232],
    [90, 100, 255],
    [210, 95, 255],
    [255, 255, 255],
];

#[derive(Default)]
struct DeviceStatus {
    name: String,
    mode: String,
    model: Option<&'static str>,
    battery: Option<u8>,
    effect: Option<usize>,
    brightness: Option<u8>,
    configurable: bool,
    note: String,
}

fn read_status() -> Result<DeviceStatus, String> {
    let d = device::detect()?;
    let mut state = DeviceStatus {
        name: d.name.clone(),
        mode: d.mode().into(),
        battery: d.battery()?,
        ..Default::default()
    };
    if !d.supports_configuration() {
        state.note = "Configuração RGB indisponível nesta conexão. Use cabo ou receptor wireless compatível.".into();
        return Ok(state);
    }
    let h = protocol::open(&d)?;
    let model = identify(&h)?;
    let led = protocol::read_led(&h)?;
    if led[126..] != [0x5a, 0xa5] {
        return Err("Configuração LED recebida sem marcador válido.".into());
    }
    state.model = Some(model);
    state.configurable = true;
    if led[9] == 0 {
        state.effect = rgb::EFFECTS.iter().position(|(_, hw, _)| *hw == led[10]);
    }
    state.brightness = rgb::brightness_percent(
        &led,
        model,
        rgb::LedLayout::for_connection(model, h.wireless()),
    );
    if state.effect.is_none() {
        state.note = "Efeito atual personalizado ou desconhecido. Escolha um efeito para alterar a iluminação.".into();
    }
    if state.battery.is_none() {
        match protocol::battery(&h) {
            Ok(value) => state.battery = value,
            Err(e) => {
                state.note = format!("{} Bateria indisponível: {e}", state.note)
                    .trim()
                    .into()
            }
        }
    }
    Ok(state)
}

enum Job {
    Refresh,
    Apply {
        effect: usize,
        color: Option<[u8; 3]>,
        brightness: Option<u8>,
    },
}
enum Outcome {
    Refreshed(DeviceStatus),
    Applied {
        message: String,
        color: Option<[u8; 3]>,
    },
}

fn worker(ctx: egui::Context) -> (Sender<Job>, Receiver<Result<Outcome, String>>) {
    let (tx, jobs) = mpsc::channel();
    let (results, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for job in jobs {
            let result = match job {
                Job::Refresh => read_status().map(Outcome::Refreshed),
                Job::Apply {
                    effect,
                    color,
                    brightness,
                } => pomni::apply_settings(
                    color,
                    Some(rgb::EFFECTS[effect].0.into()),
                    brightness.map(Brightness::Absolute),
                    false,
                )
                .map(|message| Outcome::Applied { message, color }),
            };
            if results.send(result).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    (tx, rx)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ColorFormat {
    #[default]
    Hex,
    Rgb,
}

struct App {
    tx: Sender<Job>,
    rx: Receiver<Result<Outcome, String>>,
    device: Option<DeviceStatus>,
    effect: usize,
    color: [u8; 3],
    hex: String,
    change_color: bool,
    brightness: u8,
    change_brightness: bool,
    preview_model: usize,
    status: Result<String, String>,
    busy: bool,
    palettes: PaletteBook,
    palette_dirty: bool,
    editing_favorites: bool,
    editing_favorite: usize,
    favorite_hex: String,
    color_format: ColorFormat,
    favorite_format: ColorFormat,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.style_mut_of(egui::Theme::Dark, |s| {
            s.visuals.panel_fill = BG;
            s.visuals.selection.bg_fill = ACCENT;
            s.spacing.item_spacing = Vec2::new(10.0, 10.0);
            s.spacing.button_padding = Vec2::new(14.0, 8.0);
        });
        let (tx, rx) = worker(cc.egui_ctx.clone());
        let mut app = Self {
            tx,
            rx,
            device: None,
            effect: 0,
            color: [76, 181, 232],
            hex: "4CB5E8".into(),
            change_color: false,
            brightness: 70,
            change_brightness: false,
            preview_model: 0,
            status: Ok("Consultando teclado…".into()),
            busy: false,
            palettes: cc
                .storage
                .and_then(|storage| storage.get_string(palettes::STORAGE_KEY))
                .map_or_else(PaletteBook::default, |s| PaletteBook::decode(&s)),
            palette_dirty: false,
            editing_favorites: false,
            editing_favorite: 0,
            favorite_hex: color_hex(SWATCHES[0]),
            color_format: ColorFormat::default(),
            favorite_format: ColorFormat::default(),
        };
        app.send(Job::Refresh);
        app
    }

    fn send(&mut self, job: Job) {
        self.busy = self.tx.send(job).is_ok();
        if !self.busy {
            self.status =
                Err("O processo de comunicação foi encerrado. Reabra o aplicativo.".into());
        }
    }

    fn receive(&mut self) {
        if let Ok(result) = self.rx.try_recv() {
            self.busy = false;
            match result {
                Ok(Outcome::Refreshed(state)) => {
                    if let Some(i) = state.effect {
                        self.effect = i;
                    }
                    if let Some(value) = state.brightness {
                        self.brightness = value;
                    }
                    self.preview_model = match state.model {
                        Some("F87") => 1,
                        Some("F99") => 2,
                        _ => 0,
                    };
                    self.change_color = false;
                    self.change_brightness = false;
                    self.device = Some(state);
                    if self
                        .status
                        .as_ref()
                        .is_ok_and(|s| s == "Consultando teclado…")
                    {
                        self.status =
                            Ok("Configuração lida. Faça os ajustes e clique em Aplicar.".into());
                    }
                }
                Ok(Outcome::Applied { message, color }) => {
                    if let Some(color) = color {
                        self.palettes.remember(color);
                        self.palette_dirty = true;
                    }
                    self.status = Ok(message);
                    self.send(Job::Refresh);
                }
                Err(e) => {
                    self.device = None;
                    self.status = Err(e);
                }
            }
        }
    }

    fn effects(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new("ILUMINAÇÃO").color(MUTED).size(11.0));
        ui.heading("Efeitos de luz");
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_enabled_ui(!self.busy, |ui| {
                for (i, (_, _, label)) in rgb::EFFECTS.iter().enumerate() {
                    let (rect, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
                    let fill = if self.effect == i {
                        ACCENT
                    } else if response.hovered() {
                        Color32::from_gray(42)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.left_center() + Vec2::new(10.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::proportional(14.0),
                        Color32::from_gray(225),
                    );
                    if response.clicked() {
                        self.effect = i;
                    }
                }
            });
        });
    }

    fn select_color(&mut self, color: [u8; 3]) {
        self.color = color;
        self.hex = format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        self.change_color = true;
    }

    fn palette_controls(&mut self, ui: &mut egui::Ui) {
        palette_heading(ui, "Favoritas", |ui| {
            let label = if self.editing_favorites {
                "Concluir"
            } else {
                "Editar"
            };
            if ui
                .add_sized(
                    [64.0, 22.0],
                    egui::Button::new(label).selected(self.editing_favorites),
                )
                .clicked()
            {
                self.editing_favorites = !self.editing_favorites;
                self.favorite_hex = color_hex(self.palettes.favorites[self.editing_favorite]);
            }
            if ui
                .add_sized([64.0, 22.0], egui::Button::new("Resetar"))
                .on_hover_text("Restaurar as oito cores favoritas padrão")
                .clicked()
            {
                self.palettes.reset_favorites();
                self.favorite_hex = color_hex(self.palettes.favorites[self.editing_favorite]);
                self.palette_dirty = true;
            }
        });
        ui.horizontal(|ui| {
            for i in 0..8 {
                let color = self.palettes.favorites[i];
                let response = color_swatch(ui, color, self.editing_favorites && i == self.editing_favorite, true);
                if self.editing_favorites {
                    if response.on_hover_text(format!("Editar favorita {}", i + 1)).clicked() {
                        self.editing_favorite = i;
                        self.favorite_hex = color_hex(color);
                    }
                } else {
                    let response = response.on_hover_text(format!("#{:02X}{:02X}{:02X} · Clique para selecionar; botão direito para substituir", color[0], color[1], color[2]));
                    if response.clicked() { self.select_color(color); }
                    response.context_menu(|ui| {
                        if ui.button("Substituir pela cor selecionada").clicked() {
                            self.palettes.favorites[i] = self.color;
                            self.palette_dirty = true;
                            ui.close();
                        }
                    });
                }
            }
        });
        palette_heading(ui, "Últimas usadas", |_| {});
        ui.horizontal(|ui| {
            for i in 0..8 {
                let color = self.palettes.recent.get(i).copied();
                let response = color_swatch(ui, color.unwrap_or([38; 3]), false, color.is_some());
                if let Some(color) = color
                    && response
                        .on_hover_text(format!(
                            "#{:02X}{:02X}{:02X} · Selecionar",
                            color[0], color[1], color[2]
                        ))
                        .clicked()
                {
                    self.select_color(color);
                }
            }
        });
        // Keep this hint's space even once the history has entries.
        ui.add_sized(
            [330.0, 16.0],
            egui::Label::new(
                RichText::new(if self.palettes.recent.is_empty() {
                    "Suas cores aparecem aqui após aplicar ao teclado."
                } else {
                    ""
                })
                .size(12.0)
                .color(MUTED),
            ),
        );
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let (_, hw, _) = rgb::EFFECTS[self.effect];
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(330.0);
                    ui.heading(if self.editing_favorites {
                        format!("Favorita {}", self.editing_favorite + 1)
                    } else {
                        "Cor".into()
                    });
                    if rgb::color_capable(hw) {
                        if self.editing_favorites {
                            ui.allocate_ui_with_layout(
                                Vec2::new(330.0, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new("Editando favorita · salva automaticamente")
                                            .size(12.0)
                                            .color(MUTED),
                                    );
                                },
                            );
                            if color_fields(
                                ui,
                                &mut self.palettes.favorites[self.editing_favorite],
                                &mut self.favorite_hex,
                                &mut self.favorite_format,
                            ) {
                                self.palette_dirty = true;
                            }
                        } else {
                            ui.allocate_ui_with_layout(
                                Vec2::new(330.0, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut self.change_color, "Alterar cor");
                                },
                            );
                            if color_fields(
                                ui,
                                &mut self.color,
                                &mut self.hex,
                                &mut self.color_format,
                            ) {
                                self.change_color = true;
                            }
                        }
                        self.palette_controls(ui);
                        ui.label(
                            RichText::new(if self.editing_favorites {
                                "Escolha uma amostra e clique em Concluir ao terminar."
                            } else {
                                "Desmarcado: mantém a paleta do teclado."
                            })
                            .color(MUTED)
                            .size(12.0),
                        );
                    } else {
                        ui.label(if hw == 0 {
                            "Iluminação desligada."
                        } else {
                            "Este efeito usa as cores do firmware."
                        });
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(220.0);
                    ui.heading("Brilho");
                    ui.add_enabled_ui(hw != 0, |ui| {
                        ui.checkbox(&mut self.change_brightness, "Alterar brilho");
                        if ui
                            .add(egui::Slider::new(&mut self.brightness, 0..=100).suffix("%"))
                            .changed()
                        {
                            self.change_brightness = true;
                        }
                    });
                    ui.label(
                        RichText::new("F75: 10 níveis · F87/F99: 5 níveis")
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.label(
                        RichText::new("Desmarcado: mantém o brilho do efeito.")
                            .size(12.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                let valid_color =
                    !self.change_color || !rgb::color_capable(hw) || parse_hex(&self.hex).is_some();
                let ready = self.device.as_ref().is_some_and(|d| d.configurable)
                    && valid_color
                    && !self.editing_favorites;
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(RichText::new("Aplicar ao teclado").strong())
                            .fill(ACCENT),
                    )
                    .clicked()
                {
                    self.status = Ok("Enviando e confirmando configuração…".into());
                    self.send(Job::Apply {
                        effect: self.effect,
                        color: (self.change_color && rgb::color_capable(hw)).then_some(self.color),
                        brightness: (self.change_brightness && hw != 0).then_some(self.brightness),
                    });
                }
                if ui.button("Atualizar conexão").clicked() {
                    self.status = Ok("Consultando teclado…".into());
                    self.send(Job::Refresh);
                }
                ui.label(
                    RichText::new("As alterações são enviadas ao clicar em Aplicar.")
                        .color(MUTED)
                        .size(12.0),
                );
            });
        });
        ui.add_space(8.0);
        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Comunicando com o teclado…");
            });
        }
        match &self.status {
            Ok(s) => {
                ui.colored_label(OK, s);
            }
            Err(s) => {
                ui.colored_label(ERROR, s);
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("POMNI").size(32.0).strong());
                ui.label(
                    RichText::new("CONTROLE RGB  /  LINUX")
                        .size(11.0)
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(d) = &self.device {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let color = if d.configurable { OK } else { MUTED };
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(8.0, 16.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.0, color);
                            ui.colored_label(color, &d.mode);
                        });
                        ui.label(
                            d.model
                                .map_or_else(|| d.name.clone(), |m| format!("AULA {m}")),
                        );
                        match d.battery {
                            Some(value) => {
                                ui.add(
                                    egui::ProgressBar::new(f32::from(value) / 100.0)
                                        .desired_width(140.0)
                                        .text(format!("Bateria {value}%")),
                                );
                            }
                            None => {
                                ui.label(RichText::new("Bateria indisponível").color(MUTED));
                            }
                        }
                    });
                } else {
                    ui.colored_label(MUTED, "Sem conexão confirmada");
                }
            });
        });
        if let Some(d) = &self.device
            && !d.note.is_empty()
        {
            ui.add_space(8.0);
            ui.label(RichText::new(&d.note).color(MUTED));
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        ui.add_space(30.0);
        ui.horizontal(|ui| {
            ui.heading(rgb::EFFECTS[self.effect].2);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("preview_model")
                    .selected_text(format!(
                        "{} · ABNT2",
                        ["F75", "F87", "F99"][self.preview_model]
                    ))
                    .show_ui(ui, |ui| {
                        for (i, model) in ["F75", "F87", "F99"].iter().enumerate() {
                            ui.selectable_value(&mut self.preview_model, i, *model);
                        }
                    });
                ui.label(RichText::new("Visualização").color(MUTED));
            });
        });
        ui.add_space(24.0);
        let rows = keyboard_rows(self.preview_model);
        let units = rows
            .iter()
            .map(|r| r.iter().map(|(_, w)| w).sum::<f32>())
            .fold(0.0, f32::max);
        let scale = ((ui.available_width() - 30.0) / units)
            .min(((ui.available_height() - 62.0) / 6.0).max(12.0))
            .min(48.0);
        let height = scale * 6.0 + 30.0;
        let (area, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let width = units * scale + 24.0;
        let origin = egui::pos2(area.center().x - width / 2.0, area.top());
        let painter = ui.painter();
        painter.rect_filled(
            egui::Rect::from_min_size(origin, Vec2::new(width, height)),
            12.0,
            Color32::from_rgb(7, 7, 9),
        );
        let t = ui.input(|i| i.time) as f32;
        let hw = rgb::EFFECTS[self.effect].1;
        let mut key_index = 0;
        for (row, keys) in rows.iter().enumerate() {
            let mut x = 0.0;
            for (label, units) in keys {
                if !label.is_empty() {
                    let rect = egui::Rect::from_min_size(
                        origin + Vec2::new(12.0 + x * scale, 12.0 + row as f32 * scale),
                        Vec2::new(units * scale - 4.0, scale - 4.0),
                    );
                    let color = preview(hw, self.color, x / 22.0, row as f32 / 6.0, key_index, t)
                        .gamma_multiply(if self.change_brightness {
                            f32::from(self.brightness) / 100.0
                        } else {
                            0.8
                        });
                    let outline = Stroke::new(1.2, color.gamma_multiply(0.8));
                    if *label == "EnterISO" {
                        // ABNT2 Enter spans two rows, with a quarter-unit notch
                        // on its left side. The next row reserves its stem.
                        let stem_left = rect.left() + scale * 0.25;
                        let bottom = rect.bottom() + scale;
                        painter.rect_filled(rect, 3.0, color.gamma_multiply(0.12));
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(stem_left, rect.bottom() - 4.0),
                                egui::pos2(rect.right(), bottom),
                            ),
                            3.0,
                            color.gamma_multiply(0.12),
                        );
                        painter.add(egui::Shape::closed_line(
                            vec![
                                rect.left_top(),
                                rect.right_top(),
                                egui::pos2(rect.right(), bottom),
                                egui::pos2(stem_left, bottom),
                                egui::pos2(stem_left, rect.bottom()),
                                rect.left_bottom(),
                            ],
                            outline,
                        ));
                        draw_key_legend(
                            painter,
                            egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), bottom)),
                            "Enter",
                            (scale * 0.25).min(12.0),
                        );
                    } else {
                        painter.rect_filled(rect, 4.0, color.gamma_multiply(0.12));
                        painter.rect_stroke(rect, 4.0, outline, egui::StrokeKind::Inside);
                        draw_key_legend(painter, rect, label, (scale * 0.25).min(12.0));
                    }
                    key_index += 1;
                }
                x += units;
            }
        }
        ui.add_space(14.0);
        ui.label(RichText::new("Prévia ilustrativa do efeito e do layout. A animação real é executada pelo firmware.").size(12.0).color(MUTED));
        if hw != 0 && hw != 1 {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
    }
}

/// Arrows and the knob are geometry, so missing font glyphs cannot turn them
/// into replacement squares on another desktop/font configuration.
fn draw_key_legend(painter: &egui::Painter, rect: egui::Rect, label: &str, font_size: f32) {
    let color = Color32::from_gray(215);
    let direction = match label {
        "←" => Some(Vec2::new(-1.0, 0.0)),
        "→" => Some(Vec2::new(1.0, 0.0)),
        "↑" => Some(Vec2::new(0.0, -1.0)),
        "↓" => Some(Vec2::new(0.0, 1.0)),
        _ => None,
    };
    let size = rect.width().min(rect.height());
    if let Some(direction) = direction {
        let center = rect.center();
        let length = size * 0.22;
        let tip = center + direction * length;
        let perpendicular = Vec2::new(-direction.y, direction.x);
        let stroke = Stroke::new((size * 0.035).max(1.0), color);
        painter.line_segment([center - direction * length, tip], stroke);
        for sign in [-1.0, 1.0] {
            painter.line_segment(
                [
                    tip,
                    tip - direction * length * 0.65 + perpendicular * length * 0.65 * sign,
                ],
                stroke,
            );
        }
    } else if label == "Knob" {
        painter.circle_stroke(rect.center(), size * 0.34, Stroke::new(1.5, color));
        painter.circle_stroke(rect.center(), size * 0.25, Stroke::new(1.0, MUTED));
        painter.line_segment(
            [
                rect.center() - Vec2::new(0.0, size * 0.18),
                rect.center() - Vec2::new(0.0, size * 0.29),
            ],
            Stroke::new(1.5, color),
        );
    } else if label != "Space" {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(font_size),
            color,
        );
    }
}

/// Draw at an exact size: hover/selection never expands the color's rectangle.
fn color_swatch(
    ui: &mut egui::Ui,
    color: [u8; 3],
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        COLOR_SIZE,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    ui.painter().rect_filled(rect, 3.0, c32(color));
    if selected || (enabled && response.hovered()) {
        ui.painter().rect_stroke(
            rect,
            3.0,
            Stroke::new(if selected { 2.0 } else { 1.0 }, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    response
}

fn palette_heading(ui: &mut egui::Ui, title: &str, buttons: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(6.0, 2.0);
        ui.spacing_mut().interact_size.y = 22.0;
        ui.horizontal(|ui| {
            ui.set_height(22.0);
            ui.label(RichText::new(title).size(12.0).color(MUTED));
            buttons(ui);
        });
    });
}

fn color_hex([r, g, b]: [u8; 3]) -> String {
    format!("{r:02X}{g:02X}{b:02X}")
}

/// One editor shared by the active color and favorites. Partial hex input stays
/// editable; only a complete valid value changes the color. Picker/RGB changes
/// replace the hex draft so all representations agree.
fn color_fields(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    hex: &mut String,
    format: &mut ColorFormat,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .scope(|ui| {
                ui.spacing_mut().interact_size = COLOR_SIZE;
                ui.color_edit_button_srgb(color)
            })
            .inner
            .changed()
        {
            *hex = color_hex(*color);
            changed = true;
        }
        let previous = *format;
        for (value, label) in [(ColorFormat::Hex, "HEX"), (ColorFormat::Rgb, "RGB")] {
            if ui
                .add_sized(
                    [52.0, 28.0],
                    egui::Button::new(label).selected(*format == value),
                )
                .clicked()
            {
                *format = value;
            }
        }
        if *format != previous {
            // A format switch preserves the last valid color and discards an
            // incomplete hex draft without marking the color as changed.
            *hex = color_hex(*color);
        }
    });
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 28.0;
        ui.horizontal(|ui| match format {
            ColorFormat::Hex => {
                let valid = parse_hex(hex).is_some();
                ui.label("#");
                let response = ui.add_sized(
                    [85.0, 28.0],
                    egui::TextEdit::singleline(hex)
                        .horizontal_align(egui::Align::Center)
                        .vertical_align(egui::Align::Center)
                        .desired_width(85.0)
                        .min_size(Vec2::new(85.0, 28.0))
                        .char_limit(7)
                        .hint_text("RRGGBB")
                        .text_color(if valid {
                            ui.visuals().text_color()
                        } else {
                            ERROR
                        }),
                );
                if !valid {
                    response
                        .clone()
                        .on_hover_text("Hex inválido: use RRGGBB ou #RRGGBB");
                }
                if response.changed()
                    && let Some(value) = parse_hex(hex)
                {
                    *color = value;
                    changed = true;
                }
            }
            ColorFormat::Rgb => {
                let mut rgb_changed = false;
                for (channel, prefix) in color.iter_mut().zip(["R ", "G ", "B "]) {
                    rgb_changed |= ui
                        .add_sized(
                            [76.0, 28.0],
                            egui::DragValue::new(channel).range(0..=255).prefix(prefix),
                        )
                        .changed();
                }
                if rgb_changed {
                    *hex = color_hex(*color);
                    changed = true;
                }
            }
        });
    });
    changed
}

fn c32([r, g, b]: [u8; 3]) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn preview(hw: u8, base: [u8; 3], u: f32, v: f32, i: usize, t: f32) -> Color32 {
    use std::f32::consts::TAU;
    let hue = |h: f32| Color32::from(egui::ecolor::Hsva::new(h.rem_euclid(1.0), 0.8, 1.0, 1.0));
    let phase = i as f32 * 2.399;
    let pulse = |a: f32| 0.15 + 0.85 * (a.sin() * 0.5 + 0.5);
    match hw {
        0 => Color32::from_gray(35),
        1 => c32(base),
        2 => c32(base).gamma_multiply(pulse(t * 2.0)),
        3 => hue(u - t * 0.15),
        4 => c32(base).gamma_multiply((1.0 - (t * 1.5).fract()).powi(2)),
        5 | 8 | 9 | 12 => c32(base).gamma_multiply(pulse(t * 2.0 + phase).powi(5)),
        6 | 15 => hue((v - 0.4).atan2(u - 0.4) / TAU + t * 0.15),
        10 | 14 => {
            c32(base).gamma_multiply((1.0 - ((u - (t * 0.3).fract()).abs() * 7.0)).max(0.08))
        }
        11 => hue(t * 0.13),
        16 => hue(v - t * 0.4 + u * 0.2),
        17 => hue(u + v - t * 0.2).gamma_multiply(pulse((u * u + v * v).sqrt() * 20.0 - t * 3.0)),
        18 => c32(base).gamma_multiply(pulse((v - 0.4).atan2(u - 0.4) * 3.0 - t * 3.0)),
        _ => c32(base).gamma_multiply(pulse(u * TAU + v * 3.0 - t * 3.0)),
    }
}

/// Illustrative Brazilian ABNT2 layouts; no coordinates are sent to the device.
fn keyboard_rows(model: usize) -> Vec<Vec<(&'static str, f32)>> {
    let row = |labels: &'static str| {
        labels
            .split_whitespace()
            .map(|s| (s, 1.0))
            .collect::<Vec<_>>()
    };
    let mut rows = vec![
        row("Esc F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 F12"),
        row("' 1 2 3 4 5 6 7 8 9 0 - ="),
        vec![("Tab", 1.5)],
        vec![("Caps", 1.75)],
        vec![("Shift", 1.25), ("\\", 1.0)],
        vec![
            ("Ctrl", 1.25),
            ("Win", 1.25),
            ("Alt", 1.25),
            ("Space", 6.25),
            ("AltGr", 1.0),
            ("Fn", 1.0),
            ("Ctrl", 1.0),
        ],
    ];
    rows[1].push(("Bksp", 2.0));
    rows[2].extend(row("Q W E R T Y U I O P ´ ["));
    rows[2].push(("EnterISO", 1.5));
    rows[3].extend(row("A S D F G H J K L Ç ~ ]"));
    rows[3].push(("", 1.25)); // lower stem of the two-row Enter
    rows[4].extend(row("Z X C V B N M , . ; /"));
    rows[4].push(("Shift", if model == 0 { 0.75 } else { 1.75 }));
    if model == 0 {
        rows[0] = vec![("Esc", 1.0), ("", 0.5)];
        rows[0].extend(row("F1 F2 F3 F4"));
        rows[0].push(("", 0.5));
        rows[0].extend(row("F5 F6 F7 F8"));
        rows[0].push(("", 0.5));
        rows[0].extend(row("F9 F10 F11 F12"));
        rows[0].extend([("", 0.5), ("Knob", 1.0)]);
        for (r, label) in [(1, "Del"), (2, "PgUp"), (3, "PgDn")] {
            rows[r].push((label, 1.0));
        }
        rows[4].extend([("↑", 1.0), ("End", 1.0)]);
        rows[5].extend(row("← ↓ →"));
    } else {
        let nav = [
            "Prt Scr Pause",
            "Ins Home PgUp",
            "Del End PgDn",
            "",
            "_ ↑ _",
            "← ↓ →",
        ];
        for (r, labels) in nav.iter().enumerate() {
            rows[r].push(("", 0.4));
            rows[r].extend(
                row(labels)
                    .into_iter()
                    .map(|(s, w)| (if s == "_" { "" } else { s }, w)),
            );
        }
        if model == 2 {
            let pad = ["", "Num / * -", "7 8 9 +", "4 5 6", "1 2 3 Ent", "0 ."];
            for (r, labels) in pad.iter().enumerate() {
                rows[r].push(("", 0.4));
                rows[r].extend(row(labels));
            }
        }
    }
    rows
}

impl App {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        self.receive();
        egui::Panel::left("effects")
            .exact_size(245.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(14.0))
            .show(ui, |ui| self.effects(ui));
        egui::Panel::bottom("controls")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(24, 18)),
            )
            .show(ui, |ui| self.controls(ui));
        let content = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24.0))
            .show(ui, |ui| {
                self.header(ui);
                self.keyboard(ui);
            })
            .response
            .rect;
        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        content
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.show(ui);
        if self.palette_dirty
            && let Some(storage) = frame.storage_mut()
        {
            self.save(storage);
            storage.flush();
            self.palette_dirty = false;
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(palettes::STORAGE_KEY, self.palettes.encode());
    }
}

fn main() -> eframe::Result {
    if let (Some(old), Some(new)) = (
        eframe::storage_dir("aula-gui"),
        eframe::storage_dir("pomni"),
    ) && let Err(error) = preferences::migrate(&old.join("app.ron"), &new.join("app.ron"))
    {
        eprintln!("Não foi possível importar as preferências anteriores: {error}");
    }
    eframe::run_native(
        "Pomni",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Pomni")
                .with_app_id("pomni")
                .with_inner_size([1280.0, 820.0])
                .with_min_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        app: App,
        jobs: Receiver<Job>,
        results: Sender<Result<Outcome, String>>,
    }

    fn harness() -> Harness {
        let (tx, jobs) = mpsc::channel();
        let (results, rx) = mpsc::channel();
        let app = App {
            tx,
            rx,
            device: Some(DeviceStatus {
                configurable: true,
                model: Some("F75"),
                ..Default::default()
            }),
            effect: 0,
            color: [76, 181, 232],
            hex: "4CB5E8".into(),
            change_color: false,
            brightness: 70,
            change_brightness: false,
            preview_model: 0,
            status: Ok("Pronto".into()),
            busy: false,
            palettes: PaletteBook::default(),
            palette_dirty: false,
            editing_favorites: false,
            editing_favorite: 0,
            favorite_hex: color_hex(SWATCHES[0]),
            color_format: ColorFormat::default(),
            favorite_format: ColorFormat::default(),
        };
        Harness { app, jobs, results }
    }

    #[test]
    fn history_records_confirmed_payload_only() {
        let Harness {
            mut app,
            jobs,
            results,
        } = harness();
        let applied = [10, 20, 30];
        app.select_color(applied);
        assert!(app.palettes.recent.is_empty());
        assert!(jobs.try_recv().is_err());
        // A failed operation or one preserving color adds no history entry.
        results.send(Err("Falha simulada".into())).unwrap();
        app.receive();
        assert!(app.palettes.recent.is_empty());
        results
            .send(Ok(Outcome::Applied {
                message: "Brilho alterado".into(),
                color: None,
            }))
            .unwrap();
        app.receive();
        assert!(app.palettes.recent.is_empty());
        assert!(matches!(jobs.try_recv(), Ok(Job::Refresh)));
        // Save the color acknowledged by the worker, even if the preview differs.
        app.select_color([90, 80, 70]);
        results
            .send(Ok(Outcome::Applied {
                message: "Cor confirmada".into(),
                color: Some(applied),
            }))
            .unwrap();
        app.receive();
        assert_eq!(app.palettes.recent, vec![applied]);
        assert!(app.palette_dirty);
        assert!(matches!(jobs.try_recv(), Ok(Job::Refresh)));
    }

    #[test]
    fn format_and_favorite_editing_keep_keyboard_area_unchanged() {
        let mut baseline = None;
        for editing in [false, true] {
            for format in [ColorFormat::Hex, ColorFormat::Rgb] {
                for history_filled in [false, true] {
                    let ctx = egui::Context::default();
                    ctx.set_theme(egui::Theme::Dark);
                    ctx.style_mut_of(egui::Theme::Dark, |s| {
                        s.spacing.item_spacing = Vec2::new(10.0, 10.0);
                        s.spacing.button_padding = Vec2::new(14.0, 8.0);
                    });
                    let Harness {
                        mut app,
                        results: _results,
                        jobs: _jobs,
                    } = harness();
                    app.editing_favorites = editing;
                    app.color_format = format;
                    app.favorite_format = format;
                    if history_filled {
                        for color in SWATCHES {
                            app.palettes.remember(color);
                        }
                    }
                    let mut content = egui::Rect::NOTHING;
                    for _ in 0..3 {
                        let _ = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    Vec2::new(1100.0, 760.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                content = app.show(ui);
                            },
                        );
                    }
                    let expected = *baseline.get_or_insert(content);
                    assert_eq!(
                        content,
                        expected,
                        "keyboard area moved: editing={editing}, rgb={}, history={history_filled}",
                        format == ColorFormat::Rgb
                    );
                }
            }
        }
    }

    #[test]
    fn swatches_have_identical_bounds_for_every_color_and_selection() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for color in SWATCHES {
                for selected in [false, true] {
                    assert_eq!(
                        color_swatch(ui, color, selected, true).rect.size(),
                        COLOR_SIZE
                    );
                }
            }
        });
    }

    #[test]
    fn all_effects_and_layouts_render_at_minimum_window_size_without_sending_jobs() {
        let ctx = egui::Context::default();
        let Harness {
            mut app,
            jobs,
            results: _results,
        } = harness();
        for model in 0..3 {
            app.preview_model = model;
            for effect in 0..rgb::EFFECTS.len() {
                app.effect = effect;
                app.editing_favorites = effect % 2 == 0;
                app.color_format = if model == 1 {
                    ColorFormat::Rgb
                } else {
                    ColorFormat::Hex
                };
                app.favorite_format = if model == 2 {
                    ColorFormat::Rgb
                } else {
                    ColorFormat::Hex
                };
                if model == 1 {
                    for color in SWATCHES {
                        app.palettes.remember(color);
                    }
                }
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1100.0, 760.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.show(ui);
                    },
                );
                assert!(!output.shapes.is_empty());
                assert!(
                    ctx.globally_used_rect().max.x <= 1100.5,
                    "layout {model}, effect {effect} overflows horizontally"
                );
                assert!(
                    jobs.try_recv().is_err(),
                    "rendering must never send settings"
                );
            }
        }
    }
}

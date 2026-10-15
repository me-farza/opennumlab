//! Source-line controls follow the actual text layout, including wrapped rows.

use crate::{GREEN, MUTED, RED};
use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Vec2};

pub fn width(line_count: usize) -> f32 {
    28.0 + line_count.to_string().len() as f32 * 8.0
}

pub fn show(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    gutter: Rect,
    breakpoints: &[usize],
    paused_line: Option<usize>,
    enabled: bool,
) -> Option<usize> {
    let mut clicked = None;
    for (line, row) in line_rows(&output.galley) {
        let rect = Rect::from_min_size(
            Pos2::new(gutter.left(), output.galley_pos.y + row.pos.y),
            Vec2::new(gutter.width(), row.size.y),
        );
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let response = ui.interact(
            rect,
            output.response.id.with(("breakpoint_line", line)),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                enabled,
                format!("Toggle breakpoint at line {line}"),
            )
        });
        let marked = breakpoints.contains(&line);
        let marker = Pos2::new(rect.left() + 9.0, rect.center().y);
        if marked {
            ui.painter().circle_filled(marker, 4.5, RED);
        }
        if paused_line == Some(line) {
            let tip = marker + Vec2::new(10.0, 0.0);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![tip, tip + Vec2::new(-5.0, -5.0), tip + Vec2::new(-5.0, 5.0)],
                GREEN,
                egui::Stroke::NONE,
            ));
        }
        ui.painter().text(
            Pos2::new(rect.right() - 2.0, rect.center().y),
            Align2::RIGHT_CENTER,
            line,
            FontId::monospace(13.0),
            MUTED,
        );
        if response.clicked() {
            clicked = Some(line);
        }
        response.on_hover_text(if enabled {
            if marked {
                "Clear breakpoint · F12"
            } else {
                "Set breakpoint · F12"
            }
        } else {
            "Enable the debugger and wait for an idle session to change breakpoints."
        });
    }
    clicked
}

fn line_rows(
    galley: &egui::Galley,
) -> impl Iterator<Item = (usize, &egui::epaint::text::PlacedRow)> {
    let mut line = 1;
    let mut first = true;
    galley.rows.iter().filter_map(move |row| {
        let result = first.then_some((line, row));
        first = row.ends_with_newline;
        if first {
            line += 1;
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_unicode_source_keeps_gutter_on_real_source_lines() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let galley = ui.fonts_mut(|fonts| {
                    fonts.layout(
                        "αβ = 123456789012345678901234567890;\n\nresult = αβ;\n".into(),
                        FontId::monospace(13.0),
                        MUTED,
                        70.0,
                    )
                });
                assert!(galley.rows.len() > 4);
                let rows: Vec<_> = line_rows(&galley).collect();
                assert_eq!(
                    rows.iter().map(|(line, _)| *line).collect::<Vec<_>>(),
                    [1, 2, 3, 4]
                );
                assert!(rows
                    .windows(2)
                    .all(|pair| pair[0].1.pos.y < pair[1].1.pos.y));
                assert!(rows[1].1.glyphs.is_empty());
                assert!(rows[3].1.glyphs.is_empty());
            });
        });
    }

    #[test]
    fn pointer_click_selects_the_source_line_in_a_wrapped_layout_and_respects_disabled_state() {
        let ctx = egui::Context::default();
        let mut source =
            "value = 1234567890123456789012345678901234567890;\n\nanswer = value;\n".to_owned();
        let target = std::cell::Cell::new(Pos2::ZERO);
        let clicked = std::cell::Cell::new(None);
        let mut draw = |ctx: &egui::Context, enabled| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    let (gutter, _) =
                        ui.allocate_exact_size(Vec2::new(width(4), 80.0), Sense::hover());
                    let output = egui::TextEdit::multiline(&mut source)
                        .id(egui::Id::new("gutter_pointer_proof"))
                        .font(FontId::monospace(13.0))
                        .desired_width(160.0)
                        .show(ui);
                    let (_, row) = line_rows(&output.galley)
                        .find(|(line, _)| *line == 3)
                        .unwrap();
                    target.set(Pos2::new(
                        gutter.center().x,
                        output.galley_pos.y + row.pos.y + row.size.y / 2.0,
                    ));
                    if let Some(line) = show(ui, &output, gutter, &[3], None, enabled) {
                        clicked.set(Some(line));
                    }
                });
            });
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| draw(ctx, true));
        let position = target.get();
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| draw(ctx, true),
            );
        }
        assert_eq!(clicked.get(), Some(3));
        clicked.set(None);
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| draw(ctx, false),
            );
        }
        assert_eq!(clicked.get(), None);
        assert_eq!(
            source,
            "value = 1234567890123456789012345678901234567890;\n\nanswer = value;\n"
        );
    }
}

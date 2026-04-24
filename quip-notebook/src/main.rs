use std::f32;

use eframe::egui;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

fn main() {
  let native_options = eframe::NativeOptions::default();
  eframe::run_native(
    "My egui App",
    native_options,
    Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
  )
  .unwrap();
}

#[derive(Debug, Default)]
struct Cell {
  id: usize,
  code: String,
  result: Option<Result<String, String>>,
}

impl Cell {
  fn with_id(mut self, id: usize) -> Self {
    self.id = id;
    self
  }
}

#[derive(Default)]
struct MyEguiApp {
  cells: Vec<Cell>,
  idx: usize,
}

impl MyEguiApp {
  fn new(_: &eframe::CreationContext<'_>) -> Self {
    Self {
      cells: vec![Cell::default()],
      idx: 1,
    }
  }
}

impl eframe::App for MyEguiApp {
  fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
      egui::ScrollArea::vertical().show(ui, |ui| {
        for i in 0..self.cells.len() {
          if ui.button("Add above").clicked() {
            self.cells.insert(i, Cell::default().with_id(self.idx));
            self.idx += 1;
          }
          {
            let cell = self.cells.get_mut(i).unwrap();
            ui.horizontal(|ui| {
              ui.label(format!("[{}]: ", cell.id));
              CodeEditor::default()
                .id_source(format!("cell {}", cell.id))
                .vscroll(false)
                .with_fontsize(14.0)
                .with_theme(ColorTheme::GRUVBOX)
                .with_syntax(Syntax::rust())
                .with_numlines(true)
                .show(ui, &mut cell.code);
            });
          }
          if ui.button("Add below").clicked() {
            self.cells.insert(i + 1, Cell::default().with_id(self.idx));
            self.idx += 1;
          }
        }
      })
    });
  }
}

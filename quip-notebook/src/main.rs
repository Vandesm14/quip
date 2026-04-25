use std::{net::TcpStream, sync::mpsc, thread};

use eframe::egui;
use egui::{Event, Key, Modifiers, RichText};
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use quip_notebook::{Request, Response, read_framed_json, write_framed_json};

fn main() {
  let native_options = eframe::NativeOptions::default();
  let (request_sender, request_receiver) = mpsc::channel();
  let (reply_sender, reply_receiver) = mpsc::channel();

  thread::spawn(move || {
    let mut stream = TcpStream::connect("127.0.0.1:7478").unwrap();
    for req in request_receiver {
      if write_framed_json(&mut stream, &req).is_err() {
        break;
      }
      if let Request::Eval { .. } = &req {
        let resp: Result<Response, _> = read_framed_json(&mut stream);
        if let Ok(resp) = resp {
          if reply_sender.send(resp).is_err() {
            break;
          }
        } else {
          break;
        }
      }
    }
  });

  eframe::run_native(
    "Quip",
    native_options,
    Box::new(|cc| {
      Ok(Box::new(NotebookApp::new(
        cc,
        (request_sender, reply_receiver),
      )))
    }),
  )
  .unwrap();
}

#[derive(Debug, Default)]
struct Cell {
  id: usize,
  code: String,
  result: Option<Result<String, String>>,
  pending: bool,
  run_count: u32,
}

impl Cell {
  fn with_id(mut self, id: usize) -> Self {
    self.id = id;
    self
  }
}

type Channel = (mpsc::Sender<Request>, mpsc::Receiver<Response>);

struct NotebookApp {
  channel: Channel,
  cells: Vec<Cell>,
  next_id: usize,
  /// Active cell: keyboard shortcuts and highlight target.
  selected: usize,
  /// One-shot: focus the editor for this index after navigation.
  focus_index: Option<usize>,
  /// If true, a cell asked for focus this frame; clear [focus_index] at end of frame.
  focus_served: bool,
}

impl NotebookApp {
  fn new(_: &eframe::CreationContext<'_>, channel: Channel) -> Self {
    Self {
      channel,
      cells: vec![Cell::default()],
      next_id: 1,
      selected: 0,
      focus_index: None,
      focus_served: false,
    }
  }

  fn insert_cell(&mut self, at: usize) {
    let id = self.next_id;
    self.next_id += 1;
    self.cells.insert(at, Cell::default().with_id(id));
    self.selected = at;
    self.focus_index = Some(at);
  }

  fn run_cell(&mut self, index: usize) {
    if let Some(cell) = self.cells.get_mut(index) {
      cell.pending = true;
      let _ = self.channel.0.send(Request::Eval {
        id: cell.id,
        source: cell.code.clone(),
      });
    }
  }

  fn handle_notebook_keys(&mut self, ctx: &egui::Context) {
    let (cmd_enter, shift_enter) = ctx.input_mut(|i| {
      let mut cmd_enter = false;
      let mut shift_enter = false;
      i.events.retain(|event: &Event| {
        if let Event::Key {
          key: Key::Enter,
          pressed: true,
          repeat: false,
          modifiers,
          ..
        } = event
        {
          if modifiers.matches_exact(Modifiers::COMMAND) {
            cmd_enter = true;
            return false;
          }
          if modifiers.matches_exact(Modifiers::SHIFT) {
            shift_enter = true;
            return false;
          }
        }
        true
      });
      (cmd_enter, shift_enter)
    });

    if self.cells.is_empty() {
      return;
    }
    self.selected = self.selected.min(self.cells.len() - 1);

    if cmd_enter {
      self.run_cell(self.selected);
    }
    if shift_enter {
      self.run_cell(self.selected);
      let n = self.cells.len();
      let s = self.selected;
      if s + 1 < n {
        self.selected = s + 1;
        self.focus_index = Some(self.selected);
      } else {
        self.insert_cell(n);
      }
    }
  }

  fn jupyter_in_prompt(cell: &Cell) -> String {
    if cell.pending {
      "In [*]:".to_string()
    } else if cell.run_count == 0 {
      "In [ ]:".to_string()
    } else {
      format!("In [{}]:", cell.run_count)
    }
  }
}

impl eframe::App for NotebookApp {
  fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
    let ctx = ui.ctx().clone();
    for reply in self.channel.1.try_iter() {
      match reply {
        Response::Eval { id, result } => {
          if let Some(ref mut cell) = self.cells.iter_mut().find(|c| c.id == id)
          {
            cell.pending = false;
            cell.run_count = cell.run_count.saturating_add(1);
            cell.result = Some(result);
          }
        }
      }
    }

    self.handle_notebook_keys(&ctx);
    self.focus_served = false;

    let dark = ctx.global_style().visuals.dark_mode;
    let sel_accent = if dark {
      egui::Color32::from_rgb(68, 138, 201)
    } else {
      egui::Color32::from_rgb(40, 96, 160)
    };
    let code_theme = if dark {
      ColorTheme::GITHUB_DARK
    } else {
      ColorTheme::GITHUB_LIGHT
    };
    let cell_fill = if dark {
      egui::Color32::from_rgba_premultiplied(35, 45, 60, 80)
    } else {
      egui::Color32::from_rgba_premultiplied(250, 250, 255, 120)
    };

    egui::CentralPanel::default().show_inside(ui, |ui| {
      egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
          if ui
            .add_sized(
              [ui.available_width().max(120.0), 0.0],
              egui::Button::new(RichText::new("new cell").weak()).frame(false),
            )
            .clicked()
          {
            self.insert_cell(0);
          }
          if !self.cells.is_empty() {
            ui.add_space(2.0);
          }

          let n = self.cells.len();
          for i in 0..n {
            if i > 0 {
              ui.add_space(4.0);
              if ui
                .add_sized(
                  [ui.available_width().max(80.0), 0.0],
                  egui::Button::new(RichText::new("new cell").weak())
                    .frame(false),
                )
                .clicked()
              {
                self.insert_cell(i);
              }
            }

            let cell_snapshot = if let Some(c) = self.cells.get(i) {
              c
            } else {
              continue;
            };
            let in_prompt = Self::jupyter_in_prompt(cell_snapshot);
            let selected = self.selected == i;

            let _frame = egui::Frame::new()
              .inner_margin(egui::Margin::symmetric(8, 6))
              .fill(if selected {
                cell_fill
              } else {
                egui::Color32::TRANSPARENT
              })
              .stroke(if selected {
                egui::Stroke::new(2.0, sel_accent)
              } else {
                egui::Stroke::NONE
              })
              .show(ui, |ui| {
                ui.set_width(ui.available_width().max(100.0));
                ui.vertical(|ui| {
                  ui.horizontal_top(|ui| {
                    if ui
                      .add_sized(
                        [76.0, 0.0],
                        egui::Button::new(
                          RichText::new(&in_prompt)
                            .monospace()
                            .size(12.0)
                            .color(if selected {
                              sel_accent
                            } else {
                              ui.style().visuals.weak_text_color()
                            }),
                        )
                        .frame(false)
                        .min_size(egui::vec2(0.0, 20.0)),
                      )
                      .clicked()
                    {
                      self.selected = i;
                      self.focus_index = Some(i);
                    }
                    if ui.button("Run ▶").clicked() {
                      self.selected = i;
                      self.run_cell(i);
                    }
                  });
                  let id = {
                    let cell = self.cells.get(i).expect("i in range");
                    cell.id
                  };
                  let out = {
                    let cell = self.cells.get_mut(i).expect("i in range");
                    CodeEditor::default()
                      .id_source(format!("quip_cell_{}", id))
                      .vscroll(false)
                      .with_rows(4)
                      .with_numlines(false)
                      .with_fontsize(13.0)
                      .with_theme(code_theme)
                      .with_syntax(Syntax::rust())
                      .show(ui, &mut cell.code)
                  };
                  {
                    if out.response.has_focus() {
                      self.selected = i;
                    }
                    if self.focus_index.is_some_and(|ix| ix == i) {
                      out.response.request_focus();
                      self.focus_served = true;
                    }
                  }
                  {
                    let cell = self.cells.get(i).expect("i in range");
                    if let Some(ref r) = cell.result {
                      ui.add_space(2.0);
                      let monos = ui
                        .style()
                        .text_styles
                        .get(&egui::TextStyle::Monospace)
                        .map(|f| f.size * 0.9)
                        .unwrap_or(12.0);
                      match r {
                        Ok(s) if !s.is_empty() => {
                          ui.label(
                            RichText::new(format!("Out: {s}"))
                              .monospace()
                              .size(monos)
                              .color(if dark {
                                egui::Color32::from_rgb(160, 210, 185)
                              } else {
                                egui::Color32::from_rgb(0, 110, 50)
                              }),
                          );
                        }
                        Ok(_) => {}
                        Err(e) => {
                          ui.label(
                            RichText::new(format!("Error: {e}"))
                              .monospace()
                              .size(monos)
                              .color(if dark {
                                egui::Color32::from_rgb(255, 150, 130)
                              } else {
                                egui::Color32::from_rgb(195, 45, 25)
                              }),
                          );
                        }
                      }
                    }
                  }
                });
              });
          }
          if ui
            .add_sized(
              [ui.available_width().max(120.0), 0.0],
              egui::Button::new(RichText::new("new cell").weak()).frame(false),
            )
            .clicked()
          {
            self.insert_cell(self.cells.len());
          }
        });
    });
    if self.focus_served {
      self.focus_index = None;
    }
  }
}

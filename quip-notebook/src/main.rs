use std::{net::TcpStream, sync::mpsc, thread};

use eframe::egui;
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
    "My egui App",
    native_options,
    Box::new(|cc| {
      Ok(Box::new(MyEguiApp::new(
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
}

impl Cell {
  fn with_id(mut self, id: usize) -> Self {
    self.id = id;
    self
  }
}

type Channel = (mpsc::Sender<Request>, mpsc::Receiver<Response>);

struct MyEguiApp {
  channel: Channel,
  cells: Vec<Cell>,
  idx: usize,
}

impl MyEguiApp {
  fn new(_: &eframe::CreationContext<'_>, channel: Channel) -> Self {
    Self {
      channel,
      cells: vec![Cell::default()],
      idx: 1,
    }
  }
}

impl eframe::App for MyEguiApp {
  fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
    for reply in self.channel.1.try_iter() {
      match reply {
        Response::Eval { id, result } => {
          if let Some(ref mut cell) =
            self.cells.iter_mut().find(|cell| cell.id == id)
          {
            cell.result = Some(result);
          }
        }
      }
    }

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
            if ui.button("Run").clicked() {
              self
                .channel
                .0
                .send(Request::Eval {
                  id: cell.id,
                  source: cell.code.clone(),
                })
                .unwrap();
            }
            ui.label(format!("{:?}", cell.result));
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

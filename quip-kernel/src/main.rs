//! `quip-kernel` - Jupyter kernel for the Quip language.
//!
//! Invoked by a Jupyter frontend (e.g. Zed's REPL) with a single argument:
//! the path to a connection file containing ZeroMQ ports and a signing key.
//!
//! Usage (typically automated by Jupyter):
//! ```sh
//! quip-kernel <connection-file>
//! ```

use std::{path::PathBuf, process::ExitCode};

use quip_kernel::{connection::ConnectionInfo, server};

fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().collect();

  if matches!(args.get(1).map(String::as_str), Some("--help" | "-h")) {
    print_help(&args[0]);
    return ExitCode::SUCCESS;
  }

  if matches!(args.get(1).map(String::as_str), Some("install")) {
    match install::install_kernelspec() {
      Ok(path) => {
        println!("installed Quip kernel spec at: {}", path.display());
        return ExitCode::SUCCESS;
      }
      Err(e) => {
        eprintln!("install failed: {}", e);
        return ExitCode::from(1);
      }
    }
  }

  let Some(connection_arg) = args.get(1) else {
    eprintln!("usage: quip-kernel <connection-file>");
    eprintln!("       quip-kernel install");
    return ExitCode::from(2);
  };

  let info = match ConnectionInfo::from_file(PathBuf::from(connection_arg)) {
    Ok(info) => info,
    Err(e) => {
      eprintln!("error: {}", e);
      return ExitCode::from(1);
    }
  };

  let runtime = match tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
  {
    Ok(r) => r,
    Err(e) => {
      eprintln!("error: failed to build tokio runtime: {}", e);
      return ExitCode::from(1);
    }
  };

  match runtime.block_on(server::run(info)) {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      eprintln!("kernel error: {}", e);
      ExitCode::from(1)
    }
  }
}

fn print_help(exe: &str) {
  println!(
    r#"quip-kernel - Jupyter kernel for Quip

USAGE:
    {exe} <connection-file>     Start the kernel (invoked by Jupyter/Zed)
    {exe} install               Install the kernel spec for the current user
    {exe} --help                Print this help

The kernel speaks the Jupyter 5.3 wire protocol over ZeroMQ.
"#,
    exe = exe
  );
}

/// Installation support: write a `kernel.json` + companion files into the
/// user-scope Jupyter kernelspec directory.
mod install {
  use std::{
    fs,
    path::{Path, PathBuf},
  };

  pub fn install_kernelspec() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
      .map_err(|e| format!("cannot resolve current exe: {}", e))?;
    let dir = kernelspec_dir()?;
    fs::create_dir_all(&dir)
      .map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;

    let spec = format!(
      r#"{{
  "display_name": "Quip",
  "language": "quip",
  "argv": [
    "{}",
    "{{connection_file}}"
  ],
  "interrupt_mode": "message",
  "metadata": {{
    "debugger": false
  }}
}}
"#,
      exe.display()
    );

    let spec_path = dir.join("kernel.json");
    fs::write(&spec_path, spec)
      .map_err(|e| format!("cannot write {}: {}", spec_path.display(), e))?;

    Ok(spec_path)
  }

  fn kernelspec_dir() -> Result<PathBuf, String> {
    // Per Jupyter's platform defaults.
    // See https://jupyter.readthedocs.io/en/latest/use/jupyter-directories.html
    if cfg!(target_os = "macos") {
      let home = std::env::var_os("HOME")
        .ok_or_else(|| "HOME is not set".to_string())?;
      Ok(Path::new(&home).join("Library/Jupyter/kernels/quip"))
    } else if cfg!(target_os = "windows") {
      let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA is not set".to_string())?;
      Ok(Path::new(&appdata).join("jupyter/kernels/quip"))
    } else {
      // Linux / other: follow XDG_DATA_HOME, fall back to ~/.local/share.
      let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
          std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share"))
        })
        .ok_or_else(|| "HOME is not set".to_string())?;
      Ok(base.join("jupyter/kernels/quip"))
    }
  }
}

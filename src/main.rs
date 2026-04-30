#![warn(clippy::all, clippy::pedantic)]
mod editor;
use editor::Editor;

fn main() {
    match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [flag] if matches!(flag.as_str(), "--version" | "-V") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        [] => run_editor(None),
        [arg] if arg.starts_with('-') => {
            eprintln!("Unknown option: {arg}");
            std::process::exit(1);
        }
        [path] => run_editor(Some(path)),
        _ => {
            eprintln!("Too many arguments");
            std::process::exit(1);
        }
    }
}

fn run_editor(path: Option<&str>) {
    match Editor::new(path) {
        Ok(mut editor) => editor.run(),
        Err(err) => {
            eprintln!("Failed to start editor: {err}");
            std::process::exit(1);
        }
    }
}

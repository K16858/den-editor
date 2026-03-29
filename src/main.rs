#![warn(clippy::all, clippy::pedantic)]
mod editor;
use editor::Editor;

fn main() {
    match Editor::new() {
        Ok(mut editor) => editor.run(),
        Err(err) => {
            eprintln!("Failed to start editor: {err}");
            std::process::exit(1);
        }
    }
}

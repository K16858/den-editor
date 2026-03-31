use super::super::{Position, Size, command::MoveDirection, terminal::Terminal};
use super::UIComponent;
use crossterm::style::Attribute::{Reset, Reverse};
use std::collections::HashSet;
use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone)]
struct VisibleEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    depth: usize,
}

pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    visible: Vec<VisibleEntry>,
    selected: usize,
    scroll: usize,
    size: Size,
    needs_redraw: bool,
    pending_open: Option<PathBuf>,
}

impl FileTree {
    pub const WIDTH: usize = 24;
    const CONTENT_WIDTH: usize = Self::WIDTH - 1;

    pub fn new(root: PathBuf) -> Self {
        let mut tree = Self {
            root,
            expanded: HashSet::new(),
            visible: Vec::new(),
            selected: 0,
            scroll: 0,
            size: Size::default(),
            needs_redraw: true,
            pending_open: None,
        };
        tree.rebuild();
        tree
    }

    pub fn take_pending_open(&mut self) -> Option<PathBuf> {
        self.pending_open.take()
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.visible.get(self.selected).map(|e| e.path.clone())
    }

    pub fn workspace_root(&self) -> &Path {
        &self.root
    }

    pub fn rebuild(&mut self) {
        let sel = self.visible.get(self.selected).map(|e| e.path.clone());
        self.visible.clear();
        let root = self.root.clone();
        let _ = self.append_children(&root, 0);
        self.clamp_selected();
        if let Some(p) = sel
            && let Some(i) = self.visible.iter().position(|e| e.path == p)
        {
            self.selected = i;
        }
        self.ensure_scroll();
    }

    fn append_children(&mut self, dir: &Path, depth: usize) -> Result<(), Error> {
        let read = fs::read_dir(dir)?;
        let mut entries: Vec<_> = read.filter_map(std::result::Result::ok).collect();
        entries.sort_by(|a, b| {
            let pa = a.path();
            let pb = b.path();
            let da = pa.is_dir();
            let db = pb.is_dir();
            match (da, db) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => pa
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&pb.to_string_lossy().to_lowercase()),
            }
        });
        for e in entries {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = path.is_dir();
            self.visible.push(VisibleEntry {
                path: path.clone(),
                name,
                is_dir,
                depth,
            });
            if is_dir && self.expanded.contains(&path) {
                let _ = self.append_children(&path, depth.saturating_add(1));
            }
        }
        Ok(())
    }

    fn clamp_selected(&mut self) {
        if self.visible.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        }
    }

    fn ensure_scroll(&mut self) {
        let h = self.size.height;
        if h == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(h) {
            self.scroll = self.selected.saturating_sub(h.saturating_sub(1));
        }
    }

    pub fn handle_move(&mut self, direction: MoveDirection) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        match direction {
            MoveDirection::Up | MoveDirection::ScrollUp => {
                self.selected = self.selected.saturating_sub(1);
            }
            MoveDirection::Down | MoveDirection::ScrollDown => {
                self.selected = (self.selected + 1).min(self.visible.len().saturating_sub(1));
            }
            MoveDirection::Left => {
                let Some(entry) = self.visible.get(self.selected) else {
                    return false;
                };
                if entry.is_dir && self.expanded.remove(&entry.path) {
                    self.rebuild();
                }
                return true;
            }
            MoveDirection::Right => {
                let Some(entry) = self.visible.get(self.selected) else {
                    return false;
                };
                if entry.is_dir && !self.expanded.contains(&entry.path) {
                    self.expanded.insert(entry.path.clone());
                    self.rebuild();
                }
                return true;
            }
            _ => return false,
        }
        self.ensure_scroll();
        true
    }

    pub fn handle_enter(&mut self) -> bool {
        let Some(entry) = self.visible.get(self.selected) else {
            return false;
        };
        if entry.is_dir {
            if self.expanded.contains(&entry.path) {
                self.expanded.remove(&entry.path);
            } else {
                self.expanded.insert(entry.path.clone());
            }
            self.rebuild();
            true
        } else {
            self.pending_open = Some(entry.path.clone());
            true
        }
    }

    pub fn caret_position(&self, origin_y: usize) -> Position {
        let row = origin_y + self.selected.saturating_sub(self.scroll);
        Position { row, col: 0 }
    }
}

fn format_line(entry: &VisibleEntry, is_selected: bool) -> String {
    let indent = "  ".repeat(entry.depth);
    let label = if entry.is_dir {
        format!("{indent}{}/", entry.name)
    } else {
        format!("{indent}{}", entry.name)
    };
    let padded = pad_or_truncate(&label, FileTree::CONTENT_WIDTH);
    if is_selected {
        format!("{Reverse}{padded}{Reset}|")
    } else {
        format!("{padded}|")
    }
}

fn pad_or_truncate(s: &str, max_display_width: usize) -> String {
    if s.width() <= max_display_width {
        let pad = max_display_width.saturating_sub(s.width());
        return format!("{s}{}", " ".repeat(pad));
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_display_width.saturating_sub(1) {
            out.push('…');
            break;
        }
        out.push(ch);
        w += cw;
    }
    let pad = max_display_width.saturating_sub(out.width());
    format!("{out}{}", " ".repeat(pad))
}

impl UIComponent for FileTree {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, size: Size) {
        self.size = size;
        self.ensure_scroll();
    }

    fn draw(&mut self, origin_y: usize) -> Result<(), Error> {
        let h = self.size.height;
        for row in 0..h {
            let idx = self.scroll + row;
            let draw_row = origin_y + row;
            let line = if let Some(entry) = self.visible.get(idx) {
                format_line(entry, idx == self.selected)
            } else {
                format!("{}|", " ".repeat(Self::CONTENT_WIDTH))
            };
            Terminal::move_caret_to(Position {
                row: draw_row,
                col: 0,
            })?;
            Terminal::print(&line)?;
        }
        Ok(())
    }
}

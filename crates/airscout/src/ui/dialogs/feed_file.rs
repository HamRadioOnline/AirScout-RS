//! Path entry and a listing of the application's Tmp directory (Decision 7).

use crate::feed_files::feed_file_name;
use eframe::egui::{self, Button, ScrollArea, TextEdit};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum FeedFileKind {
    Import,
    Export,
}

pub(crate) struct FeedFileRequest {
    pub kind: FeedFileKind,
    pub slot: usize,
    pub path: PathBuf,
}

pub(crate) struct FeedFileDlg {
    kind: FeedFileKind,
    slot: usize,
    tmp: Option<PathBuf>,
    files: Vec<String>,
    path: String,
    hint: String,
    replace: Option<PathBuf>,
}

pub(crate) enum FeedFileResult {
    Open,
    Cancel,
    Submit(FeedFileRequest),
}

impl FeedFileDlg {
    pub fn new(kind: FeedFileKind, slot: usize, feed: &str, tmp: Option<PathBuf>) -> Self {
        let files = tmp.as_deref().map(list_feed_files).unwrap_or_default();
        let export = matches!(kind, FeedFileKind::Export);
        let hint = feed_file_name(feed, export);
        Self {
            kind,
            slot,
            tmp,
            files,
            path: if export { hint.clone() } else { String::new() },
            hint,
            replace: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> FeedFileResult {
        let export = matches!(self.kind, FeedFileKind::Export);
        let caption = if export { "Export" } else { "Import" };
        let mut result = FeedFileResult::Open;
        let modal = egui::Modal::new(egui::Id::unique("options_feed_file")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 32.0).clamp(0.0, 520.0));
            ui.heading(format!("{caption} Plane Feed {}", self.slot));
            let label = ui.label("File path");
            let edit = ui.add(
                TextEdit::singleline(&mut self.path)
                    .hint_text(&self.hint)
                    .desired_width(ui.available_width()),
            );
            if edit.labelled_by(label.id).changed() {
                self.replace = None;
            }
            if let Some(tmp) = &self.tmp {
                ui.label("Relative paths are resolved in the data directory's Tmp folder.");
                if ui.button("Refresh list").clicked() {
                    self.files = list_feed_files(tmp);
                }
                ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    for name in &self.files {
                        if ui.selectable_label(self.path == *name, name).clicked() {
                            self.path.clone_from(name);
                            self.replace = None;
                        }
                    }
                });
            }
            let entered = Path::new(self.path.trim());
            let path = match &self.tmp {
                Some(tmp) => tmp.join(entered),
                None => entered.to_path_buf(),
            };
            let replacing = self.replace.as_ref() == Some(&path);
            if replacing {
                ui.label("Replace the existing feed file?");
            }
            ui.horizontal(|ui| {
                let submit = if replacing { "Replace" } else { caption };
                if ui
                    .add_enabled(!entered.as_os_str().is_empty(), Button::new(submit))
                    .clicked()
                {
                    if export && !replacing && path.is_file() {
                        self.replace = Some(path);
                    } else {
                        result = FeedFileResult::Submit(FeedFileRequest {
                            kind: self.kind,
                            slot: self.slot,
                            path,
                        });
                    }
                }
                if ui.button("Cancel").clicked() {
                    result = FeedFileResult::Cancel;
                }
            });
        });
        if modal.should_close() {
            return FeedFileResult::Cancel;
        }
        result
    }
}

fn list_feed_files(dir: &Path) -> Vec<String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let ext = Path::new(&name).extension()?.to_str()?;
            (ext.eq_ignore_ascii_case("feed") || ext.eq_ignore_ascii_case("toml")).then_some(name)
        })
        .collect();
    files.sort();
    files
}

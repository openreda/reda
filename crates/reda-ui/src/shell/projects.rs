//! Opening, creating and showing Projects: recent Projects, opening in place, the saved
//! view (`.reda/view.json`), the layout a Project starts with, missing Documents and the
//! fonts the engine loads.

use super::*;

impl Shell {
    /// Shows the file the OS asked a second app to open, when it belongs to the open
    /// Project: a Document file focuses that Document's tab, the Project file or folder
    /// just the window. Anything else is `rejected`, so that app opens it itself.
    pub fn open_in_place(&mut self, path: &Path) -> Result<(), CommandError> {
        let document = document_file(path);
        let project = match &document {
            Some(_) => path.parent().unwrap_or(path),
            None => path,
        };
        let wanted = store::resolve_project_file(project);
        let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if !self.project_file().is_some_and(|open| same(&open, &wanted)) {
            return Err(CommandError::rejected(format!(
                "{} is not in the open Project",
                path.display()
            )));
        }
        if let Some((stem, kind)) = document {
            let uuid = self.engine.project().and_then(|p| {
                p.documents()
                    .find(|d| d.kind == kind && d.name == stem)
                    .map(|d| d.uuid)
            });
            let Some(uuid) = uuid else {
                return Err(CommandError::not_found(format!(
                    "the open Project has no Document {}",
                    path.display()
                )));
            };
            self.run("document.open", json!({ "document": uuid }))?;
        }
        Ok(())
    }

    /// The recent Projects from `state.json`, newest first.
    pub fn recent_projects(&self) -> Vec<RecentProject> {
        let Some(dirs) = self.engine.user_dirs() else {
            return Vec::new();
        };
        state::read(dirs)
            .recent_projects
            .into_iter()
            .map(|path| RecentProject {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                missing: !path.is_file(),
                path,
            })
            .collect()
    }

    /// Forgets a recent Project.
    pub fn remove_recent(&mut self, path: &Path) {
        if let Some(dirs) = self.engine.user_dirs()
            && let Err(e) = state::update(dirs, |s| s.recent_projects.retain(|p| p != path))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// Opens the last Project, when the Preferences ask for it. A last Project that is
    /// gone is not reopened, and the status bar says so.
    pub fn reopen_last_project(&mut self) {
        let Some(dirs) = self.engine.user_dirs() else {
            return;
        };
        if !preferences::current(Some(dirs)).general.reopen_last_project {
            return;
        }
        let Some(last) = self.recent_projects().into_iter().next() else {
            return;
        };
        if last.missing {
            self.message = Some(tr!(
                "status-last-project-missing",
                project = last.name.as_str(),
                path = reda_engine::fs::display_path(&last.path)
            ));
            return;
        }
        let _ = self.open_project(&last.path);
    }

    /// Saves the open Project's view state now, as when it closes.
    pub fn save_view(&mut self) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        // The view state is disposable: a failed write loses nothing worth reporting.
        if view_state::write(&folder, &bytes).is_ok() {
            self.view_saves.saved(bytes);
        }
    }

    /// Saves the open Project's view state once it has stayed unchanged for
    /// [`VIEW_SAVE_DELAY`]. The window calls this every frame.
    pub fn save_view_if_due(&mut self, now: Instant) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        if let Some(bytes) = self.view_saves.poll(bytes, now)
            && view_state::write(&folder, &bytes).is_ok()
        {
            self.view_saves.saved(bytes);
        }
    }

    /// The open Project's folder and its view state as `view.json` bytes.
    pub(super) fn view_bytes(&self) -> Option<(PathBuf, Vec<u8>)> {
        let folder = self.engine.location()?.to_path_buf();
        let project = self.engine.project()?;
        let mut documents = self.closed_views.clone();
        documents.retain(|uuid, _| project.documents().any(|d| d.uuid == *uuid));
        for tab in &self.documents {
            documents.insert(
                tab.document,
                DocumentView {
                    camera: tab.camera,
                    layers: tab.layers.clone(),
                    ..DocumentView::default()
                },
            );
        }
        let grids = self.engine.view().get::<Grids>();
        let working = self.engine.view().get::<WorkingState>();
        for entry in project.documents() {
            let grid = grids.and_then(|g| g.picked.get(&entry.uuid).copied());
            let snap = working.and_then(|w| w.snap.get(&entry.uuid).cloned());
            let filter = working.map(|w| w.filter(entry.uuid)).unwrap_or_default();
            let view = documents.get_mut(&entry.uuid);
            let unchanged = grid.is_none() && snap.is_none() && filter.off.is_empty();
            let view = match view {
                Some(view) => view,
                None if unchanged => continue,
                None => documents.entry(entry.uuid).or_default(),
            };
            view.grid = grid;
            view.snap_mode = snap.as_ref().map(|s| s.mode);
            view.gridless = snap.as_ref().is_some_and(|s| s.gridless);
            view.hotspots_off = snap.as_ref().map(|s| {
                (s.hotspots_off.iter())
                    .map(|k| crate::snap::hotspot_name(*k).to_owned())
                    .collect()
            });
            view.selection_filter_off = filter.off.iter().map(|k| k.name().to_owned()).collect();
        }
        if let Some(pcb) = project.pcb() {
            let view = documents.entry(pcb.uuid).or_default();
            view.layers = self.board_view().layers.clone();
            view.units = Some(self.board_view().units);
        }
        // The settings windows are not kept: their edits do not outlive the window.
        let mut layout = self.layout().clone();
        layout.retain_tabs(|t| !matches!(t, TabKey::Settings(_)));
        let file = ViewFile::new(layout, documents);
        Some((folder, file.encode()))
    }

    /// Opens the Project at `path`, replacing any open one. `path` is its `.reda` file or
    /// folder, or one of its Document files (`.redasch`, `.redapcb`), which opens the
    /// Project with that Document's tab focused.
    pub fn open_project(&mut self, path: &Path) -> Result<(), CommandError> {
        let document = document_file(path);
        let project = match &document {
            Some(_) => path.parent().unwrap_or(path),
            None => path,
        };
        if let Some(dirs) = self.engine.user_dirs()
            && let Some(pid) = recovery::live_instance(dirs, project)
        {
            // Another REDA has it open: that one keeps it, and no recovery is offered.
            let name = store::resolve_project_file(project);
            let name = name.file_stem().unwrap_or_default().to_string_lossy();
            self.message = Some(tr!(
                "error-project-held",
                project = name.as_ref(),
                pid = pid.to_string()
            ));
            return Ok(());
        }
        self.run_reporting("project.open", json!({ "path": project }))?;
        if let Some((stem, kind)) = document {
            let uuid = self.engine.project().and_then(|p| {
                p.documents()
                    .find(|d| d.kind == kind && d.name == stem)
                    .map(|d| d.uuid)
            });
            if let Some(uuid) = uuid {
                self.run_reporting("document.open", json!({ "document": uuid }))?;
            }
        }
        Ok(())
    }

    /// The Fab Presets a new Project can start from (shipped, then the user's) and the
    /// default pick: the last one used, else Generic.
    pub fn fab_preset_choices(&mut self) -> (Vec<String>, String) {
        use reda_engine::commands::fab_preset::PresetListResult;
        let list = self
            .engine
            .run("fab_preset.list", json!({}))
            .ok()
            .and_then(|v| serde_json::from_value::<PresetListResult>(v).ok());
        match list {
            Some(list) => (
                list.presets.into_iter().map(|p| p.preset.name).collect(),
                list.default,
            ),
            None => (Vec::new(), "Generic".to_owned()),
        }
    }

    /// Creates the Project `name` in a new folder inside `folder`, and opens it.
    pub fn new_project(&mut self, folder: &Path, name: &str) -> Result<(), CommandError> {
        self.run_reporting("project.new", json!({ "path": folder, "name": name }))?;
        Ok(())
    }

    /// Puts the open Project at the top of the recent Projects in `state.json`.
    pub(super) fn remember_project(&mut self) {
        if let (Some(dirs), Some(file)) = (self.engine.user_dirs(), self.project_file())
            && let Err(e) = state::update(dirs, |s| s.add_recent_project(&file))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// The open Project's `.reda` file, canonical when it exists.
    pub(super) fn project_file(&self) -> Option<PathBuf> {
        let folder = self.engine.location()?;
        let name = &self.engine.project()?.props().name;
        let file = store::project_file_path(folder, name);
        Some(std::fs::canonicalize(&file).unwrap_or(file))
    }

    /// Shows the open Project as its `view.json` left it, or else in the default layout
    /// with its first sheet in the centre.
    pub(super) fn show_project(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let titles: BTreeMap<Uuid, String> = project
            .documents()
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        let first_sheet = project
            .documents()
            .find(|d| d.kind == DocumentKind::Schematic)
            .map(|d| d.uuid);
        let restored = self.engine.location().and_then(view_state::read);
        let (layout, mut views) = match restored {
            Some(file) => {
                let mut layout = file.layout;
                layout.retain_tabs(|t| match t {
                    TabKey::Document(uuid) => titles.contains_key(uuid),
                    TabKey::Start | TabKey::Settings(_) => false,
                    TabKey::Panel(_) => true,
                });
                (layout, file.documents)
            }
            None => (self.default_layout(first_sheet), BTreeMap::new()),
        };
        let pcb = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid);
        let prefs = preferences::current(self.engine.user_dirs());
        let units = prefs.pcb.default_units;
        let mut board = BoardView::new(units);
        if let Some(view) = pcb.and_then(|pcb| views.get(&pcb)) {
            board.layers = view.layers.clone();
            board.units = view.units.unwrap_or(units);
        }
        *self.board_view_mut() = board;
        // Views start on the grids the Preferences pick, unless view.json has their own.
        let kinds: BTreeMap<Uuid, DocumentKind> = self
            .engine
            .project()
            .map(|p| p.documents().map(|d| (d.uuid, d.kind)).collect())
            .unwrap_or_default();
        let mut working = WorkingState::following(&preferences::current(self.engine.user_dirs()));
        let mut picked = BTreeMap::new();
        for (document, view) in &views {
            let Some(kind) = kinds.get(document).copied() else {
                continue;
            };
            if let Some(g) = (view.grid).filter(|g| {
                grid::tables_in(&prefs.editing.grid, kind.into())
                    .row(*g)
                    .is_some()
            }) {
                picked.insert(*document, g);
            }
            let ty = kind.into();
            let mode = view
                .snap_mode
                .filter(|m| working_state::SnapMode::offered(ty).contains(m));
            if mode.is_some() || view.gridless || view.hotspots_off.is_some() {
                let mut snap = working.default_snap(ty);
                snap.mode = mode.unwrap_or(snap.mode);
                snap.gridless = view.gridless;
                if let Some(off) = &view.hotspots_off {
                    let offered = crate::snap::offered(ty);
                    snap.hotspots_off = (offered.iter())
                        .filter(|k| off.contains(crate::snap::hotspot_name(**k)))
                        .copied()
                        .collect();
                }
                working.snap.insert(*document, snap);
            }
            let list = working_state::filter_kinds(ty);
            let off: BTreeSet<_> = (view.selection_filter_off.iter())
                .filter_map(|name| working_state::FilterKind::parse(name))
                .filter(|k| list.contains(k))
                .collect();
            if !off.is_empty() {
                working.filters.insert(*document, SelectionFilter { off });
            }
        }
        if let Some(grids) = self.engine.view_mut().get_mut::<Grids>() {
            grids.follow(&prefs);
            grids.picked = picked;
        }
        self.engine.view_mut().set(working);
        self.documents.clear();
        let open: Vec<Uuid> = layout
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in open {
            let view = views.remove(&document).unwrap_or_default();
            let scene = self.scene_of(document);
            let mut tab = DocumentTab::new(document, titles[&document].clone(), scene);
            tab.camera = view.camera;
            tab.layers = view.layers;
            self.documents.push(tab);
        }
        self.closed_views = views;
        self.replace_layout(layout);
        // Library elements opened before the Project stay open in their tabs.
        for element in crate::element_editor::open_elements(&self.engine) {
            if self.layout().place(&TabKey::Document(element)).is_none() {
                self.layout_mut().open_document(element);
            }
            if self.document_tab(element).is_none() {
                let title = crate::element_editor::title(&self.engine, element);
                let scene = self.scene_of(element);
                let tab = DocumentTab::new(element, title.unwrap_or_default(), scene);
                self.documents.push(tab);
            }
        }
        self.engine.view_mut().set(Selection::default());
        self.sync_selection();
        self.messages.clear_runs();
        self.load_fonts();
        self.change_list = self.engine.project().and_then(reda_engine::sync::plan);
        let bytes = self.view_bytes().map(|(_, bytes)| bytes);
        self.view_saves = Debounce::default();
        if let Some(bytes) = bytes {
            self.view_saves.saved(bytes);
        }
    }

    /// The layout a Project opens in without saved view state: the user's default Saved
    /// Layout when there is a usable one, else the Default Layout, with `document` open.
    pub(super) fn default_layout(&self, document: Option<Uuid>) -> Layout {
        let mut layout = Layout::with_centre(Vec::new());
        if let Some(document) = document {
            layout.open_document(document);
        }
        let dirs = self.engine.user_dirs();
        let chosen = preferences::current(dirs).general.default_layout;
        let saved = dirs
            .zip(chosen)
            .and_then(|(dirs, name)| saved::load(dirs, &name).ok())
            .and_then(|saved| saved.apply_to(&layout).ok());
        saved.unwrap_or(layout)
    }

    /// Replaces the layout with `layout`. Preferences is per-user, so an open Preferences
    /// window stays open, where it was.
    pub(super) fn replace_layout(&mut self, mut layout: Layout) {
        let preferences = TabKey::Settings(SettingsWindow::Preferences);
        let keep = self.layout().place(&preferences).map(|_| {
            let window = self.layout().window_of(&preferences);
            (self.layout().windows.iter())
                .find(|w| Some(w.id) == window)
                .and_then(|w| w.rect)
        });
        if let Some(rect) = keep
            && layout.place(&preferences).is_none()
        {
            layout.open_settings(SettingsWindow::Preferences, rect);
        }
        *self.layout_mut() = layout;
    }

    /// A note in Messages for each Document whose file is missing: an error for the PCB
    /// Document, a warning for a sheet.
    pub(super) fn note_missing_documents(&mut self) {
        let folder = self.engine.location().map(Path::to_path_buf);
        let notes = (self.engine.project().into_iter())
            .flat_map(|p| p.props().missing.iter())
            .map(|entry| {
                let path = match &folder {
                    Some(folder) => reda_engine::fs::display_path(&folder.join(entry.file_name())),
                    None => entry.file_name(),
                };
                Note {
                    source: MISSING_DOCUMENTS.to_owned(),
                    document: None,
                    severity: match entry.kind {
                        DocumentKind::Pcb => Severity::Error,
                        DocumentKind::Schematic => Severity::Warning,
                    },
                    text: tr!(
                        "message-missing-document",
                        name = entry.title.as_str(),
                        path = path.as_str()
                    ),
                }
            })
            .collect();
        self.messages.set_notes(MISSING_DOCUMENTS, notes);
    }

    /// Hands the engine the installed fonts that PCB text names and it does not have yet.
    pub(super) fn load_fonts(&mut self) {
        self.note_missing_documents();
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            self.messages.set_notes(FONTS, Vec::new());
            return;
        };
        let wanted: BTreeSet<String> = pcb
            .texts()
            .map(|t| t.font.clone())
            .filter(|f| f != HERSHEY_FONT && f != NOTO_SANS_FONT)
            .filter(|f| !self.asked_fonts.contains(f))
            .collect();
        for family in wanted {
            if let Some(face) = self.fonts.load(&family) {
                self.engine
                    .fonts_mut()
                    .add_face(family.clone(), face.bytes, face.index);
            }
            self.asked_fonts.insert(family);
        }
        let missing = reda_engine::pcb::missing_fonts(pcb_of(&self.engine), self.engine.fonts());
        let document = pcb_of(&self.engine).uuid;
        let notes = missing
            .into_iter()
            .map(|font| Note {
                source: FONTS.to_owned(),
                document: Some(document),
                severity: Severity::Warning,
                text: tr!("message-missing-font", font = font),
            })
            .collect();
        self.messages.set_notes(FONTS, notes);
    }
}

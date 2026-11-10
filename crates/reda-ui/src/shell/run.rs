//! Running Commands through the Shell: [`Shell::run`] and its bridged and reporting forms,
//! what follows a run (wiring, element edits, the grid Preferences), undo and redo, and
//! the Journal export.

use super::*;

impl Shell {
    /// Runs a Command an agent sent over the bridge, as [`Shell::run`] does, with its
    /// source (for the Journal and the "MCP: …" undo label) and signals. If it modified
    /// an object the Tool in use depends on, the Tool's preview is cancelled with a note.
    pub fn run_bridged(
        &mut self,
        source: Source,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        if crate::help::USER_ONLY.contains(&id) {
            return Err(CommandError::rejected(format!(
                "only the user can run {id}, from the app"
            )));
        }
        let out = self.engine.run_from(source, id, params, signals)?;
        self.after_run(id);
        if let Some(tool) = &self.tool
            && tool.touched_by(&out)
        {
            self.note = Some(tr!(
                "status-tool-cancelled",
                tool = tool.title.as_str(),
                source = source.label()
            ));
            self.tool = None;
        }
        Ok(out)
    }

    /// Runs a Command, then refreshes the tabs when it may have changed the design or
    /// closed a Document's tab. Opening, creating or saving a Project puts it at the top
    /// of the recent Projects.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let base = id.split(':').next().unwrap_or(id);
        if matches!(base, "project.open" | "project.new" | "project.close") {
            self.save_view();
        }
        let kind = self.engine.registry().get(base).map(|meta| meta.kind);
        let params = self.with_wiring(base, params);
        if base == "selection.cycle"
            && let Some(document) = params
                .get("document")
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
        {
            selection::picking::prepare_cycle(self, document);
        }
        if base == "tool.activate" {
            // The Tool stack needs to know whether a run is in flight.
            let run = self
                .active_sheet()
                .is_some_and(|sheet| crate::schematic_editor::run_in_flight(self, sheet));
            if let Some(stack) = self.engine.view_mut().get_mut::<tools::ToolStack>() {
                stack.run = run;
            }
        }
        // Delete while a Wire vertex's handle is held removes that vertex instead.
        if base == "schematic.delete"
            && let Some(sheet) = self.active_sheet()
            && let Some(commands) = crate::schematic_editor::delete_held_vertex(self, sheet)
        {
            let mut out = json!({});
            for (id, params) in commands {
                out = self.run(&id, params)?;
            }
            return Ok(out);
        }
        if matches!(base, "tool.mirror_x" | "tool.mirror_y") && self.board_mirror_refused() {
            return Err(CommandError::rejected(
                reda_engine::pcb::place::MIRROR_REFUSED,
            ));
        }
        // Tab skips a Footprint while Reposition Selected runs, and widens otherwise.
        if base == "pcb.selection.widen" && self.pcb_tool.queue().is_some() {
            return self.run("pcb.place.skip", json!({}));
        }
        if base == "pcb.copper.delete" && self.pcb_tool.polygon_edit().is_some() {
            // Del while holding a polygon's vertex removes the vertex, not the selection.
            let held = self.with_pcb_tool(|tool, shell| tool.delete_held_vertex(shell));
            if held {
                return Ok(json!({}));
            }
        }
        if base == "pcb.copper.delete"
            && let Some(out) = pcb::delete::intercept(self, &params)
        {
            return out;
        }
        let silkscreen = crate::pcb::silkscreen::COMMANDS
            .contains(&base)
            .then(|| params.clone());
        let shown = self.preferences().editing.highlight.clone();
        self.engine.view_mut().set(shown);
        let out = self.engine.run(id, params.clone());
        if !matches!(kind, Some(Kind::View | Kind::Query)) {
            // A move put down shows until its Command has run, whatever the outcome.
            self.end_committed_moves();
        }
        if let Err(e) = &out {
            self.loop_retry = crate::pcb::board_shapes::retry_for(id, &params, e);
        }
        let out = out?;
        if base == "pcb.route.unroute" {
            pcb::placing::ask_about_locked_unroute(self, &params, &out);
        }
        self.after_edit(base, &out);
        self.after_run(id);
        if matches!(base, "library.element.open" | "library.element.new") {
            crate::element_editor::after_open(self, &out);
        }
        self.after_element_run(base, &out);
        if let Some(params) = silkscreen {
            crate::pcb::silkscreen::after_run(self, base, &params, &out);
        }
        crate::pcb::measure::after_run(self, base, &out);
        self.after_cross_select_run(base);
        if let Some(why) = crate::similar::after_run(self) {
            return Err(CommandError::rejected(why));
        }
        Ok(out)
    }

    /// `params` for Command `id` with its `wiring` filled in from the wiring Preferences
    /// and the sheet's snap grid, when `id` takes one and the caller gave none. Agents'
    /// Commands keep the fixed defaults, so their results don't depend on this machine.
    pub(super) fn with_wiring(&self, id: &str, mut params: Value) -> Value {
        if !reda_engine::schematic::WIRING_COMMANDS.contains(&id) {
            return params;
        }
        let document = (params.get("document"))
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<Uuid>().ok())
            .or_else(|| self.active_document().map(|t| t.document));
        let Some(map) = params.as_object_mut() else {
            return params;
        };
        if map.contains_key("wiring") {
            return params;
        }
        let grid = document.map_or(grid::SCHEMATIC_DEFAULT, |d| self.grid(d));
        let wiring = self.preferences().schematic.wiring(grid);
        map.insert(
            "wiring".to_owned(),
            serde_json::to_value(wiring).expect("wiring serialises"),
        );
        params
    }

    /// What the Library editors do once `id` ran (with result `out`) and the tabs are
    /// refreshed: show the Unit picked, open Edit in Library's Symbol, check a saved
    /// element into Messages, and reopen restored elements in their tabs.
    pub(super) fn after_element_run(&mut self, id: &str, out: &Value) {
        let uuid = |v: &Value| v.as_str().and_then(|s| s.parse::<Uuid>().ok());
        match id {
            "library.symbol.unit.show" => {
                if let Some(symbol) = uuid(&out["symbol"]) {
                    let scene = self.scene_of(symbol);
                    if let Some(tab) = self.document_tab_mut(symbol) {
                        tab.scene = scene;
                    }
                }
            }
            "library.element.edit_in_library" => {
                if let Some(unit) = uuid(&out["object"]) {
                    crate::symbol_editor::edit_in_library(self, unit);
                }
            }
            "library.element.save" => crate::element_editor::check_after_save(self),
            "library.element.copy" => {
                let copy = out["name"].as_str().unwrap_or_default().to_owned();
                self.set_note(tr!("to-library-copied", copy = copy));
            }
            "library.extract_from_project" => {
                let count = |key: &str| out[key].as_array().map_or(0, Vec::len);
                self.set_note(tr!(
                    "to-library-extracted",
                    written = count("written"),
                    skipped = count("skipped")
                ));
            }
            "library.import_part" => {
                if let Some(notice) = out["notice"].as_str() {
                    self.notice = Some(notice.to_owned());
                }
                crate::element_editor::report_created(self, out);
            }
            _ if reda_engine::library::GENERATORS.contains(&id) => {
                crate::element_editor::report_created(self, out);
            }
            "recovery.restore" => {
                let untabbed: Vec<Uuid> = crate::element_editor::open_elements(&self.engine)
                    .into_iter()
                    .filter(|e| self.layout().place(&TabKey::Document(*e)).is_none())
                    .collect();
                for element in untabbed {
                    let _ = self.run("document.open", json!({ "document": element }));
                }
            }
            _ => {}
        }
    }

    /// Refreshes what a successful run of `id` may have changed.
    pub(super) fn after_run(&mut self, id: &str) {
        let base = id.split(':').next().unwrap_or(id);
        // First, so the Scenes the refresh below rebuilds are drawn with them.
        if matches!(
            base,
            "app.preferences.set"
                | "app.preferences.reset"
                | "view.theme.set"
                | "view.ui_scale.set"
                | "view.ui_scale.step"
        ) {
            self.reload_preferences();
        }
        match base {
            "project.open" | "project.new" => {
                self.show_project();
                self.recorder.opened(&self.engine);
                if base == "project.open" {
                    self.run_health_checks();
                }
            }
            "project.close" => {
                self.engine.view_mut().set(Selection::default());
                self.closed_views.clear();
                self.refresh();
                self.layout_mut().open_start();
                // The closed Project's recovery folder goes; open Library elements
                // snapshot into the per-user one from now on.
                self.recorder.opened(&self.engine);
            }
            _ => match self.engine.registry().get(base).map(|meta| meta.kind) {
                Some(Kind::Query) => {}
                Some(Kind::View) => self.sync_document_tabs(),
                Some(Kind::Edit) => {
                    self.refresh();
                    self.auto_refill();
                }
                _ => self.refresh(),
            },
        }
        if base.starts_with("library.")
            || base.starts_with("project.")
            || matches!(base, "app.preferences.set" | "app.preferences.reset")
        {
            self.library_generation += 1;
        }
        if base.starts_with("app.preferences.") {
            self.follow_grid_preferences();
        }
        self.sync_selection();
        self.sync_messages(base);
        self.sync_board_view();
        self.note_focus();
        if matches!(base, "project.open" | "project.new" | "project.save") {
            self.remember_project();
        }
    }

    /// The Undo stack `Mod+Z` uses: a focused Document tab's own, else the Project's.
    /// The Projects panel (and any Panel that is not about one Document) uses the
    /// Project stack. An open dialog names the stack its changes go on (`dialog`): the
    /// Project's for Project Settings, the PCB Document's for Board Setup.
    pub fn undo_scope(&self, dialog: Option<Scope>) -> Option<Scope> {
        if self.engine.project().is_none() {
            // With no Project open, only open Library elements have stacks.
            return match &self.layout().focus {
                Some(TabKey::Document(uuid)) if self.engine.element(*uuid).is_some() => {
                    Some(Scope::Document(*uuid))
                }
                _ => self
                    .active_document()
                    .filter(|t| self.engine.element(t.document).is_some())
                    .map(|t| Scope::Document(t.document)),
            };
        }
        if dialog.is_some() {
            return dialog;
        }
        match &self.layout().focus {
            Some(TabKey::Document(uuid)) => Some(Scope::Document(*uuid)),
            // Board Setup's changes go on the PCB Document's stack.
            Some(TabKey::Settings(SettingsWindow::BoardSetup)) => {
                let pcb = self.engine.project().and_then(|p| p.pcb());
                Some(pcb.map_or(Scope::Project, |pcb| Scope::Document(pcb.uuid)))
            }
            _ => Some(Scope::Project),
        }
    }

    /// Undoes (or with `redo`, redoes) on the stack [`Shell::undo_scope`] picks.
    pub fn undo(&mut self, redo: bool, dialog: Option<Scope>) -> Result<Value, CommandError> {
        let Some(scope) = self.undo_scope(dialog) else {
            return Err(CommandError::unavailable("no Project is open"));
        };
        let document = match scope {
            Scope::Document(uuid) => json!(uuid),
            _ => json!("project"),
        };
        let id = if redo { "edit.redo" } else { "edit.undo" };
        self.run_reporting(id, json!({ "document": document }))
    }

    /// [`Shell::run`], keeping any error as the message the status bar shows.
    pub fn run_reporting(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let out = self.run(id, params);
        match &out {
            Ok(_) => self.message = None,
            Err(e) => self.message = Some(tr!("error-command", error = e.message.clone())),
        }
        out
    }

    /// Saves the session's Journal to `path` as a script that `reda run --script`
    /// replays. View Commands are left out: only the app can run them. A failure is also
    /// kept as the message the status bar shows.
    pub fn export_journal(&mut self, path: &Path) -> std::io::Result<()> {
        let script = crate::journal_panel::script(&self.engine);
        let out = reda_engine::fs::write_atomic(path, script.as_bytes());
        if let Err(e) = &out {
            self.message = Some(tr!("journal-export-failed", error = e.to_string()));
        }
        out
    }

    /// Shows `note` in the status bar: news, not an error.
    pub(crate) fn set_note(&mut self, note: String) {
        self.note = Some(note);
    }

    /// Journals a run detached from the engine (see [`JobRunner`](crate::job_run::JobRunner))
    /// and returns its outcome.
    pub fn finish_detached(&mut self, done: reda_engine::Finished) -> Result<Value, CommandError> {
        self.engine.finish(done)
    }
}

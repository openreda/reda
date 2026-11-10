//! The Shell's tabs: the Document tabs and settings windows, focus and the Ctrl+Tab
//! switcher, keeping the tabs in step with the Project, and each canvas's view history
//! (re-centre, previous view, jumps).

use super::*;

impl Shell {
    pub(super) fn focus_state(&mut self) -> &mut layout::focus::Focus {
        self.engine
            .view_mut()
            .get_mut::<layout::focus::Focus>()
            .expect("the Shell installs its Focus")
    }

    /// Remembers the focused tab as the most recently used.
    pub(super) fn note_focus(&mut self) {
        if let Some(tab) = self.layout().focus.clone() {
            self.focus_state().used(&tab);
        }
    }

    /// The tab switcher (Ctrl+Tab), while it is open.
    pub fn tab_switch(&self) -> Option<&layout::focus::TabSwitch> {
        self.engine
            .view()
            .get::<layout::focus::Focus>()
            .and_then(|f| f.switching.as_ref())
    }

    /// Picks the switcher's tab at `index`.
    pub fn pick_tab_switch(&mut self, index: usize) {
        if let Some(switch) = &mut self.focus_state().switching
            && index < switch.tabs.len()
        {
            switch.index = index;
        }
    }

    /// Closes the tab switcher; with `commit`, shows and focuses the tab it picked
    /// (through `layout.tab.activate`).
    pub fn end_tab_switch(&mut self, commit: bool) {
        let Some(switch) = self.focus_state().switching.take() else {
            return;
        };
        if commit && let Some(tab) = switch.picked() {
            let tab = tab.clone();
            let params = json!({"tab": tab.to_string()});
            if self.run_reporting("layout.tab.activate", params).is_ok() {
                self.focus_state().request = Some(tab);
            }
        }
    }

    /// Takes the tab whose strip button should take the keyboard focus, after a Command
    /// moved focus to it from the keyboard.
    pub fn take_focus_request(&mut self) -> Option<TabKey> {
        self.focus_state().request.take()
    }

    /// Centres `document`'s canvas on where the pointer last was over it (`view.recenter`,
    /// Home), keeping the zoom; nothing while the pointer has not been over it.
    pub fn recenter(&mut self, document: Uuid) {
        let Some(tab) = self.document_tab_mut(document) else {
            return;
        };
        if let Some(at) = tab.cursor {
            self.jump(
                document,
                crate::navigation::Jump::To(Point::new(Nm(at.0 as i64), Nm(at.1 as i64))),
            );
        }
    }

    /// Puts `document`'s canvas back to its previous view (`view.previous`, Alt+←);
    /// false when it has none.
    pub fn previous_view(&mut self, document: Uuid) -> bool {
        self.document_tab_mut(document)
            .is_some_and(|tab| tab.previous_view())
    }

    /// Moves `document`'s canvas as `view.jump_to` asked, keeping the zoom.
    pub fn jump(&mut self, document: Uuid, jump: crate::navigation::Jump) {
        use crate::navigation::Jump;
        let Some(tab) = self.document_tab_mut(document) else {
            return;
        };
        tab.remember_view();
        let pending = tab.center_on.take();
        let center = match (tab.camera.as_ref(), pending) {
            (_, Some(p)) => (p.x.0 as f64, p.y.0 as f64),
            (Some(camera), None) => camera.center,
            (None, None) => {
                // No camera yet: the fitted canvas centres on the Document's bounds.
                let zero = Point::new(Nm(0), Nm(0));
                let (lo, hi) = tab.scene.scene.bounds().unwrap_or((zero, zero));
                (
                    (lo.x.0 + hi.x.0) as f64 / 2.0,
                    (lo.y.0 + hi.y.0) as f64 / 2.0,
                )
            }
        };
        let target = match jump {
            Jump::To(p) => (p.x.0 as f64, p.y.0 as f64),
            Jump::By(d) => (center.0 + d.x.0 as f64, center.1 + d.y.0 as f64),
        };
        match tab.camera.as_mut() {
            Some(camera) => camera.center = target,
            None => {
                tab.center_on = Some(Point::new(Nm(target.0 as i64), Nm(target.1 as i64)));
            }
        }
    }

    /// The open Documents' tabs.
    pub fn document_tabs(&self) -> &[DocumentTab] {
        &self.documents
    }

    /// The tab of the open Document `document`.
    pub fn document_tab(&self, document: Uuid) -> Option<&DocumentTab> {
        self.documents.iter().find(|d| d.document == document)
    }

    /// The tab of an open Document, to update its camera.
    pub fn document_tab_mut(&mut self, document: Uuid) -> Option<&mut DocumentTab> {
        self.documents.iter_mut().find(|d| d.document == document)
    }

    /// The active Document's tab: the Document last interacted with.
    pub fn active_document(&self) -> Option<&DocumentTab> {
        self.document_tab(self.layout().active_document()?)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut DocumentTab> {
        let document = self.layout().active_document()?;
        self.document_tab_mut(document)
    }

    /// Shows the settings window `window`: raises it where it is, or opens it in a window
    /// of its own at `rect`. Returns whether it opened just now.
    pub fn open_settings(&mut self, window: SettingsWindow, rect: Option<WindowRect>) -> bool {
        self.layout_mut().open_settings(window, rect)
    }

    /// Closes the settings window `window`'s tab, as its own buttons do: a view change
    /// with nothing to journal.
    pub fn close_settings(&mut self, window: SettingsWindow) {
        let _ = self.layout_mut().close(&TabKey::Settings(window));
    }

    /// Names the settings window `window`'s tab.
    pub fn set_settings_title(&mut self, window: SettingsWindow, title: String) {
        self.settings_titles.insert(window, title);
    }

    /// The title of the settings window `window`'s tab, once the window named it.
    pub fn settings_title(&self, window: SettingsWindow) -> Option<&str> {
        self.settings_titles.get(&window).map(String::as_str)
    }

    /// The title of `tab`'s Document, if it is an open Document.
    pub fn document_title(&self, tab: &TabKey) -> Option<&str> {
        match tab {
            TabKey::Document(uuid) => self.document_tab(*uuid).map(|d| d.title.as_str()),
            _ => None,
        }
    }

    /// Matches the Document tabs to the layout after a view Command: drops the state of
    /// closed tabs and starts it for tabs `document.open` added.
    pub(super) fn sync_document_tabs(&mut self) {
        self.forget_closed_documents();
        let opened: Vec<Uuid> = self
            .layout()
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) if self.document_tab(*uuid).is_none() => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in opened {
            let title = self
                .engine
                .project()
                .and_then(|p| p.document(document))
                .map(|d| d.title.clone())
                .or_else(|| crate::element_editor::title(&self.engine, document))
                .unwrap_or_default();
            let scene = self.scene_of(document);
            let mut tab = DocumentTab::new(document, title, scene);
            // A Document opened again comes back as it was last seen.
            if let Some(view) = self.closed_views.remove(&document) {
                tab.camera = view.camera;
                tab.layers = view.layers;
            }
            self.documents.push(tab);
        }
        self.sync_selection();
    }

    /// Drops the tab state of Documents whose tabs the layout no longer has.
    /// Their view is kept for `view.json` and for when they open again.
    pub(super) fn forget_closed_documents(&mut self) {
        let documents = std::mem::take(&mut self.documents);
        let (open, closed): (Vec<_>, Vec<_>) = documents
            .into_iter()
            .partition(|d| self.layout().place(&TabKey::Document(d.document)).is_some());
        for tab in closed {
            let view = DocumentView {
                camera: tab.camera,
                layers: tab.layers,
                ..DocumentView::default()
            };
            self.closed_views.insert(tab.document, view);
        }
        self.documents = open;
    }
}

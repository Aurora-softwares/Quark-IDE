//! User-facing behaviour regressions. Each harness owns its persistence directory.
use super::*;
use eframe::App;

struct Harness {
    q: Quark,
    ctx: egui::Context,
    dir: tempfile::TempDir,
    tick: u64,
}
impl Harness {
    fn new() -> Self {
        // macOS may report /var via a symlink to /private/var; normalise
        // the test sandbox to the same path used by Quark's canonicalize().
        let root = std::env::temp_dir();
        let canonical_root = root.canonicalize().unwrap_or(root);
        let dir = tempfile::tempdir_in(canonical_root).unwrap();
        let mut q = Quark::from_state(Settings::default(), Session::default(), None, None);
        q.workspace_requested = false;
        q.settings.automatic_completion = false;
        q.settings_path = Some(dir.path().join("settings.json"));
        q.session_path = Some(dir.path().join("session.json"));
        Self {
            q,
            ctx: egui::Context::default(),
            dir,
            tick: 0,
        }
    }
    fn draw(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.tick += 1;
        let input = egui::RawInput {
            time: Some(self.tick as f64 * 0.05),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 1400.),
            )),
            events,
            ..Default::default()
        };
        let mut frame = eframe::Frame::_new_kittest();
        self.ctx.run(input, |ctx| self.q.update(ctx, &mut frame))
    }
    fn key(&mut self, key: Key, modifiers: Modifiers) -> egui::FullOutput {
        self.draw(vec![]);
        let out = self.draw(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.draw(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }]);
        out
    }
    fn click(&mut self, label: &str) -> egui::FullOutput {
        let mut pos = None;
        for attempt in 0..20 {
            let events = if attempt > 2 && pos.is_none() && self.q.show_settings {
                let p = self
                    .ctx
                    .memory(|m| m.area_rect(egui::Id::new("Settings")))
                    .unwrap()
                    .center();
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -150.),
                        modifiers: Modifiers::NONE,
                    },
                ]
            } else {
                vec![]
            };
            let out = self.draw(events);
            for shape in out.shapes {
                if let egui::epaint::Shape::Text(t) = shape.shape {
                    if t.galley.text() == label {
                        pos = Some(t.pos + t.galley.size() * 0.5);
                    }
                }
            }
            if pos.is_some() && attempt >= 2 {
                break;
            }
        }
        self.click_at(pos.unwrap_or_else(|| panic!("Missing control: {label}")))
    }
    fn click_at(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.draw(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.draw(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ])
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.dir.path().join(name);
        fs::write(&p, text).unwrap();
        p
    }
    fn editor(&mut self, text: &str) {
        self.q.new_document();
        self.q.docs.last_mut().unwrap().text = text.into();
        self.draw(vec![]);
        self.draw(vec![]);
    }
    fn stored_settings(&self) -> Settings {
        serde_json::from_slice(&fs::read(self.q.settings_path.as_ref().unwrap()).unwrap()).unwrap()
    }
    fn busy(&mut self) -> mpsc::SyncSender<command::Event> {
        let (tx, rx) = mpsc::sync_channel(8);
        self.q.job_rx = Some(rx);
        self.q.job_cancel = Some(Arc::new(AtomicBool::new(false)));
        tx
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(c) = &self.q.job_cancel {
            c.store(true, Ordering::Relaxed);
        }
        self.q.job_rx = None;
        if let Some(w) = self.q.job_worker.take() {
            w.join().unwrap();
        }
        self.q.terminal.take();
    }
}
fn has_text(output: &egui::FullOutput, text: &str) -> bool {
    output
        .shapes
        .iter()
        .any(|s| matches!(&s.shape,egui::epaint::Shape::Text(t) if t.galley.text().contains(text)))
}
fn has_command(output: &egui::FullOutput, command: egui::ViewportCommand) -> bool {
    output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&command)
}

#[test]
fn new_document_is_empty_and_has_a_unique_identity() {
    let mut h = Harness::new();
    h.q.new_document();
    h.q.new_document();
    assert_ne!(h.q.docs[0].id, h.q.docs[1].id);
    assert_eq!(h.q.active, Some(1));
    assert!(h
        .q
        .docs
        .iter()
        .all(|d| d.path.is_none() && d.text.is_empty() && d.saved.is_empty()));
}
#[test]
fn new_document_action_keeps_existing_unsaved_text() {
    let mut h = Harness::new();
    h.editor("keep me");
    h.q.action(0, &h.ctx);
    assert_eq!(h.q.docs[0].text, "keep me");
    assert_eq!(h.q.docs.len(), 2);
    assert_eq!(h.q.active, Some(1));
}
#[test]
fn opening_unicode_file_reuses_its_existing_tab() {
    let mut h = Harness::new();
    let p = h.file("π.hy", "π✓\n");
    h.q.open(p.clone());
    h.q.open(p);
    assert_eq!(h.q.docs.len(), 1);
    assert_eq!(h.q.docs[0].text, "π✓\n");
    assert_eq!(h.q.docs[0].saved, h.q.docs[0].text);
}
#[test]
fn missing_or_binary_file_keeps_existing_editor() {
    let mut h = Harness::new();
    h.editor("keep");
    let p = h.dir.path().join("binary");
    fs::write(&p, [0xff, 0xfe]).unwrap();
    for p in [p, h.dir.path().join("missing")] {
        h.q.open(p);
        assert_eq!(h.q.docs.len(), 1);
        assert_eq!(h.q.docs[0].text, "keep");
        assert!(h.q.status.starts_with("Open failed:"));
    }
}
#[test]
fn save_updates_disk_baseline_and_recovery() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "new π".into();
    assert!(h.q.save(0));
    assert_eq!(fs::read_to_string(p).unwrap(), "new π");
    assert_eq!(h.q.docs[0].saved, "new π");
    let s: Session = load_json(h.q.session_path.as_deref()).0;
    assert_eq!(s.docs[0].text, "new π");
}
#[test]
fn save_failure_preserves_dirty_buffer_and_original() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "mine".into();
    fs::remove_file(&p).unwrap();
    assert!(!h.q.save(0));
    assert_eq!(h.q.pending_conflict, Some(0));
    assert_eq!(h.q.docs[0].saved, "old");
    assert_eq!(h.q.docs[0].text, "mine");
    assert!(!p.exists());
}
#[test]
fn save_as_writes_new_path_and_preserves_original() {
    let mut h = Harness::new();
    let original = h.file("a.hy", "original");
    h.q.open(original.clone());
    h.q.docs[0].text = "copy".into();
    let copy = h.dir.path().join("copy.hy");
    assert!(h.q.save_document_to(0, copy.clone(), false, false));
    assert_eq!(fs::read_to_string(original).unwrap(), "original");
    assert_eq!(fs::read_to_string(&copy).unwrap(), "copy");
    assert_eq!(h.q.docs[0].path, Some(copy));
}
#[test]
fn save_as_rejects_another_open_document_destination() {
    let mut h = Harness::new();
    let a = h.file("a.hy", "a");
    let b = h.file("b.hy", "b");
    h.q.open(a.clone());
    h.q.open(b.clone());
    h.q.docs[0].text = "edited".into();
    assert!(!h.q.save_document_to(0, b.clone(), false, false));
    assert_eq!(fs::read_to_string(b).unwrap(), "b");
    assert_eq!(h.q.docs[0].path, Some(a));
    assert_eq!(h.q.docs[0].text, "edited");
    assert!(h.q.status.contains("already open"));
}
#[test]
fn save_all_writes_each_dirty_file() {
    let mut h = Harness::new();
    for name in ["a.hy", "b.hy"] {
        let p = h.file(name, "old");
        h.q.open(p);
    }
    for d in &mut h.q.docs {
        d.text = "new".into();
    }
    assert!(h.q.save_all());
    assert!(h.q.docs.iter().all(
        |d| d.saved == "new" && fs::read_to_string(d.path.as_ref().unwrap()).unwrap() == "new"
    ));
}
#[test]
fn save_all_stops_on_conflict_without_overwriting_later_files() {
    let mut h = Harness::new();
    let a = h.file("a.hy", "a");
    let b = h.file("b.hy", "b");
    h.q.open(a.clone());
    h.q.open(b.clone());
    for d in &mut h.q.docs {
        d.text = "mine".into();
    }
    fs::write(&a, "external").unwrap();
    assert!(!h.q.save_all());
    assert_eq!(fs::read_to_string(a).unwrap(), "external");
    assert_eq!(fs::read_to_string(b).unwrap(), "b");
    assert!(h.q.docs.iter().all(|d| d.text == "mine"));
}
#[test]
fn clean_tab_close_selects_remaining_neighbour() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.q.new_document();
    }
    h.q.active = Some(1);
    let last = h.q.docs[2].id;
    h.q.close(1);
    assert_eq!(h.q.docs.len(), 2);
    assert_eq!(h.q.docs[h.q.active.unwrap()].id, last);
    h.q.close(1);
    h.q.close(0);
    assert_eq!(h.q.active, None);
}
#[test]
fn dirty_tab_save_button_writes_before_closing() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "new".into();
    h.q.close(0);
    h.click("Save");
    assert!(h.q.docs.is_empty());
    assert_eq!(fs::read_to_string(p).unwrap(), "new");
}
#[test]
fn conflict_overwrite_is_explicit_and_updates_baseline() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "mine".into();
    fs::write(&p, "external").unwrap();
    assert!(!h.q.save(0));
    h.click("Overwrite disk with my edits");
    assert_eq!(fs::read_to_string(p).unwrap(), "mine");
    assert_eq!(h.q.docs[0].saved, "mine");
    assert_eq!(h.q.pending_conflict, None);
}
#[test]
fn conflict_cancel_preserves_both_versions() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "mine".into();
    fs::write(&p, "external").unwrap();
    h.q.save(0);
    h.click("Cancel");
    assert_eq!(h.q.docs[0].text, "mine");
    assert_eq!(h.q.docs[0].saved, "old");
    assert_eq!(fs::read_to_string(p).unwrap(), "external");
    assert_eq!(h.q.pending_conflict, None);
}
#[test]
fn conflict_reload_failure_keeps_unsaved_edits() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "mine".into();
    fs::remove_file(p).unwrap();
    h.q.save(0);
    h.click("Reload disk · discard my edits");
    assert_eq!(h.q.docs[0].text, "mine");
    assert_eq!(h.q.pending_conflict, Some(0));
    assert!(h.q.status.contains("Could not reload"));
}
#[test]
fn clean_exit_emits_close_without_confirmation() {
    let mut h = Harness::new();
    let out = h
        .ctx
        .run(egui::RawInput::default(), |ctx| h.q.request_exit(ctx));
    assert!(h.q.allow_exit);
    assert!(!h.q.exit_requested);
    assert!(has_command(&out, egui::ViewportCommand::Close));
}
#[test]
fn busy_exit_can_be_cancelled_without_stopping_job() {
    let mut h = Harness::new();
    let _tx = h.busy();
    h.q.request_exit(&h.ctx);
    assert!(h.q.exit_requested);
    h.click("Cancel");
    assert!(!h.q.exit_requested);
    assert!(h.q.job_rx.is_some());
    assert!(!h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
}
#[test]
fn exit_save_all_writes_dirty_files_then_requests_close() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "saved on exit".into();
    h.q.request_exit(&h.ctx);
    let out = h.click("Save all & close");
    assert!(has_command(&out, egui::ViewportCommand::Close));
    assert!(h.q.allow_exit);
    assert_eq!(fs::read_to_string(p).unwrap(), "saved on exit");
}
#[test]
fn exit_discard_removes_untitled_recovery_and_restores_saved_files() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "dirty".into();
    h.editor("untitled");
    h.q.request_exit(&h.ctx);
    h.click("Discard changes & close");
    let session: Session = load_json(h.q.session_path.as_deref()).0;
    assert_eq!(session.docs.len(), 1);
    assert_eq!(session.docs[0].text, "old");
    assert_eq!(fs::read_to_string(p).unwrap(), "old");
}
#[test]
fn settings_persist_all_preferences_to_isolated_file() {
    let mut h = Harness::new();
    h.q.settings.word_wrap = true;
    h.q.settings.automatic_completion = false;
    h.q.settings.integrated_title_bar = false;
    h.q.settings.index_limit = 1234;
    h.q.settings.command_timeout_seconds = 0;
    h.q.settings.command_folder = Some(h.dir.path().to_path_buf());
    h.q.persist();
    let s = h.stored_settings();
    assert!(s.word_wrap && !s.automatic_completion && !s.integrated_title_bar);
    assert_eq!(s.index_limit, 1234);
    assert_eq!(s.command_timeout_seconds, 0);
    assert_eq!(s.command_folder, Some(h.dir.path().to_path_buf()));
}
#[test]
fn persistence_write_failure_is_visible_and_keeps_settings() {
    let mut h = Harness::new();
    let p = h.file("blocker", "original");
    h.q.settings_path = Some(p.join("settings.json"));
    h.q.settings.word_wrap = true;
    h.q.persist();
    assert!(h.q.status.contains("Settings not saved"));
    assert!(h.q.settings.word_wrap);
    assert_eq!(fs::read_to_string(p).unwrap(), "original");
}
#[test]
fn recovery_snapshot_restores_untitled_text_and_active_tab() {
    let mut h = Harness::new();
    h.editor("first");
    h.editor("second π");
    h.q.save_session();
    let s: Session = load_json(h.q.session_path.as_deref()).0;
    let restored = Quark::from_state(Settings::default(), s, None, None);
    assert_eq!(restored.active, Some(1));
    assert_eq!(restored.docs[1].text, "second π");
    assert!(restored
        .notices
        .iter()
        .any(|n| n.contains("Restored unsaved")));
}
#[test]
fn clean_restored_document_refreshes_from_disk_but_dirty_one_does_not() {
    let mut h = Harness::new();
    let a = h.file("a.hy", "a");
    let b = h.file("b.hy", "b");
    h.q.open(a.clone());
    h.q.open(b.clone());
    h.q.docs[1].text = "my edits".into();
    h.q.save_session();
    fs::write(a, "disk a").unwrap();
    fs::write(b, "disk b").unwrap();
    let restored = Quark::from_state(
        Settings::default(),
        load_json(h.q.session_path.as_deref()).0,
        None,
        None,
    );
    assert_eq!(restored.docs[0].text, "disk a");
    assert_eq!(restored.docs[1].text, "my edits");
    assert_eq!(restored.docs[1].saved, "b");
}
#[test]
fn corrupt_recovery_is_preserved_and_warned() {
    let h = Harness::new();
    let p = h.file("broken.json", "{invalid");
    let (s, w) = load_json::<Session>(Some(&p));
    assert!(s.docs.is_empty());
    assert!(w.unwrap().contains("Original preserved"));
    assert_eq!(fs::read_to_string(p).unwrap(), "{invalid");
}
#[test]
fn reset_persistence_backs_up_unreadable_files_before_replacing() {
    let mut h = Harness::new();
    fs::write(h.q.settings_path.as_ref().unwrap(), "bad settings").unwrap();
    fs::write(h.q.session_path.as_ref().unwrap(), "bad recovery").unwrap();
    h.q.settings_blocked = true;
    h.q.session_blocked = true;
    h.q.reset_persistence();
    assert!(!h.q.settings_blocked && !h.q.session_blocked);
    let backups: Vec<_> = fs::read_dir(h.dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("backup-"))
        .map(|p| fs::read_to_string(p).unwrap())
        .collect();
    assert!(backups.contains(&"bad settings".into()) && backups.contains(&"bad recovery".into()));
    assert!(load_json::<Settings>(h.q.settings_path.as_deref())
        .1
        .is_none());
}
#[test]
fn failed_backup_keeps_persistence_paused() {
    let mut h = Harness::new();
    h.q.settings_path = Some(h.dir.path().join("missing.json"));
    h.q.settings_blocked = true;
    h.q.session_blocked = true;
    h.q.reset_persistence();
    assert!(h.q.settings_blocked && h.q.session_blocked);
    assert!(h.q.status.contains("Could not back up"));
    assert!(!h.q.settings_path.as_ref().unwrap().exists());
}
#[test]
fn recovery_write_failure_is_visible_and_keeps_buffer() {
    let mut h = Harness::new();
    let p = h.file("blocker", "old");
    h.q.session_path = Some(p.join("session.json"));
    h.q.new_document();
    h.q.docs[0].text = "safe in memory".into();
    h.q.save_session();
    assert_eq!(h.q.docs[0].text, "safe in memory");
    assert!(h.q.status.contains("Recovery snapshot not saved"));
}

#[test]
fn adding_folder_is_canonical_and_deduplicated() {
    let mut h = Harness::new();
    let p = h.dir.path().to_path_buf();
    h.q.add_folder(p.clone());
    h.q.add_folder(p.join("."));
    assert_eq!(h.q.settings.folders, vec![p.canonicalize().unwrap()]);
    assert_eq!(h.stored_settings().folders, h.q.settings.folders);
}
#[test]
fn adding_project_selects_it_and_opens_its_manifest_once() {
    let mut h = Harness::new();
    let p = h.file("app.hyproj", "name='App'\ntype='exe'\n");
    h.q.add_project(p.clone());
    h.q.add_project(p.clone());
    assert_eq!(h.q.settings.projects, vec![p.canonicalize().unwrap()]);
    assert_eq!(h.q.docs.len(), 1);
    assert_eq!(h.q.selected_project(), Some(p.canonicalize().unwrap()));
    assert_eq!(h.q.settings.folders.len(), 1);
}
#[test]
fn manifest_save_as_does_not_register_a_project() {
    let mut h = Harness::new();
    h.editor("id='new'\n");
    let p = h.dir.path().join("new.hyproj");
    assert!(h.q.save_document_to(0, p.clone(), false, false));
    assert!(h.q.settings.projects.is_empty());
    assert_eq!(h.q.settings.selected_project, None);
    assert_eq!(fs::read_to_string(p).unwrap(), "id='new'\n");
}
#[test]
fn removing_project_keeps_files_and_resets_selected_target() {
    let mut h = Harness::new();
    let p = h.file("app.hyproj", "id='app'");
    h.q.add_project(p.clone());
    h.q.show_settings = true;
    h.click("Remove project");
    assert!(p.exists());
    assert!(h.q.settings.projects.is_empty());
    assert!(h.q.settings.selected_project.is_none());
}
#[test]
fn removing_folder_clears_directory_override_and_preserves_files() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "keep");
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.settings.command_folder = Some(h.dir.path().to_path_buf());
    h.q.show_settings = true;
    h.click("Remove");
    assert!(h.q.settings.folders.is_empty());
    assert!(h.q.settings.command_folder.is_none());
    assert_eq!(fs::read_to_string(p).unwrap(), "keep");
}
#[test]
fn selected_project_falls_back_when_saved_selection_was_removed() {
    let mut h = Harness::new();
    let a = h.file("a.hyproj", "id='a'");
    let b = h.file("b.hyproj", "id='b'");
    h.q.settings.projects = vec![a.clone()];
    h.q.settings.selected_project = Some(b);
    assert_eq!(h.q.selected_project(), Some(a));
    h.q.settings.projects.clear();
    assert_eq!(h.q.selected_project(), None);
}
#[test]
fn directory_override_takes_priority_over_project_and_workspace() {
    let mut h = Harness::new();
    let p = h.file("a.hyproj", "id='a'");
    h.q.add_project(p);
    let alternate = h.dir.path().join("chosen");
    h.q.settings.command_folder = Some(alternate.clone());
    assert_eq!(h.q.cwd(), alternate);
    h.q.settings.command_folder = None;
    assert_eq!(h.q.cwd(), h.dir.path().canonicalize().unwrap());
}
#[test]
fn directory_follows_first_folder_when_no_project_is_selected() {
    let mut h = Harness::new();
    let first = h.dir.path().join("one");
    let second = h.dir.path().join("two");
    h.q.settings.folders = vec![first.clone(), second];
    h.editor("outside");
    assert_eq!(h.q.cwd(), first);
}
#[test]
fn refresh_action_invalidates_workspace_and_diagnostics() {
    let mut h = Harness::new();
    h.q.workspace_requested = false;
    h.q.diagnostics_requested = false;
    let revision = h.q.diagnostics_revision;
    h.q.action(18, &h.ctx);
    assert!(h.q.workspace_requested && h.q.diagnostics_requested);
    assert!(h.q.diagnostics_revision > revision);
}
#[test]
fn disconnected_workspace_worker_reports_retry_without_losing_snapshot() {
    let mut h = Harness::new();
    h.q.workspace_requested = false;
    h.q.workspace.files = vec![PathBuf::from("keep.hy")];
    let (tx, rx) = mpsc::channel();
    drop(tx);
    h.q.workspace_rx = Some(rx);
    h.q.workspace_tick();
    assert!(h.q.workspace_rx.is_none());
    assert_eq!(h.q.workspace.files, vec![PathBuf::from("keep.hy")]);
    assert!(h.q.status.contains("Refresh to retry"));
}

macro_rules! sidebar_cases {
    ($open:ident,$render:ident,$view:ident,$title:literal) => {
        #[test]
        fn $open() {
            let mut h = Harness::new();
            h.q.settings.sidebar_visible = false;
            h.q.show_sidebar(SidebarView::$view);
            assert!(h.q.settings.sidebar_visible);
            assert_eq!(h.q.settings.sidebar_view, SidebarView::$view);
            assert_eq!(h.stored_settings().sidebar_view, SidebarView::$view);
        }
        #[test]
        fn $render() {
            let mut h = Harness::new();
            h.q.show_sidebar(SidebarView::$view);
            let out = h.draw(vec![]);
            assert!(has_text(&out, $title));
            assert!(h.q.sidebar_rect.width() >= 240.);
            assert!(!h.q.show_settings);
        }
    };
}
sidebar_cases!(
    explorer_opens_and_persists,
    explorer_renders_controls,
    Explorer,
    "Open a folder"
);
sidebar_cases!(
    search_opens_and_persists,
    search_renders_controls,
    Search,
    "matches in active file"
);
sidebar_cases!(
    source_control_opens_and_persists,
    source_control_renders_controls,
    SourceControl,
    "Git uses your OS credentials"
);
sidebar_cases!(
    plugins_opens_and_persists,
    plugins_renders_controls,
    Plugins,
    "No extensions installed."
);
sidebar_cases!(
    themes_opens_and_persists,
    themes_renders_controls,
    Themes,
    "Colour themes"
);
#[test]
fn sidebar_toggle_button_hides_and_restores_without_losing_view() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.draw(vec![]);
    h.draw(vec![]);
    h.click_at(egui::pos2(1126., 14.));
    assert!(!h.q.settings.sidebar_visible);
    h.click_at(egui::pos2(1126., 14.));
    assert!(h.q.settings.sidebar_visible);
    assert_eq!(h.q.settings.sidebar_view, SidebarView::Themes);
}
#[test]
fn sidebar_shortcut_and_palette_toggle_keep_documents() {
    let mut h = Harness::new();
    h.editor("keep");
    h.key(Key::B, Modifiers::COMMAND);
    assert!(!h.q.settings.sidebar_visible);
    h.q.action(13, &h.ctx);
    assert!(h.q.settings.sidebar_visible);
    assert_eq!(h.q.docs[0].text, "keep");
}
#[test]
fn bottom_toggle_button_changes_visibility_without_starting_output_shell() {
    let mut h = Harness::new();
    h.q.settings.bottom_tab = BottomTab::Output;
    h.draw(vec![]);
    h.draw(vec![]);
    h.click_at(egui::pos2(1159., 14.));
    assert!(h.q.settings.terminal_visible);
    assert!(h.q.terminal.is_none());
    h.click_at(egui::pos2(1159., 14.));
    assert!(!h.q.settings.terminal_visible);
}
#[test]
fn bottom_panel_shortcut_and_palette_action_are_reversible() {
    let mut h = Harness::new();
    h.q.settings.bottom_tab = BottomTab::Output;
    h.key(Key::Backtick, Modifiers::CTRL);
    assert!(h.q.settings.terminal_visible);
    h.q.action(14, &h.ctx);
    assert!(!h.q.settings.terminal_visible);
}
#[test]
fn welcome_action_preserves_all_open_documents() {
    let mut h = Harness::new();
    h.editor("unsaved");
    h.q.action(15, &h.ctx);
    let out = h.draw(vec![]);
    assert!(h.q.active.is_none());
    assert_eq!(h.q.docs[0].text, "unsaved");
    assert!(has_text(&out, "Coming from VS Code?"));
}
#[test]
fn welcome_new_file_and_configure_buttons_reach_their_actions() {
    let mut h = Harness::new();
    h.click("Configure compiler…");
    assert!(h.q.show_settings);
    h.q.show_settings = false;
    h.click("New File…");
    assert_eq!(h.q.docs.len(), 1);
    assert_eq!(h.q.active, Some(0));
}
#[test]
fn settings_action_opens_and_save_button_persists_then_closes() {
    let mut h = Harness::new();
    h.q.action(16, &h.ctx);
    assert!(h.q.show_settings);
    h.q.settings.word_wrap = true;
    h.click("Save settings");
    assert!(!h.q.show_settings);
    assert!(h.stored_settings().word_wrap);
}
#[test]
fn blocked_settings_save_does_not_hide_recovery_warning() {
    let mut h = Harness::new();
    h.q.settings_blocked = true;
    h.q.show_settings = true;
    h.click("Save settings");
    assert!(h.q.show_settings);
    assert!(h.q.settings_blocked);
    assert!(!h.q.settings_path.as_ref().unwrap().exists());
}
#[test]
fn word_wrap_shortcut_preserves_text_and_persists_choice() {
    let mut h = Harness::new();
    h.editor(&"long ".repeat(200));
    let before = h.q.docs[0].text.clone();
    h.key(Key::Z, Modifiers::ALT);
    assert!(h.q.settings.word_wrap);
    assert_eq!(h.q.docs[0].text, before);
    assert!(h.stored_settings().word_wrap);
}
#[test]
fn word_wrap_can_be_toggled_off_after_restoring_preferences() {
    let mut h = Harness::new();
    h.q.settings.word_wrap = true;
    h.key(Key::Z, Modifiers::ALT);
    assert!(!h.q.settings.word_wrap);
    assert!(!h.stored_settings().word_wrap);
}
#[test]
fn quick_open_filters_paths_and_enter_opens_the_selected_file() {
    let mut h = Harness::new();
    let a = h.file("a.hy", "a");
    let b = h.file("b.hy", "b");
    h.q.workspace.files = vec![a, b.clone()];
    h.q.workspace_revision = 5;
    h.q.quick_query = "B.HY".into();
    h.q.action(8, &h.ctx);
    h.draw(vec![]);
    h.key(Key::Enter, Modifiers::NONE);
    assert!(!h.q.show_quick_open);
    assert_eq!(h.q.current_path(), Some(b.canonicalize().unwrap()));
}
#[test]
fn quick_open_empty_results_and_escape_leave_document_unchanged() {
    let mut h = Harness::new();
    h.editor("keep");
    h.q.quick_query = "missing".into();
    h.q.action(8, &h.ctx);
    h.draw(vec![]);
    assert!(has_text(&h.draw(vec![]), "0 matches"));
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.q.docs.len(), 1);
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.q.show_quick_open);
    assert_eq!(h.q.docs[0].text, "keep");
}
#[test]
fn quick_open_arrow_selection_wraps_both_directions() {
    let mut h = Harness::new();
    h.q.workspace.files = vec![h.file("a.hy", "a"), h.file("b.hy", "b")];
    h.q.workspace_revision = 1;
    h.q.action(8, &h.ctx);
    h.draw(vec![]);
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(h.q.quick_selected, 1);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.q.quick_selected, 0);
}
#[test]
fn command_palette_filters_case_insensitively_and_runs_matching_action() {
    let mut h = Harness::new();
    h.q.show_palette = true;
    h.q.palette_query = "> PREFERENCES: THEMES".into();
    h.draw(vec![]);
    h.key(Key::Enter, Modifiers::NONE);
    assert!(!h.q.show_palette);
    assert_eq!(h.q.settings.sidebar_view, SidebarView::Themes);
}
#[test]
fn command_palette_no_matches_and_escape_do_not_run_actions() {
    let mut h = Harness::new();
    h.editor("keep");
    h.q.show_palette = true;
    h.q.palette_query = "no-such-command-123".into();
    h.draw(vec![]);
    assert!(has_text(&h.draw(vec![]), "No matching commands"));
    h.key(Key::Enter, Modifiers::NONE);
    assert!(h.q.show_palette);
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.q.show_palette);
    assert_eq!(h.q.docs[0].text, "keep");
}
#[test]
fn command_palette_arrows_wrap_and_query_change_resets_selection() {
    let mut h = Harness::new();
    h.q.show_palette = true;
    h.q.palette_focus = true;
    h.draw(vec![]);
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert!(h.q.palette_selected > 0);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.q.palette_selected, 0);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.q.palette_selected, 1);
    h.draw(vec![egui::Event::Text("Themes".into())]);
    assert_eq!(h.q.palette_selected, 0);
}
#[test]
fn go_to_line_positions_unicode_caret_without_changing_text() {
    let mut h = Harness::new();
    h.editor("π\nsecond\nthird");
    h.q.go_line_input = " 2 ".into();
    h.q.show_go_to_line = true;
    h.q.go_to_line(&h.ctx);
    assert_eq!(h.q.caret(&h.ctx), 2);
    assert!(!h.q.show_go_to_line);
    assert_eq!(h.q.docs[0].text, "π\nsecond\nthird");
}
#[test]
fn go_to_line_rejects_invalid_and_outside_lines() {
    let mut h = Harness::new();
    h.editor("a\nb");
    for s in ["bad", "0", "99"] {
        h.q.go_line_input = s.into();
        h.q.show_go_to_line = true;
        h.q.go_to_line(&h.ctx);
        assert!(h.q.show_go_to_line);
        assert_eq!(h.q.docs[0].text, "a\nb");
    }
    assert_eq!(h.q.status, "Line outside file");
}
#[test]
fn find_next_and_previous_wrap_unicode_matches_without_editing() {
    let mut h = Harness::new();
    h.editor("π a π");
    h.q.search = "π".into();
    h.q.set_selection(&h.ctx, 0, 0);
    h.q.next_match(&h.ctx, false);
    assert_eq!(selection(&h), (0, 1));
    h.q.next_match(&h.ctx, false);
    assert_eq!(selection(&h), (4, 5));
    h.q.next_match(&h.ctx, false);
    assert_eq!(selection(&h), (0, 1));
    h.q.next_match(&h.ctx, true);
    assert_eq!(selection(&h), (4, 5));
    assert_eq!(h.q.docs[0].text, "π a π");
}
#[test]
fn find_missing_text_keeps_caret_and_reports_no_match() {
    let mut h = Harness::new();
    h.editor("keep");
    h.q.set_selection(&h.ctx, 2, 2);
    h.q.search = "absent".into();
    h.q.next_match(&h.ctx, false);
    assert_eq!(h.q.caret(&h.ctx), 2);
    assert!(h.q.status.contains("No matching"));
}
#[test]
fn replace_one_uses_selected_unicode_match() {
    let mut h = Harness::new();
    h.editor("π π");
    h.q.search = "π".into();
    h.q.replace_with = "✓✓".into();
    h.q.set_selection(&h.ctx, 2, 3);
    h.q.replace_next(&h.ctx);
    assert_eq!(h.q.docs[0].text, "π ✓✓");
    assert_eq!(h.q.caret(&h.ctx), 4);
}
#[test]
fn replace_one_missing_or_empty_query_does_not_modify_document() {
    let mut h = Harness::new();
    h.editor("keep");
    for query in ["absent", ""] {
        h.q.search = query.into();
        h.q.replace_with = "changed".into();
        h.q.replace_next(&h.ctx);
        assert_eq!(h.q.docs[0].text, "keep");
    }
}
#[test]
fn replace_all_replaces_each_match_and_reports_count() {
    let mut h = Harness::new();
    h.editor("π π π");
    h.q.search = "π".into();
    h.q.replace_with = "new".into();
    h.q.replace_all(&h.ctx);
    assert_eq!(h.q.docs[0].text, "new new new");
    assert_eq!(h.q.status, "Replaced 3 occurrence(s)");
}
#[test]
fn replace_all_no_matches_or_empty_query_keeps_document() {
    let mut h = Harness::new();
    h.editor("keep");
    for query in ["absent", ""] {
        h.q.search = query.into();
        h.q.replace_with = "changed".into();
        h.q.replace_all(&h.ctx);
        assert_eq!(h.q.docs[0].text, "keep");
    }
}
#[test]
fn duplicate_line_updates_cursor_and_invalidates_diagnostics() {
    let mut h = Harness::new();
    h.editor("πx\nlast");
    h.q.set_selection(&h.ctx, 1, 1);
    let rev = h.q.diagnostics_revision;
    h.q.duplicate_active_line(&h.ctx);
    assert_eq!(h.q.docs[0].text, "πx\nπx\nlast");
    assert_eq!(h.q.caret(&h.ctx), 4);
    assert!(h.q.diagnostics_revision > rev);
}
#[test]
fn duplicate_line_on_empty_document_and_missing_editor_is_safe() {
    let mut h = Harness::new();
    h.q.duplicate_active_line(&h.ctx);
    assert!(h.q.docs.is_empty());
    h.editor("");
    h.q.duplicate_active_line(&h.ctx);
    assert_eq!(h.q.docs[0].text, "\n");
}
#[test]
fn next_tab_wraps_and_requests_editor_focus() {
    let mut h = Harness::new();
    for _ in 0..2 {
        h.q.new_document();
    }
    h.q.next_tab(false);
    assert_eq!(h.q.active, Some(0));
    assert!(h.q.focus_editor);
    h.q.next_tab(true);
    assert_eq!(h.q.active, Some(1));
}
#[test]
fn tab_navigation_handles_welcome_and_no_open_documents() {
    let mut h = Harness::new();
    h.q.next_tab(true);
    assert!(h.q.active.is_none());
    h.q.new_document();
    h.q.active = None;
    h.q.next_tab(false);
    assert_eq!(h.q.active, Some(0));
}

fn selection(h: &Harness) -> (usize, usize) {
    let s = TextEdit::load_state(&h.ctx, h.q.editor_id().unwrap())
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    (
        s.primary.index.min(s.secondary.index),
        s.primary.index.max(s.secondary.index),
    )
}

fn click_beside(h: &mut Harness, label: &str) {
    let mut reference = None;
    let mut crosses = vec![];
    for _ in 0..3 {
        let out = h.draw(vec![]);
        crosses.clear();
        for s in out.shapes {
            if let egui::epaint::Shape::Text(t) = s.shape {
                let p = t.pos + t.galley.size() * 0.5;
                if t.galley.text() == label {
                    reference = Some(p);
                }
                if t.galley.text() == "×" {
                    crosses.push(p);
                }
            }
        }
    }
    let r = reference.unwrap_or_else(|| panic!("Missing label {label}"));
    let p = crosses
        .into_iter()
        .filter(|p| p.x > r.x && (p.y - r.y).abs() < 8.)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .expect("Missing adjacent dismiss button");
    h.click_at(p);
}
#[test]
fn removing_recent_project_persists_without_removing_workspace_or_files() {
    let mut h = Harness::new();
    let p = h.file("recent-app.hyproj", "id='app'");
    h.q.add_project(p.clone());
    h.q.active = None;
    click_beside(&mut h, "recent-app");
    assert!(!h
        .stored_settings()
        .recent_workspaces
        .unwrap()
        .iter()
        .any(|e| e.path == p && e.project));
    assert_eq!(h.q.settings.projects, vec![p]);
    assert_eq!(h.q.docs.len(), 1);
    assert!(h.q.docs[0].path.as_ref().unwrap().exists());
}
#[test]
fn removing_recent_folder_keeps_workspace_and_stays_removed_after_reload() {
    let mut h = Harness::new();
    let path = h.dir.path().join("recent-folder");
    fs::create_dir(&path).unwrap();
    h.q.add_folder(path.clone());
    click_beside(&mut h, "recent-folder");
    let restored = Quark::from_state(h.stored_settings(), Session::default(), None, None);
    assert!(restored.settings.recent_workspaces.unwrap().is_empty());
    assert_eq!(restored.settings.folders, vec![path.clone()]);
    assert!(path.exists());
}
#[test]
fn reopening_removed_recent_workspace_restores_it_once_at_the_front() {
    let mut h = Harness::new();
    let a = h.file("a.hyproj", "id='a'");
    h.q.add_project(a.clone());
    let entry = RecentWorkspace {
        path: a.clone(),
        project: true,
    };
    h.q.remove_recent(&entry);
    h.q.add_project(a.clone());
    h.q.add_project(a);
    let entries = h.stored_settings().recent_workspaces.unwrap();
    assert_eq!(entries[0], entry);
    assert_eq!(entries.iter().filter(|e| **e == entry).count(), 1);
}
#[test]
fn recent_history_migrates_old_settings_without_readding_removed_entries() {
    let path = PathBuf::from("old.hyproj");
    let settings = Settings {
        projects: vec![path.clone()],
        ..Default::default()
    };
    let old = Quark::from_state(settings, Session::default(), None, None);
    assert_eq!(
        old.settings.recent_workspaces.unwrap(),
        vec![RecentWorkspace {
            path: path.clone(),
            project: true
        }]
    );
    let removed = Settings {
        projects: vec![path],
        recent_workspaces: Some(vec![]),
        ..Default::default()
    };
    assert!(Quark::from_state(removed, Session::default(), None, None)
        .settings
        .recent_workspaces
        .unwrap()
        .is_empty());
}
#[test]
fn recent_history_is_bounded_and_keeps_newest_first() {
    let mut h = Harness::new();
    for n in 0..25 {
        h.q.remember_workspace(PathBuf::from(format!("{n}.hyproj")), true);
    }
    let entries = h.q.settings.recent_workspaces.as_ref().unwrap();
    assert_eq!(entries.len(), 20);
    assert_eq!(entries[0].path, PathBuf::from("24.hyproj"));
    assert_eq!(entries.last().unwrap().path, PathBuf::from("5.hyproj"));
}
#[test]
fn shortcut_guide_dismiss_button_hides_only_that_card_and_persists() {
    let mut h = Harness::new();
    click_beside(&mut h, "Coming from VS Code?");
    assert!(!h.q.settings.welcome_shortcuts_visible);
    assert!(h.q.settings.welcome_build_visible);
    assert!(!h.stored_settings().welcome_shortcuts_visible);
    let out = h.draw(vec![]);
    assert!(!has_text(&out, "Coming from VS Code?"));
    assert!(has_text(&out, "Your first build"));
}
#[test]
fn shortcut_guide_can_be_restored_from_welcome_page() {
    let mut h = Harness::new();
    h.q.settings.welcome_shortcuts_visible = false;
    h.click("Restore hidden sections");
    assert!(h.q.settings.welcome_shortcuts_visible);
    assert!(h.stored_settings().welcome_shortcuts_visible);
    assert!(has_text(&h.draw(vec![]), "Coming from VS Code?"));
}
#[test]
fn first_build_dismiss_button_hides_only_that_card_and_persists() {
    let mut h = Harness::new();
    click_beside(&mut h, "Your first build");
    assert!(!h.q.settings.welcome_build_visible);
    assert!(h.q.settings.welcome_shortcuts_visible);
    assert!(!h.stored_settings().welcome_build_visible);
    let out = h.draw(vec![]);
    assert!(!has_text(&out, "Your first build"));
    assert!(has_text(&out, "Coming from VS Code?"));
}
#[test]
fn first_build_guide_can_be_restored_after_preferences_reload() {
    let mut h = Harness::new();
    h.q.settings.welcome_build_visible = false;
    h.q.persist();
    h.q.settings = h.stored_settings();
    h.click("Restore hidden sections");
    assert!(h.q.settings.welcome_build_visible);
    assert!(h.stored_settings().welcome_build_visible);
    assert!(has_text(&h.draw(vec![]), "Configure compiler…"));
}
#[test]
fn both_hidden_welcome_sections_leave_a_restore_control() {
    let mut h = Harness::new();
    h.q.settings.welcome_build_visible = false;
    h.q.settings.welcome_shortcuts_visible = false;
    let out = h.draw(vec![]);
    assert!(has_text(&out, "Restore hidden sections"));
    assert!(!has_text(&out, "Coming from VS Code?"));
    assert!(!has_text(&out, "Your first build"));
    h.click("Restore hidden sections");
    assert!(h.q.settings.welcome_build_visible && h.q.settings.welcome_shortcuts_visible);
}
#[test]
fn welcome_visibility_settings_can_hide_each_guide_independently() {
    let mut h = Harness::new();
    h.q.show_settings = true;
    h.click("Show VS Code shortcut guide");
    assert!(!h.q.settings.welcome_shortcuts_visible && h.q.settings.welcome_build_visible);
    h.click("Show first-build guide");
    let s = h.stored_settings();
    assert!(!s.welcome_shortcuts_visible && !s.welcome_build_visible);
}

macro_rules! theme_cases {
    ($apply:ident,$persist:ident,$preset:ident,$label:literal) => {
        #[test]
        fn $apply() {
            let mut h = Harness::new();
            h.q.show_sidebar(SidebarView::Themes);
            h.q.enable_theme(
                &format!("{:?}", theme::Preset::$preset).to_lowercase(),
                false,
            );
            h.draw(vec![]);
            let definition = theme::Preset::$preset.definition();
            assert_eq!(theme::colors(&h.ctx), definition.colors);
            assert_eq!(h.ctx.style().visuals.dark_mode, definition.dark);
            assert_eq!(theme::current(&h.ctx).syntax, definition.syntax);
        }
        #[test]
        fn $persist() {
            let mut h = Harness::new();
            h.q.show_sidebar(SidebarView::Themes);
            h.q.settings.theme.custom = Some(theme::Preset::Dawn.definition().clone());
            h.q.enable_theme(
                &format!("{:?}", theme::Preset::$preset).to_lowercase(),
                false,
            );
            let s = h.stored_settings();
            assert_eq!(s.theme.preset, theme::Preset::$preset);
            assert!(s.theme.custom.is_none());
            assert_eq!(s.theme.definition().name, $label);
        }
    };
}
theme_cases!(
    graphite_applies_to_interface_and_syntax,
    graphite_selection_clears_custom_theme_and_persists,
    Graphite,
    "Graphite"
);
theme_cases!(
    aurora_applies_to_interface_and_syntax,
    aurora_selection_clears_custom_theme_and_persists,
    Aurora,
    "Aurora"
);
theme_cases!(
    dawn_applies_to_interface_and_syntax,
    dawn_selection_clears_custom_theme_and_persists,
    Dawn,
    "Dawn"
);
theme_cases!(
    contrast_applies_to_interface_and_syntax,
    contrast_selection_clears_custom_theme_and_persists,
    Contrast,
    "High Contrast"
);
#[test]
fn theme_import_applies_and_persists_a_valid_custom_palette() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Dawn.definition().clone();
    d.name = "Imported".into();
    d.colors.accent = theme::Rgb([13, 25, 37]);
    let p = h.file("theme.json", &serde_json::to_string(&d).unwrap());
    h.q.import_theme(&p, &h.ctx);
    assert_eq!(theme::colors(&h.ctx).accent, d.colors.accent);
    assert_eq!(h.stored_settings().theme.custom, Some(d));
}
#[test]
fn invalid_or_missing_theme_import_preserves_current_theme() {
    let mut h = Harness::new();
    let old = h.q.settings.theme.clone();
    let p = h.file("bad.json", "{}");
    for p in [p, h.dir.path().join("missing.json")] {
        h.q.import_theme(&p, &h.ctx);
        assert_eq!(h.q.settings.theme, old);
        assert!(h.q.status.contains("Theme import failed"));
    }
}
#[test]
fn theme_export_roundtrips_custom_interface_syntax_icons_and_terminal() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Aurora.definition().clone();
    d.icon_scale = 125;
    d.syntax.keyword = theme::Rgb([1, 2, 3]);
    d.colors.ansi[7] = theme::Rgb([4, 5, 6]);
    h.q.settings.theme.custom = Some(d.clone());
    let p = h.dir.path().join("export.json");
    h.q.export_theme(&p, &h.q.settings.theme.definition().clone());
    assert_eq!(theme::parse(&fs::read_to_string(p).unwrap()).unwrap(), d);
}
#[test]
fn theme_export_failure_preserves_existing_destination_and_reports_error() {
    let mut h = Harness::new();
    let p = h.file("blocker", "keep");
    h.q.export_theme(
        &p.join("theme.json"),
        &h.q.settings.theme.definition().clone(),
    );
    assert!(h.q.status.contains("Theme export failed"));
    assert_eq!(fs::read_to_string(p).unwrap(), "keep");
}
#[test]
fn theme_custom_name_edit_applies_live_and_is_debounced() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.draw(vec![]);
    h.draw(vec![]);
    h.q.editing_theme = Some("graphite".into());
    h.draw(vec![]);
    h.draw(vec![]);
    h.ctx
        .memory_mut(|m| m.request_focus(egui::Id::new("theme-option-name")));
    h.key(Key::A, Modifiers::COMMAND);
    h.draw(vec![egui::Event::Text("My palette".into())]);
    assert_eq!(h.q.settings.theme.definition().name, "My palette");
    assert!(h.q.theme_persist_due.is_some());
    for _ in 0..12 {
        h.draw(vec![]);
    }
    assert!(h.q.theme_persist_due.is_none());
    assert_eq!(h.stored_settings().theme.definition().name, "My palette");
}
#[test]
fn custom_theme_is_saved_on_exit_before_debounce_expires() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Graphite.definition().clone();
    d.name = "Pending".into();
    h.q.settings.theme.custom = Some(d);
    h.q.theme_persist_due = Some(1000.);
    h.q.on_exit(None);
    assert_eq!(h.stored_settings().theme.definition().name, "Pending");
    assert!(h.q.theme_persist_due.is_none());
}
#[test]
fn output_clear_button_empties_log_without_changing_document() {
    let mut h = Harness::new();
    h.editor("keep");
    h.q.settings.terminal_visible = true;
    h.q.settings.bottom_tab = BottomTab::Output;
    h.q.output = "old output".into();
    h.click("Clear");
    assert!(h.q.output.is_empty());
    assert_eq!(h.q.docs[0].text, "keep");
    assert!(h.q.terminal.is_none());
}
#[test]
fn output_clear_while_job_runs_does_not_cancel_it() {
    let mut h = Harness::new();
    let _tx = h.busy();
    h.q.settings.terminal_visible = true;
    h.q.settings.bottom_tab = BottomTab::Output;
    h.q.output = "old".into();
    h.click("Clear");
    assert!(h.q.output.is_empty());
    assert!(h.q.job_rx.is_some());
    assert!(!h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
}
#[test]
fn copy_log_exports_complete_unicode_output() {
    let mut h = Harness::new();
    h.q.settings.terminal_visible = true;
    h.q.settings.bottom_tab = BottomTab::Output;
    h.q.output = "π\nline two\n".into();
    let out = h.click("Copy log");
    assert!(out
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="π\nline two\n")));
    assert_eq!(h.q.output, "π\nline two\n");
}
#[test]
fn copying_empty_log_does_not_change_running_job() {
    let mut h = Harness::new();
    let _tx = h.busy();
    h.q.output.clear();
    h.q.settings.terminal_visible = true;
    h.q.settings.bottom_tab = BottomTab::Output;
    h.click("Copy log");
    assert!(h.q.output.is_empty());
    assert!(h.q.job_rx.is_some());
    assert!(!h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
}
#[test]
fn stop_job_button_sets_cancel_without_editing_output() {
    let mut h = Harness::new();
    let _tx = h.busy();
    h.q.output = "keep log".into();
    h.q.settings.terminal_visible = true;
    h.q.settings.bottom_tab = BottomTab::Output;
    h.click("Stop job");
    assert!(h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
    assert_eq!(h.q.output, "keep log");
}
#[test]
fn stop_job_palette_action_handles_running_and_missing_jobs() {
    let mut h = Harness::new();
    h.q.action(21, &h.ctx);
    assert!(h.q.job_cancel.is_none());
    let _tx = h.busy();
    h.q.action(21, &h.ctx);
    assert!(h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
}
#[test]
fn output_tab_is_read_only_under_text_paste_and_enter() {
    let mut h = Harness::new();
    h.q.action(27, &h.ctx);
    h.q.output = "immutable log".into();
    h.draw(vec![]);
    h.click_at(egui::pos2(550., 1220.));
    h.draw(vec![
        egui::Event::Text("typed".into()),
        egui::Event::Paste("pasted".into()),
    ]);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.q.output, "immutable log");
    assert!(h.q.terminal.is_none());
}
#[test]
fn switching_output_and_terminal_tabs_preserves_log_and_reports_shell_error() {
    let mut h = Harness::new();
    h.q.output = "retained".into();
    h.q.terminal_error = Some("test shell unavailable".into());
    h.q.action(27, &h.ctx);
    h.click("Terminal");
    assert_eq!(h.q.settings.bottom_tab, BottomTab::Terminal);
    assert!(has_text(&h.draw(vec![]), "test shell unavailable"));
    h.click("Output");
    assert_eq!(h.q.output, "retained");
    assert_eq!(h.stored_settings().bottom_tab, BottomTab::Output);
}
#[test]
fn disconnected_tool_worker_reports_failure_and_releases_job_slot() {
    let mut h = Harness::new();
    let tx = h.busy();
    drop(tx);
    h.draw(vec![]);
    assert!(h.q.job_rx.is_none());
    assert!(h.q.job_cancel.is_none());
    assert!(h.q.output.contains("Command worker disconnected"));
}
#[test]
fn finished_tool_stream_retains_output_and_real_exit_status() {
    let mut h = Harness::new();
    let tx = h.busy();
    tx.send(command::Event::Output("partial π".into())).unwrap();
    tx.send(command::Event::Finished(Ok(7))).unwrap();
    h.draw(vec![]);
    assert!(h.q.output.contains("partial π"));
    assert!(h.q.output.contains("Exit code: 7"));
    assert!(h.q.status.contains("exit code 7"));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn clone_success_registers_destination_after_completion() {
    let mut h = Harness::new();
    let destination = h.dir.path().join("clone");
    fs::create_dir(&destination).unwrap();
    let tx = h.busy();
    h.q.pending_clone = Some(destination.clone());
    assert!(h.q.settings.folders.is_empty());
    tx.send(command::Event::Finished(Ok(0))).unwrap();
    h.draw(vec![]);
    assert!(h
        .q
        .settings
        .folders
        .contains(&destination.canonicalize().unwrap()));
    assert!(h.q.pending_clone.is_none());
}
#[test]
fn clone_failure_does_not_register_destination() {
    let mut h = Harness::new();
    let destination = h.dir.path().join("failed-clone");
    let tx = h.busy();
    h.q.pending_clone = Some(destination);
    tx.send(command::Event::Finished(Ok(1))).unwrap();
    h.draw(vec![]);
    assert!(h.q.settings.folders.is_empty());
    assert!(h.q.pending_clone.is_none());
}

fn extension_fixture(h: &mut Harness) -> String {
    let root = plugins::directory(h.q.settings_path.as_deref()).unwrap();
    let dir = root.join("test-extension");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("plugin.json"),r#"{"api_version":1,"id":"test-extension","name":"Test Extension","description":"Local workspace tools","commands":[{"title":"Inspect workspace","program":"quark-nonexistent-tool"}]}"#).unwrap();
    h.q.reload_plugins();
    "test-extension".into()
}
#[test]
fn theme_catalogue_search_filters_names_and_has_no_inline_editors() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.q.theme_query = "AURORA".into();
    h.draw(vec![]);
    let out = h.draw(vec![]);
    assert!(has_text(&out, "Aurora"));
    assert!(!has_text(&out, "Dawn"));
    assert!(!has_text(&out, "Theme name"));
    assert!(!has_text(&out, "Icon size %"));
}
#[test]
fn theme_search_empty_state_keeps_active_palette() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    let before = h.q.effective_theme();
    h.q.theme_query = "missing-theme-123".into();
    let out = h.draw(vec![]);
    assert!(has_text(&out, "No themes match"));
    assert_eq!(h.q.effective_theme(), before);
}
#[test]
fn theme_options_expose_management_commands_without_editing_the_item() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.q.theme_query = "Aurora".into();
    h.click("...");
    let out = h.draw(vec![]);
    for shape in &out.shapes {
        if let egui::epaint::Shape::Text(t) = &shape.shape {
            if t.galley.text() == "Enable" {
                assert!(shape.clip_rect.contains(t.pos));
                assert!(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280., 1400.))
                        .contains(t.pos)
                );
            }
        }
    }
    for label in [
        "Enable",
        "Enable (Workspace)",
        "Disable",
        "Disable (Workspace)",
        "Auto update",
        "Settings",
        "Open Containing Folder",
        "Download ZIP",
        "Remove",
    ] {
        assert!(has_text(&out, label), "Missing {label}");
    }
    assert!(h.q.editing_theme.is_none());
}
#[test]
fn theme_settings_opens_only_from_options_and_enable_applies_palette() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.q.theme_query = "Aurora".into();
    h.click("...");
    h.click("Enable");
    h.draw(vec![]);
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Aurora);
    h.click("...");
    h.click("Settings");
    assert_eq!(h.q.editing_theme.as_deref(), Some("aurora"));
    assert!(has_text(&h.draw(vec![]), "Theme name"));
}
#[test]
fn global_theme_disable_falls_back_and_enable_restores_selection() {
    let mut h = Harness::new();
    h.q.enable_theme("aurora", false);
    h.q.disable_theme("aurora", false);
    assert_ne!(h.q.effective_theme().preset, theme::Preset::Aurora);
    assert!(h
        .stored_settings()
        .catalog
        .disabled_themes
        .contains(&"aurora".into()));
    h.q.enable_theme("aurora", false);
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Aurora);
    assert!(!h
        .stored_settings()
        .catalog
        .disabled_themes
        .contains(&"aurora".into()));
}
#[test]
fn workspace_theme_enable_does_not_change_global_theme_or_other_workspace() {
    let mut h = Harness::new();
    let one = h.dir.path().join("one");
    let two = h.dir.path().join("two");
    h.q.settings.folders = vec![one];
    h.q.enable_theme("dawn", true);
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Dawn);
    assert_eq!(h.q.settings.theme.preset, theme::Preset::Graphite);
    h.q.settings.folders = vec![two];
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Graphite);
}
#[test]
fn workspace_theme_disable_preserves_global_availability_and_persists_scope() {
    let mut h = Harness::new();
    let root = h.dir.path().to_path_buf();
    h.q.settings.folders = vec![root.clone()];
    h.q.enable_theme("aurora", false);
    h.q.disable_theme("aurora", true);
    assert_ne!(h.q.effective_theme().preset, theme::Preset::Aurora);
    assert!(!h.stored_settings().catalog.workspace_theme_enabled[&root]["aurora"]);
    h.q.settings.folders.clear();
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Aurora);
}
#[test]
fn workspace_theme_controls_reject_missing_workspace_without_mutation() {
    let mut h = Harness::new();
    h.q.enable_theme("dawn", true);
    h.q.disable_theme("graphite", true);
    assert!(h.q.settings.catalog.workspace_theme.is_empty());
    assert!(h.q.settings.catalog.workspace_theme_enabled.is_empty());
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Graphite);
    assert!(h.q.status.contains("Open a workspace"));
}
#[test]
fn workspace_theme_enable_can_override_global_disable() {
    let mut h = Harness::new();
    h.q.disable_theme("aurora", false);
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.enable_theme("aurora", true);
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Aurora);
    h.q.settings.folders.clear();
    assert_ne!(h.q.effective_theme().preset, theme::Preset::Aurora);
}
#[test]
fn theme_auto_update_option_persists_preference_without_starting_network_job() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.q.theme_query = "Aurora".into();
    h.click("...");
    h.click("Auto update");
    assert!(h
        .stored_settings()
        .catalog
        .theme_auto_update
        .contains(&"aurora".into()));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn theme_auto_update_can_be_switched_off() {
    let mut h = Harness::new();
    h.q.settings.catalog.theme_auto_update = vec!["aurora".into()];
    h.q.show_sidebar(SidebarView::Themes);
    h.q.theme_query = "Aurora".into();
    h.click("...");
    h.click("Auto update");
    assert!(!h
        .stored_settings()
        .catalog
        .theme_auto_update
        .contains(&"aurora".into()));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn theme_settings_edits_are_local_overrides_not_bundled_file_writes() {
    let mut h = Harness::new();
    let before = theme::Preset::Aurora.definition().clone();
    let mut modified = before.clone();
    modified.name = "My Aurora".into();
    modified.colors.accent = theme::Rgb([5, 9, 13]);
    h.q.enable_theme("aurora", false);
    h.q.edit_theme("aurora", modified.clone()).unwrap();
    h.q.persist();
    assert_eq!(h.q.effective_theme().definition(), &modified);
    assert_eq!(theme::Preset::Aurora.definition(), &before);
    assert_eq!(
        h.stored_settings().catalog.customisations["aurora"],
        modified
    );
}
#[test]
fn theme_settings_reject_invalid_definition_without_altering_theme() {
    let mut h = Harness::new();
    let before = h.q.effective_theme();
    let mut invalid = before.definition().clone();
    invalid.icon_scale = 0;
    assert!(h.q.edit_theme("graphite", invalid).is_err());
    assert_eq!(h.q.effective_theme(), before);
    assert!(h.q.settings.catalog.customisations.is_empty());
}
#[test]
fn add_themes_opens_a_local_import_and_clear_repository_placeholder() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Themes);
    h.click("+ Add");
    let out = h.draw(vec![]);
    assert_eq!(h.q.catalog_add, Some(true));
    assert!(has_text(&out, "online catalogue is not connected"));
    assert!(has_text(&out, "Import local theme"));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn importing_local_theme_adds_one_catalogue_entry_and_preserves_bundled_themes() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Dawn.definition().clone();
    d.name = "Local Custom".into();
    let p = h.file("custom.json", &serde_json::to_string(&d).unwrap());
    h.q.import_theme(&p, &h.ctx);
    h.q.import_theme(&p, &h.ctx);
    assert_eq!(h.q.settings.catalog.themes().len(), 5);
    assert_eq!(h.q.settings.catalog.themes.len(), 1);
    assert_eq!(h.q.effective_theme().definition().name, "Local Custom");
}
#[test]
fn removing_local_theme_cleans_workspace_overrides_and_uses_default() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Dawn.definition().clone();
    d.name = "Remove me".into();
    let id = h.q.settings.catalog.install_theme(d);
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.enable_theme(&id, false);
    h.q.enable_theme(&id, true);
    h.q.remove_theme(&id).unwrap();
    assert!(h.q.settings.catalog.themes.is_empty());
    assert!(h.q.settings.catalog.workspace_theme.is_empty());
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Graphite);
    assert!(h.stored_settings().catalog.themes.is_empty());
}
#[test]
fn bundled_theme_removal_is_rejected_and_missing_theme_is_safe() {
    let mut h = Harness::new();
    assert!(h
        .q
        .remove_theme("graphite")
        .unwrap_err()
        .contains("Bundled"));
    assert!(h.q.remove_theme("missing").is_err());
    assert_eq!(h.q.settings.catalog.themes().len(), 4);
}
#[test]
fn theme_folder_materialises_valid_local_json_without_touching_bundled_source() {
    let mut h = Harness::new();
    let root = h.q.theme_folder("aurora").unwrap();
    let d = theme::parse(&fs::read_to_string(root.join("theme.json")).unwrap()).unwrap();
    assert_eq!(d, *theme::Preset::Aurora.definition());
    assert!(root.starts_with(h.dir.path()));
}
#[test]
fn theme_folder_rejects_unknown_and_path_traversal_identifiers() {
    let mut h = Harness::new();
    assert!(h.q.theme_folder("../outside").is_err());
    assert!(h.q.theme_folder("missing").is_err());
    assert!(!h.dir.path().join("themes").exists());
}
#[test]
fn extension_catalogue_search_matches_name_id_and_description() {
    let mut h = Harness::new();
    extension_fixture(&mut h);
    h.q.show_sidebar(SidebarView::Plugins);
    h.q.extension_query = "WORKSPACE test".into();
    h.draw(vec![]);
    let out = h.draw(vec![]);
    assert!(has_text(&out, "Test Extension"));
    assert!(!has_text(&out, "Inspect workspace"));
    assert!(h.q.editing_extension.is_none());
}
#[test]
fn extension_search_empty_state_does_not_disable_or_remove_installed_items() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.show_sidebar(SidebarView::Plugins);
    h.q.extension_query = "not-installed".into();
    assert!(has_text(&h.draw(vec![]), "No extensions match"));
    assert_eq!(h.q.plugins.len(), 1);
    assert!(h.q.extension_enabled(&id));
}
#[test]
fn extension_options_have_the_same_management_menu_as_themes() {
    let mut h = Harness::new();
    extension_fixture(&mut h);
    h.q.show_sidebar(SidebarView::Plugins);
    h.click("...");
    let out = h.draw(vec![]);
    for label in [
        "Enable",
        "Enable (Workspace)",
        "Disable",
        "Disable (Workspace)",
        "Auto update",
        "Settings",
        "Open Containing Folder",
        "Download ZIP",
        "Remove",
    ] {
        assert!(has_text(&out, label), "Missing {label}");
    }
    assert!(h.q.editing_extension.is_none());
}
#[test]
fn extension_settings_opens_actions_and_manifest_editor_only_on_request() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.show_sidebar(SidebarView::Plugins);
    h.click("...");
    h.click("Settings");
    assert_eq!(h.q.editing_extension, Some(id));
    let out = h.draw(vec![]);
    assert!(has_text(&out, "Inspect workspace"));
    assert!(h.q.docs.is_empty());
    h.click("Edit extension manifest…");
    assert_eq!(h.q.docs.len(), 1);
    assert_eq!(
        h.q.current_path().unwrap().file_name().unwrap(),
        "plugin.json"
    );
}
#[test]
fn global_extension_disable_blocks_execution_and_enable_restores_it() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.set_extension_enabled(&id, false, false);
    h.q.run_plugin(&id, 0);
    assert!(h.q.job_rx.is_none());
    assert!(h.q.status.contains("Enable the plugin"));
    h.q.set_extension_enabled(&id, true, false);
    assert!(h.q.extension_enabled(&id));
    assert!(h.stored_settings().disabled_plugins.is_empty());
}
#[test]
fn workspace_extension_disable_does_not_disable_other_workspaces() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.settings.folders = vec![h.dir.path().join("one")];
    h.q.set_extension_enabled(&id, false, true);
    assert!(!h.q.extension_enabled(&id));
    assert!(h.q.settings.disabled_plugins.is_empty());
    h.q.settings.folders = vec![h.dir.path().join("two")];
    assert!(h.q.extension_enabled(&id));
}
#[test]
fn workspace_extension_enable_overrides_global_disable_and_persists() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.set_extension_enabled(&id, false, false);
    let root = h.dir.path().to_path_buf();
    h.q.settings.folders = vec![root.clone()];
    h.q.set_extension_enabled(&id, true, true);
    assert!(h.q.extension_enabled(&id));
    assert!(h.stored_settings().catalog.workspace_extensions[&root][&id]);
    h.q.settings.folders.clear();
    assert!(!h.q.extension_enabled(&id));
}
#[test]
fn extension_workspace_actions_require_a_workspace() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.set_extension_enabled(&id, false, true);
    assert!(h.q.extension_enabled(&id));
    assert!(h.q.settings.catalog.workspace_extensions.is_empty());
    assert!(h.q.status.contains("Open a workspace"));
}
#[test]
fn extension_auto_update_option_persists_without_launching_a_job() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.show_sidebar(SidebarView::Plugins);
    h.click("...");
    h.click("Auto update");
    assert!(h
        .stored_settings()
        .catalog
        .extension_auto_update
        .contains(&id));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn extension_auto_update_can_be_switched_off() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.settings.catalog.extension_auto_update = vec![id];
    h.q.show_sidebar(SidebarView::Plugins);
    h.click("...");
    h.click("Auto update");
    assert!(h.stored_settings().catalog.extension_auto_update.is_empty());
}
#[test]
fn add_extensions_shows_local_install_instructions_and_repository_placeholder() {
    let mut h = Harness::new();
    h.q.show_sidebar(SidebarView::Plugins);
    h.click("+ Add");
    let out = h.draw(vec![]);
    assert_eq!(h.q.catalog_add, Some(false));
    assert!(has_text(&out, "plugin.json"));
    assert!(has_text(&out, "online catalogue is not connected"));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn extension_reload_picks_up_saved_manifest_changes() {
    let mut h = Harness::new();
    extension_fixture(&mut h);
    let p = h.q.plugins[0].directory.join("plugin.json");
    let text = fs::read_to_string(&p)
        .unwrap()
        .replace("Test Extension", "Changed Extension");
    fs::write(p, text).unwrap();
    h.q.reload_plugins();
    assert_eq!(h.q.plugins[0].name, "Changed Extension");
    assert_eq!(h.q.plugins.len(), 1);
}
#[test]
fn removing_extension_moves_files_to_backup_and_cleans_preferences() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    let directory = h.q.plugins[0].directory.clone();
    h.q.set_extension_enabled(&id, false, false);
    h.q.remove_extension(&id).unwrap();
    assert!(!directory.exists());
    assert!(h.q.plugins.is_empty());
    assert!(!h.q.settings.disabled_plugins.contains(&id));
    let backup = fs::read_dir(h.dir.path().join("extension-backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(backup.join("test-extension/plugin.json").exists());
}
#[test]
fn extension_removal_rejects_running_jobs_and_unmanaged_paths() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    let tx = h.busy();
    assert!(h.q.remove_extension(&id).unwrap_err().contains("Stop"));
    drop(tx);
    h.q.job_rx = None;
    let outside = h.dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    h.q.plugins[0].directory = outside.clone();
    assert!(h.q.remove_extension(&id).unwrap_err().contains("outside"));
    assert!(outside.exists());
    assert_eq!(h.q.plugins.len(), 1);
}
#[test]
fn extension_removal_confirmation_cancel_keeps_extension_installed() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.pending_extension_remove = Some(id);
    h.click("Cancel");
    assert!(h.q.pending_extension_remove.is_none());
    assert_eq!(h.q.plugins.len(), 1);
    assert!(h.q.plugins[0].directory.exists());
}
#[test]
fn local_zip_export_preserves_nested_theme_and_extension_contents() {
    use std::io::Read;
    let h = Harness::new();
    let root = h.dir.path().join("package");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("plugin.json"), "manifest").unwrap();
    fs::write(root.join("nested/theme.json"), "π✓").unwrap();
    let destination = h.dir.path().join("package.zip");
    catalog::export_zip(&root, &destination).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(destination).unwrap()).unwrap();
    assert_eq!(zip.len(), 2);
    let mut content = String::new();
    zip.by_name("nested/theme.json")
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(content, "π✓");
}
#[test]
fn failed_zip_export_preserves_existing_destination_and_leaves_no_temporary_file() {
    let h = Harness::new();
    let p = h.file("package.zip", "keep");
    assert!(catalog::export_zip(&h.dir.path().join("missing"), &p).is_err());
    assert_eq!(fs::read_to_string(p).unwrap(), "keep");
    assert_eq!(fs::read_dir(h.dir.path()).unwrap().count(), 1);
}

macro_rules! tool_action_cases {
    ($route:ident,$blocked:ident,$verb:literal) => {
        #[test]
        fn $route() {
            let mut h = Harness::new();
            let p = h.file("app.hyproj", "name='App'\ntype='exe'");
            let other = h.file("other.hy", "saved");
            h.q.add_project(p.clone());
            h.q.open(other);
            h.q.settings.compiler = "quark-missing-compiler-for-test".into();
            h.q.settings.compiler_verbs.push($verb.into());
            h.q.hydrogen($verb);
            assert_eq!(h.q.settings.bottom_tab, BottomTab::Output);
            assert!(h.q.output.contains(&format!("{:?}", $verb)));
            assert!(h
                .q
                .output
                .contains(&format!("{:?}", p.to_string_lossy().as_ref())));
            assert!(h.q.terminal.is_none());
        }
        #[test]
        fn $blocked() {
            let mut h = Harness::new();
            let tx = h.busy();
            h.q.settings.compiler_verbs.push($verb.into());
            h.q.hydrogen($verb);
            assert!(h.q.status.contains("Stop the current tool job"));
            assert!(h.q.job_rx.is_some());
            assert!(!h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
            drop(tx);
        }
    };
}
tool_action_cases!(
    check_routes_selected_project_to_output,
    check_rejects_overlapping_job,
    "check"
);
tool_action_cases!(
    run_routes_selected_project_to_output,
    run_rejects_overlapping_job,
    "run"
);
#[test]
fn project_build_saves_dirty_manifest_and_routes_generated_output_path() {
    let mut h = Harness::new();
    let p = h.file("app.hyproj", "id='old'\ntype='exe'");
    h.q.add_project(p.clone());
    h.q.docs[0].text = "id='new'\ntype='exe'".into();
    h.q.settings.compiler = "quark-missing-compiler-for-test".into();
    h.q.build(BuildScope::Project);
    assert!(fs::read_to_string(p).unwrap().contains("new"));
    assert!(h.q.output.contains("\"build\""));
    assert!(h.q.output.contains(".quark"));
    assert_eq!(h.q.settings.bottom_tab, BottomTab::Output);
}
#[test]
fn active_file_compile_routes_only_source_and_explicit_output() {
    let mut h = Harness::new();
    let p = h.file("file.hy", "class A {}");
    h.q.open(p.clone());
    h.q.settings.compiler = "quark-missing-compiler-for-test".into();
    h.q.settings.compiler_verbs.push("compile".into());
    h.q.build(BuildScope::CurrentFile);
    assert!(h.q.output.contains("\"compile\""));
    assert!(h
        .q
        .output
        .contains(&format!("{:?}", p.to_string_lossy().as_ref())));
    assert!(h.q.output.contains("\"-o\""));
}
#[test]
fn build_conflict_aborts_before_launching_compiler() {
    let mut h = Harness::new();
    let p = h.file("app.hyproj", "id='old'");
    h.q.add_project(p.clone());
    h.q.docs[0].text = "id='mine'".into();
    fs::write(p, "id='external'").unwrap();
    h.q.build(BuildScope::Project);
    assert_eq!(h.q.pending_conflict, Some(0));
    assert!(h.q.job_rx.is_none());
}
#[test]
fn sdk_and_single_file_profiles_set_only_supported_verbs() {
    let mut h = Harness::new();
    h.q.show_settings = true;
    h.click("Single-file compiler profile");
    assert_eq!(h.q.settings.compiler_verbs, vec!["compile"]);
    h.click("SDK profile");
    assert_eq!(h.q.settings.compiler_verbs, vec!["build", "check", "run"]);
}
#[test]
fn capability_detection_applies_supported_help_output() {
    let mut h = Harness::new();
    let tx = h.busy();
    h.q.probe_running = true;
    tx.send(command::Event::Output("Commands: compile check\n".into()))
        .unwrap();
    tx.send(command::Event::Finished(Ok(0))).unwrap();
    h.draw(vec![]);
    assert_eq!(h.q.settings.compiler_verbs, vec!["compile", "check"]);
    assert!(!h.q.probe_running);
    assert_eq!(h.stored_settings().compiler_verbs, vec!["compile", "check"]);
}
#[test]
fn capability_detection_empty_help_keeps_profile_and_warns() {
    let mut h = Harness::new();
    let before = h.q.settings.compiler_verbs.clone();
    let tx = h.busy();
    h.q.probe_running = true;
    tx.send(command::Event::Output("no commands advertised".into()))
        .unwrap();
    tx.send(command::Event::Finished(Ok(0))).unwrap();
    h.draw(vec![]);
    assert_eq!(h.q.settings.compiler_verbs, before);
    assert!(h.q.status.contains("No supported commands"));
}
#[test]
fn extension_action_requires_selected_context_and_routes_errors_to_output() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.plugins[0].commands[0].needs_project = true;
    h.q.run_plugin(&id, 0);
    assert!(h.q.job_rx.is_none());
    assert!(h.q.output.contains("Select a Hydrogen project"));
    assert_eq!(h.q.settings.bottom_tab, BottomTab::Output);
}
#[test]
fn enabled_extension_action_routes_to_output_with_workspace_directory() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.run_plugin(&id, 0);
    assert!(h.q.job_rx.is_some());
    assert!(h.q.output.contains("quark-nonexistent-tool"));
    assert_eq!(h.q.job_cwd, Some(h.dir.path().to_path_buf()));
    assert!(h.q.terminal.is_none());
}
macro_rules! git_action_cases {
    ($route:ident,$blocked:ident,$label:literal,$first:literal) => {
        #[test]
        fn $route() {
            let mut h = Harness::new();
            h.q.settings.folders = vec![h.dir.path().to_path_buf()];
            h.q.show_sidebar(SidebarView::SourceControl);
            h.click($label);
            assert!(h.q.output.contains(&format!("\"{}\"", $first)));
            assert_eq!(h.q.settings.bottom_tab, BottomTab::Output);
            assert!(h.q.terminal.is_none());
        }
        #[test]
        fn $blocked() {
            let mut h = Harness::new();
            let _tx = h.busy();
            h.q.show_sidebar(SidebarView::SourceControl);
            let before = h.q.output.clone();
            h.click($label);
            assert_eq!(h.q.output, before);
            assert!(h.q.job_rx.is_some());
            assert!(!h.q.job_cancel.as_ref().unwrap().load(Ordering::Relaxed));
        }
    };
}
git_action_cases!(
    git_status_button_routes_cli_arguments,
    git_status_button_is_disabled_during_job,
    "Status",
    "status"
);
git_action_cases!(
    git_diff_button_routes_cli_arguments,
    git_diff_button_is_disabled_during_job,
    "Diff",
    "diff"
);
git_action_cases!(
    git_fetch_button_routes_cli_arguments,
    git_fetch_button_is_disabled_during_job,
    "Fetch",
    "fetch"
);
git_action_cases!(
    git_pull_button_routes_cli_arguments,
    git_pull_button_is_disabled_during_job,
    "Pull",
    "pull"
);
git_action_cases!(
    git_push_button_routes_cli_arguments,
    git_push_button_is_disabled_during_job,
    "Push",
    "push"
);
#[test]
fn minimising_window_preserves_dirty_document_and_running_job() {
    let mut h = Harness::new();
    h.editor("unsaved");
    let _tx = h.busy();
    let out = h.click_at(egui::pos2(1192., 14.));
    assert!(has_command(&out, egui::ViewportCommand::Minimized(true)));
    assert_eq!(h.q.docs[0].text, "unsaved");
    assert!(h.q.job_rx.is_some());
    assert!(!h.q.exit_requested);
}
#[test]
fn native_title_bar_setting_emits_decoration_command_and_persists() {
    let mut h = Harness::new();
    h.q.show_settings = true;
    h.click("Integrated window controls");
    let out = h.draw(vec![]);
    assert!(!h.q.settings.integrated_title_bar);
    assert!(!h.stored_settings().integrated_title_bar);
    assert!(h.q.applied_window_chrome == Some(false));
    assert!(!has_command(&out, egui::ViewportCommand::Close));
}
#[test]
fn automatic_completion_can_be_disabled_without_disabling_manual_request() {
    let mut h = Harness::new();
    h.editor("cla");
    assert!(h.q.completion.is_none());
    h.q.set_selection(&h.ctx, 3, 3);
    h.key(Key::Space, Modifiers::COMMAND);
    assert!(h.q.completion.is_some());
}
#[test]
fn completion_escape_dismisses_without_changing_source() {
    let mut h = Harness::new();
    h.editor("cla");
    h.q.set_selection(&h.ctx, 3, 3);
    h.key(Key::Space, Modifiers::COMMAND);
    assert!(h.q.completion.is_some());
    h.key(Key::Escape, Modifiers::NONE);
    assert!(h.q.completion.is_none());
    assert_eq!(h.q.docs[0].text, "cla");
}
#[test]
fn completion_accept_inserts_selected_candidate_at_unicode_caret() {
    let mut h = Harness::new();
    h.editor("π cla");
    h.q.set_selection(&h.ctx, 5, 5);
    h.key(Key::Space, Modifiers::COMMAND);
    assert!(h.q.completion.is_some());
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.q.docs[0].text, "π class");
}
#[test]
fn stale_workspace_search_results_are_discarded() {
    let mut h = Harness::new();
    h.q.search_generation = 2;
    let (tx, rx) = mpsc::channel();
    h.q.search_rx = Some(rx);
    tx.send(panels::SearchResults {
        generation: 1,
        query: "stale".into(),
        ..Default::default()
    })
    .unwrap();
    h.q.poll_search();
    assert_ne!(h.q.search_results.query, "stale");
    assert!(h.q.search_rx.is_none());
}
#[test]
fn disconnected_workspace_search_keeps_previous_results_and_warns() {
    let mut h = Harness::new();
    h.q.search_results.query = "retained".into();
    let (tx, rx) = mpsc::channel();
    drop(tx);
    h.q.search_rx = Some(rx);
    h.q.poll_search();
    assert_eq!(h.q.search_results.query, "retained");
    assert!(h.q.status.contains("search stopped unexpectedly"));
}

#[test]
fn workspace_search_result_opens_matching_file_at_its_line() {
    let mut h = Harness::new();
    let p = h.file("hit.hy", "first\nsecond\nneedle");
    h.q.show_sidebar(SidebarView::Search);
    h.q.workspace_search = true;
    h.q.search_results = panels::search_files(1, "needle".into(), vec![p.clone()], vec![]);
    h.click("hit.hy:3");
    h.draw(vec![]);
    assert_eq!(h.q.current_path(), Some(p.canonicalize().unwrap()));
    assert_eq!(h.q.caret(&h.ctx), 13);
    assert_eq!(h.q.docs[0].text, "first\nsecond\nneedle");
}
#[test]
fn workspace_search_result_does_not_register_a_manifest_project() {
    let mut h = Harness::new();
    let p = h.file("hit.hyproj", "id='needle'");
    h.q.show_sidebar(SidebarView::Search);
    h.q.workspace_search = true;
    h.q.search_results = panels::search_files(1, "needle".into(), vec![p], vec![]);
    h.click("hit.hyproj:1");
    assert_eq!(h.q.docs.len(), 1);
    assert!(h.q.settings.projects.is_empty());
    assert_eq!(h.q.caret(&h.ctx), 0);
}
#[test]
fn normal_sidebar_switches_do_not_open_catalogue_settings_or_change_theme() {
    let mut h = Harness::new();
    let theme = h.q.effective_theme();
    for view in [
        SidebarView::Explorer,
        SidebarView::Search,
        SidebarView::SourceControl,
        SidebarView::Plugins,
        SidebarView::Themes,
    ] {
        h.q.show_sidebar(view);
        h.draw(vec![]);
        assert!(h.q.editing_theme.is_none() && h.q.editing_extension.is_none());
        assert_eq!(h.q.effective_theme(), theme);
    }
}
#[test]
fn invalid_restored_active_index_falls_back_to_existing_document() {
    let session = Session {
        docs: vec![Document {
            id: 40,
            path: None,
            text: "kept".into(),
            saved: String::new(),
        }],
        active: Some(99),
    };
    let mut q = Quark::from_state(Settings::default(), session, None, None);
    assert_eq!(q.active, Some(0));
    q.new_document();
    assert_eq!(q.docs[1].id, 41);
}
#[test]
fn automatic_completion_appears_after_typing_an_identifier_prefix() {
    let mut h = Harness::new();
    h.q.settings.automatic_completion = true;
    h.editor("");
    h.draw(vec![egui::Event::Text("cla".into())]);
    h.draw(vec![]);
    assert!(h.q.completion.is_some());
    assert!(h
        .q
        .completion
        .as_ref()
        .unwrap()
        .candidates
        .contains(&"class".into()));
}
#[test]
fn disabled_automatic_completion_does_not_open_when_typing() {
    let mut h = Harness::new();
    h.editor("");
    h.draw(vec![egui::Event::Text("cla".into())]);
    h.draw(vec![]);
    assert!(h.q.completion.is_none());
    assert_eq!(h.q.docs[0].text, "cla");
}
#[test]
fn workspace_theme_customisation_follows_selected_workspace_without_changing_global() {
    let mut h = Harness::new();
    let one = h.dir.path().join("one");
    h.q.settings.folders = vec![one];
    h.q.enable_theme("dawn", true);
    let mut d = theme::Preset::Dawn.definition().clone();
    d.syntax.keyword = theme::Rgb([23, 45, 67]);
    h.q.edit_theme("dawn", d.clone()).unwrap();
    assert_eq!(h.q.effective_theme().definition(), &d);
    h.q.settings.folders.clear();
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Graphite);
}
#[test]
fn removing_an_inactive_theme_keeps_other_customised_themes() {
    let mut h = Harness::new();
    let mut a = theme::Preset::Dawn.definition().clone();
    a.name = "Local A".into();
    let mut b = a.clone();
    b.name = "Local B".into();
    let id_a = h.q.settings.catalog.install_theme(a);
    let id_b = h.q.settings.catalog.install_theme(b.clone());
    h.q.enable_theme(&id_b, false);
    h.q.remove_theme(&id_a).unwrap();
    assert_eq!(h.q.effective_theme().definition(), &b);
    assert_eq!(h.q.settings.catalog.themes.len(), 1);
}
#[test]
fn resetting_theme_customisations_restores_bundled_definition() {
    let mut h = Harness::new();
    h.q.enable_theme("aurora", false);
    let mut d = theme::Preset::Aurora.definition().clone();
    d.name = "Modified".into();
    h.q.edit_theme("aurora", d).unwrap();
    h.q.editing_theme = Some("aurora".into());
    h.click("Reset customisations");
    h.draw(vec![]);
    assert_eq!(
        h.q.effective_theme().definition(),
        theme::Preset::Aurora.definition()
    );
    assert!(!h
        .stored_settings()
        .catalog
        .customisations
        .contains_key("aurora"));
}
#[test]
fn resetting_inactive_theme_does_not_switch_active_palette() {
    let mut h = Harness::new();
    let mut d = theme::Preset::Dawn.definition().clone();
    d.name = "Modified Dawn".into();
    h.q.edit_theme("dawn", d).unwrap();
    h.q.editing_theme = Some("dawn".into());
    h.click("Reset customisations");
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Graphite);
    assert!(!h.q.settings.catalog.customisations.contains_key("dawn"));
}
#[test]
fn globally_enabling_extension_does_not_clear_workspace_disable() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.set_extension_enabled(&id, false, true);
    h.q.set_extension_enabled(&id, true, false);
    assert!(!h.q.extension_enabled(&id));
    h.q.settings.folders.clear();
    assert!(h.q.extension_enabled(&id));
}
#[test]
fn globally_disabling_extension_does_not_clear_workspace_enable() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    h.q.settings.folders = vec![h.dir.path().to_path_buf()];
    h.q.set_extension_enabled(&id, true, true);
    h.q.set_extension_enabled(&id, false, false);
    assert!(h.q.extension_enabled(&id));
    h.q.settings.folders.clear();
    assert!(!h.q.extension_enabled(&id));
}
#[test]
fn removing_extension_via_confirmation_moves_its_folder_only() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    let workspace = h.file("project.hy", "keep");
    h.q.pending_extension_remove = Some(id);
    h.click("Remove extension");
    assert!(h.q.plugins.is_empty());
    assert!(h.q.pending_extension_remove.is_none());
    assert_eq!(fs::read_to_string(workspace).unwrap(), "keep");
}
#[test]
fn local_catalogue_import_migrates_a_previous_custom_theme() {
    let mut definition = theme::Preset::Aurora.definition().clone();
    definition.name = "Old custom".into();
    let settings = Settings {
        theme: theme::Settings {
            preset: theme::Preset::Aurora,
            custom: Some(definition.clone()),
        },
        ..Default::default()
    };
    let q = Quark::from_state(settings, Session::default(), None, None);
    assert!(q
        .settings
        .catalog
        .themes()
        .iter()
        .any(|t| t.definition == definition));
    assert_eq!(q.effective_theme().definition(), &definition);
}

#[test]
fn compact_header_and_left_sidebar_keep_their_layout_with_panels_hidden() {
    let mut h = Harness::new();
    h.q.settings.sidebar_visible = false;
    h.q.settings.terminal_visible = false;
    h.draw(vec![]);
    let out = h.draw(vec![]);
    assert!(h.q.top_rect.height() < 45.);
    assert!(h.q.bottom_rect == egui::Rect::NOTHING);
    assert!(has_text(&out, "Search files / commands"));
}
#[test]
fn editor_and_status_render_unicode_path_and_dirty_tab_marker() {
    let mut h = Harness::new();
    let p = h.file("π-file.hy", "class A {}");
    h.q.open(p.clone());
    h.q.docs[0].text.push_str(" // edit");
    h.draw(vec![]);
    let out = h.draw(vec![]);
    assert!(has_text(&out, "* π-file.hy"));
    assert!(has_text(&out, &p.display().to_string()));
    assert!(has_text(&out, "UTF-8"));
}
#[test]
fn document_labels_distinguish_clean_saved_and_dirty_untitled_tabs() {
    let mut h = Harness::new();
    h.q.new_document();
    assert_eq!(h.q.docs[0].label(), "Untitled-1");
    h.q.docs[0].text = "dirty".into();
    assert_eq!(h.q.docs[0].label(), "* Untitled-1");
    let p = h.file("saved.hy", "saved");
    h.q.open(p);
    assert_eq!(h.q.docs[1].label(), "saved.hy");
}
#[test]
fn search_keyboard_shortcut_focuses_sidebar_and_replace_action_shows_field() {
    let mut h = Harness::new();
    h.editor("keep");
    h.key(Key::F, Modifiers::COMMAND);
    assert_eq!(h.q.settings.sidebar_view, SidebarView::Search);
    assert!(h.q.show_search);
    h.q.action(23, &h.ctx);
    assert!(h.q.show_replace);
    assert_eq!(h.q.docs[0].text, "keep");
}
#[test]
fn search_workspace_shortcut_selects_workspace_mode_without_changing_editor() {
    let mut h = Harness::new();
    h.editor("keep");
    h.key(Key::F, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.q.settings.sidebar_view, SidebarView::Search);
    assert!(h.q.workspace_search);
    assert_eq!(h.q.docs[0].text, "keep");
}

fn click_explorer_control(h: &mut Harness, label: &str) {
    let mut pos = None;
    for _ in 0..3 {
        let out = h.draw(vec![]);
        for s in out.shapes {
            if let egui::epaint::Shape::Text(t) = s.shape {
                if t.galley.text() == label && t.pos.x < h.q.sidebar_rect.right() {
                    pos = Some(t.pos + t.galley.size() * 0.5);
                }
            }
        }
    }
    h.click_at(pos.unwrap());
}
#[test]
fn explorer_expand_all_updates_tree_state_and_revision() {
    let mut h = Harness::new();
    let rev = h.q.tree_revision;
    click_explorer_control(&mut h, "+");
    assert!(h.q.tree_default_open);
    assert!(h.q.tree_revision > rev);
}
#[test]
fn explorer_collapse_all_updates_tree_state_without_removing_files() {
    let mut h = Harness::new();
    h.q.tree_default_open = true;
    let p = h.file("keep.hy", "kept");
    click_explorer_control(&mut h, "-");
    assert!(!h.q.tree_default_open);
    assert!(h.q.tree_revision > 0);
    assert_eq!(fs::read_to_string(p).unwrap(), "kept");
}
#[test]
fn explorer_files_and_symbols_views_switch_without_changing_build_target() {
    let mut h = Harness::new();
    let p = h.file("app.hyproj", "name='App'");
    h.q.add_project(p.clone());
    h.click("Symbols");
    assert!(h.q.explorer_view == ExplorerView::Outline);
    h.click("Files");
    assert!(h.q.explorer_view == ExplorerView::Files);
    assert_eq!(h.q.selected_project(), Some(p.canonicalize().unwrap()));
}
#[test]
fn explorer_symbols_empty_state_explains_how_to_add_a_project() {
    let mut h = Harness::new();
    h.click("Symbols");
    let out = h.draw(vec![]);
    assert!(has_text(&out, "Open a .hyproj"));
    assert!(h.q.settings.projects.is_empty());
}
fn tree_frame(
    q: &mut Quark,
    ctx: &egui::Context,
    root: &Path,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 5000.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| q.tree_ui(ui, root, 0));
        },
    )
}
#[test]
fn explorer_show_more_reveals_next_page_without_losing_prior_entries() {
    let mut h = Harness::new();
    let root = h.dir.path().to_path_buf();
    let entries = (0..105)
        .map(|i| workspace::Entry {
            path: root.join(format!("file-{i:03}.hy")),
            directory: false,
        })
        .collect();
    h.q.workspace.directories.insert(root.clone(), entries);
    let mut pos = None;
    for _ in 0..3 {
        let out = tree_frame(&mut h.q, &h.ctx, &root, vec![]);
        assert!(!has_text(&out, "file-104.hy"));
        for s in out.shapes {
            if let egui::epaint::Shape::Text(t) = s.shape {
                if t.galley.text() == "Show more (5 remaining)" {
                    pos = Some(t.pos + t.galley.size() * 0.5);
                }
            }
        }
    }
    let pos = pos.unwrap();
    for pressed in [true, false] {
        tree_frame(
            &mut h.q,
            &h.ctx,
            &root,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let out = tree_frame(&mut h.q, &h.ctx, &root, vec![]);
    assert!(has_text(&out, "file-000.hy"));
    assert!(has_text(&out, "file-104.hy"));
    assert!(!has_text(&out, "Show more"));
}
#[test]
fn explorer_page_boundary_and_missing_folder_do_not_panic() {
    let mut h = Harness::new();
    let root = h.dir.path().to_path_buf();
    h.q.workspace.directories.insert(root.clone(), vec![]);
    let out = tree_frame(&mut h.q, &h.ctx, &root, vec![]);
    assert!(!has_text(&out, "Show more"));
    let out = tree_frame(&mut h.q, &h.ctx, &root.join("missing"), vec![]);
    assert!(has_text(&out, "Folder unavailable"));
}
#[test]
fn exit_save_conflict_keeps_window_open_and_preserves_both_versions() {
    let mut h = Harness::new();
    let p = h.file("a.hy", "old");
    h.q.open(p.clone());
    h.q.docs[0].text = "mine".into();
    fs::write(&p, "external").unwrap();
    h.q.request_exit(&h.ctx);
    let out = h.click("Save all & close");
    assert!(!has_command(&out, egui::ViewportCommand::Close));
    assert!(!h.q.allow_exit);
    assert_eq!(h.q.pending_conflict, Some(0));
    assert_eq!(h.q.docs[0].text, "mine");
    assert_eq!(fs::read_to_string(p).unwrap(), "external");
}
#[test]
fn workspace_theme_scope_stays_stable_when_build_target_changes() {
    let mut h = Harness::new();
    let first = h.dir.path().join("one");
    let second = h.dir.path().join("two");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let a = first.join("a.hyproj");
    let b = second.join("b.hyproj");
    fs::write(&a, "id='a'").unwrap();
    fs::write(&b, "id='b'").unwrap();
    h.q.add_project(a);
    h.q.enable_theme("dawn", true);
    h.q.add_project(b);
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Dawn);
    h.q.settings.command_folder = Some(h.dir.path().join("terminal-only"));
    assert_eq!(h.q.effective_theme().preset, theme::Preset::Dawn);
    assert_eq!(h.q.workspace_root(), Some(first.canonicalize().unwrap()));
}
#[test]
fn workspace_extension_scope_does_not_follow_terminal_directory_override() {
    let mut h = Harness::new();
    let id = extension_fixture(&mut h);
    let root = h.dir.path().to_path_buf();
    h.q.settings.folders = vec![root.clone()];
    h.q.set_extension_enabled(&id, false, true);
    h.q.settings.command_folder = Some(root.join("elsewhere"));
    assert!(!h.q.extension_enabled(&id));
    assert_eq!(h.q.workspace_root(), Some(root.canonicalize().unwrap()));
}


#[test]
fn visual_designer_renders_toolbox_and_properties_and_saves() {
    let mut h = Harness::new();
    let layout_path = h.dir.path().join("Main.quarkui");
    h.q.designer = Some(designer::Designer::create(layout_path.clone()).unwrap());
    h.q.designer_active = true;
    h.q.active = None;
    let screen = h.draw(vec![]);
    assert!(has_text(&screen, "UI Designer"));
    assert!(has_text(&screen, "Toolbox"));
    assert!(has_text(&screen, "Properties"));
    h.q.designer.as_mut().unwrap().add(designer::Kind::Button, 30.0, 44.0).unwrap();
    assert!(h.q.designer.as_ref().unwrap().dirty);
    assert!(h.q.save_all());
    assert!(!h.q.designer.as_ref().unwrap().dirty);
    h.q.close_designer();
    assert!(h.q.designer.is_none());
    h.q.open_designer(layout_path);
    assert_eq!(h.q.designer.as_ref().unwrap().layout.controls.len(), 1);
}

#[test]
fn dirty_visual_layout_requires_confirmation_before_close() {
    let mut h = Harness::new();
    let path = h.dir.path().join("Form.quarkui");
    h.q.designer = Some(designer::Designer::create(path).unwrap());
    h.q.designer_active = true;
    h.q.designer.as_mut().unwrap().add(designer::Kind::Label, 10.0, 10.0).unwrap();
    h.q.close_designer();
    assert!(h.q.pending_designer_close);
    assert!(h.q.designer.is_some());
    assert!(h.q.designer.as_ref().unwrap().dirty);
    assert!(!h.q.save_active() || h.q.designer.as_ref().is_some_and(|d| !d.dirty));
}

#[test]
fn start_window_exposes_create_and_open_project_actions() {
    let mut h = Harness::new();
    h.q.show_welcome();
    let screen = h.draw(vec![]);
    assert!(has_text(&screen, "Start Window"));
    assert!(has_text(&screen, "Create a new project"));
    assert!(has_text(&screen, "Recent projects"));
    h.q.action(28, &h.ctx);
    assert!(h.q.show_new_project);
}

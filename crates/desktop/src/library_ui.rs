// SPDX-License-Identifier: GPL-3.0-or-later
//! Disk access and PGN restoration belong to one ordered worker, never the UI thread.
use crate::{
    App, Message,
    analysis_cache::{Key, Result as Analysis},
    import,
};
use chess_library::Library;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

type SavedImport = (import::ImportedPgn, Vec<i64>);
type SavedRows = Vec<(usize, String)>;
type Snapshot = (Vec<SavedImport>, Option<(i64, usize)>);
enum Command {
    Import(u64, import::ImportedPgn),
    Load(i64, String),
    Save(i64, String, usize, String),
    Session(Option<i64>, usize),
    Quit,
}
enum Event {
    Ready(Result<Snapshot, String>),
    Imported(u64, Result<SavedImport, String>),
    Loaded(i64, String, Result<SavedRows, String>),
    Error(String),
}
pub(super) struct Service {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Event>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Service {
    fn spawn(path: PathBuf) -> Result<Self, String> {
        let (commands, input) = mpsc::channel();
        let (output, events) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("chess-library".into())
            .spawn(move || {
                let opened = (|| {
                    std::fs::create_dir_all(path.parent().ok_or("Invalid library path")?)
                        .map_err(|e| e.to_string())?;
                    let db = Library::open(&path)?;
                    let mut loaded = Vec::new();
                    for stored in db.list_imports()? {
                        let games = chess_core::pgn::parse_pgn(stored.pgn.as_bytes())?;
                        if games.len() != stored.game_ids.len() {
                            return Err("Library game count does not match its PGN".into());
                        }
                        loaded.push((
                            import::ImportedPgn {
                                games: Arc::new(games),
                                source: stored.source,
                                text: stored.pgn,
                                library_ids: stored.game_ids.clone(),
                            },
                            stored.game_ids,
                        ));
                    }
                    let session = db.session()?;
                    Ok((db, loaded, session))
                })();
                let mut db = match opened {
                    Ok((db, loaded, session)) => {
                        let _ = output.send(Event::Ready(Ok((loaded, session))));
                        db
                    }
                    Err(error) => {
                        let _ = output.send(Event::Ready(Err(error)));
                        return;
                    }
                };
                while let Ok(command) = input.recv() {
                    let result = match command {
                        Command::Import(id, mut loaded) => {
                            let result = db
                                .import(&loaded.source, &loaded.text, loaded.games.len())
                                .map(|ids| {
                                    loaded.library_ids = ids.clone();
                                    (loaded, ids)
                                });
                            let _ = output.send(Event::Imported(id, result));
                            Ok(())
                        }
                        Command::Load(id, signature) => {
                            let rows = db.load_analyses(id, &signature);
                            let _ = output.send(Event::Loaded(id, signature, rows));
                            Ok(())
                        }
                        Command::Save(id, signature, ply, payload) => {
                            db.save_analysis(id, &signature, ply, &payload)
                        }
                        Command::Session(id, ply) => db.save_session(id, ply),
                        Command::Quit => break,
                    };
                    if let Err(error) = result {
                        let _ = output.send(Event::Error(error));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            commands,
            events,
            worker: Some(worker),
        })
    }
    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "Library worker is unavailable; changes are not saved".into())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredAnalysis {
    fen: String,
    root: String,
    moves: Vec<String>,
    depth: Option<u32>,
    cp: Option<i32>,
    mate: Option<i32>,
    pv: Vec<String>,
    best: Option<String>,
}
fn encode(key: &Key, result: &Analysis) -> Result<String, String> {
    if result.info.bound != engine::Bound::Exact {
        return Err("A score bound cannot be persisted as exact".into());
    }
    let (cp, mate) = match result.info.score {
        Some(engine::Score::Cp(n)) => (Some(n), None),
        Some(engine::Score::Mate(n)) => (None, Some(n)),
        None => return Err("No completed score".into()),
    };
    serde_json::to_string(&StoredAnalysis {
        fen: key.fen.clone(),
        root: key.position.fen.clone(),
        moves: key.position.moves.clone(),
        depth: result.info.depth,
        cp,
        mate,
        pv: result.info.pv.clone(),
        best: result.best_move.clone(),
    })
    .map_err(|e| e.to_string())
}
fn decode(key: &Key, payload: &str) -> Result<Analysis, String> {
    let saved: StoredAnalysis =
        serde_json::from_str(payload).map_err(|e| format!("Invalid saved analysis: {e}"))?;
    if saved.fen != key.fen
        || saved.root != key.position.fen
        || saved.moves != key.position.moves
        || saved.pv.len() > 256
    {
        return Err("Saved analysis does not match the game's history".into());
    }
    let score = match (saved.cp, saved.mate) {
        (Some(n), None) => engine::Score::Cp(n),
        (None, Some(n)) => engine::Score::Mate(n),
        _ => return Err("Invalid saved score".into()),
    };
    Ok(Analysis {
        info: engine::Info {
            depth: saved.depth,
            score: Some(score),
            bound: engine::Bound::Exact,
            pv: saved.pv,
        },
        best_move: saved.best,
    })
}
fn signature(key: &Key) -> String {
    format!(
        "history-exact-v1|{}|{}|{:?}",
        key.engine_path, key.engine_identity, key.limit
    )
}
impl App {
    pub fn new() -> Self {
        let mut app = Self::default();
        let path = directories::ProjectDirs::from("org", "chess-coach", "chess-coach")
            .map(|dirs| dirs.data_local_dir().join("library.sqlite3"));
        match path
            .ok_or_else(|| "Cannot locate the library directory".to_string())
            .and_then(Service::spawn)
        {
            Ok(service) => {
                app.library = Some(service);
                app.analysis_due = None;
                app.notice = "Opening local game library…".into();
            }
            Err(error) => app.library_error = error,
        }
        app
    }
    pub(super) fn save_import(&mut self, id: u64, loaded: import::ImportedPgn) -> bool {
        let Some(library) = &self.library else {
            return false;
        };
        if let Err(error) = library.send(Command::Import(id, loaded)) {
            self.library_error = error.clone();
            let _ = self.update(Message::PgnLoaded(id, Err(error)));
        }
        true
    }
    pub(super) fn save_cursor(&mut self) {
        if self.review.is_modified() {
            return;
        }
        if let Some(library) = &self.library
            && let Err(error) =
                library.send(Command::Session(self.review.library_id(), self.game.ply()))
        {
            self.library_error = error;
        }
    }
    pub(super) fn load_saved_analysis(&mut self) {
        self.library_pending = None;
        let Some(id) = self.review.library_id() else {
            return;
        };
        let Ok(key) = self.analysis_key() else { return };
        let Some(library) = &self.library else { return };
        let signature = signature(&key);
        match library.send(Command::Load(id, signature.clone())) {
            Ok(()) => {
                self.library_pending = Some((id, signature));
                self.analysis_due = None;
            }
            Err(error) => self.library_error = error,
        }
    }
    pub(super) fn save_analysis_result(&mut self, key: &Key, result: &Analysis) {
        let Some(id) = self.review.library_id() else {
            return;
        };
        let Some(ply) = self.game_analysis.as_ref().and_then(|b| b.ply_for(key)) else {
            return;
        };
        let Some(library) = &self.library else { return };
        if let Ok(payload) = encode(key, result)
            && let Err(error) = library.send(Command::Save(id, signature(key), ply, payload))
        {
            self.library_error = error;
        }
    }
    pub(super) fn poll_library(&mut self) {
        while let Some(event) = self.library.as_ref().and_then(|s| s.events.try_recv().ok()) {
            match event {
                Event::Ready(Ok((imports, session))) => {
                    let restore = self.game.revision() == 0 && !self.review.import_open;
                    for (loaded, _) in imports {
                        self.review.append(loaded);
                    }
                    if restore
                        && let Some((id, ply)) = session
                        && let Some(index) = self.review.index_for_id(id)
                    {
                        self.select_library_game(index, ply);
                    }
                    if self.game_analysis.is_none() {
                        self.schedule_analysis();
                    }
                    self.notice = "Local game library ready".into();
                }
                Event::Ready(Err(error)) | Event::Error(error) => {
                    self.library_error = format!("Library: {error}");
                    if self.library_pending.take().is_some() {
                        self.schedule_analysis();
                    }
                    if self.game_analysis.is_none() {
                        self.schedule_analysis();
                    }
                }
                Event::Imported(id, result) => {
                    let result = result.map(|(loaded, _)| Some(loaded));
                    let _ = self.update(Message::PgnLoaded(id, result));
                }
                Event::Loaded(id, stored_signature, rows) => {
                    if self.library_pending.as_ref() != Some(&(id, stored_signature.clone())) {
                        continue;
                    }
                    self.library_pending = None;
                    if self.review.library_id() != Some(id) {
                        continue;
                    }
                    if self
                        .analysis_key()
                        .ok()
                        .is_none_or(|key| signature(&key) != stored_signature)
                    {
                        self.load_saved_analysis();
                        continue;
                    }
                    match rows {
                        Ok(rows) => {
                            for (ply, payload) in rows {
                                if let Some(key) = self
                                    .game_analysis
                                    .as_ref()
                                    .and_then(|batch| batch.key_at(ply))
                                {
                                    match decode(&key, &payload) {
                                        Ok(result) => {
                                            self.game_analysis
                                                .as_mut()
                                                .unwrap()
                                                .record(&key, Some(result.clone()));
                                            self.analysis_cache.insert(key, result);
                                        }
                                        Err(error) => self.library_error = error,
                                    }
                                }
                            }
                        }
                        Err(error) => self.library_error = error,
                    }
                    self.schedule_analysis();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_PGN: &str = "[White \"First\"]\n\n1. e4 e5 2. Nf3 Nc6 *";
    const SECOND_PGN: &str = "[White \"Second\"]\n\n1. d4 d5 *";

    fn app_with_library(path: &std::path::Path) -> App {
        App {
            library: Some(Service::spawn(path.to_path_buf()).unwrap()),
            engine_path: "/nonexistent/chess-library-integration-fixture".into(),
            auto_analysis: false,
            analysis_due: None,
            ..App::default()
        }
    }

    fn send(app: &mut App, message: Message) {
        let _ = app.update(message);
    }

    fn poll_until(app: &mut App, condition: impl Fn(&App) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            send(app, Message::Poll);
            if condition(app) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Timed out waiting for the library: notice={:?}, error={:?}, pending={:?}",
                app.notice,
                app.library_error,
                app.library_pending
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn import_pasted(app: &mut App, id: u64, text: &str) {
        send(app, Message::OpenImport);
        // Establish the same pending request as the UI, then deliver parsed
        // content without asking an iced runtime to execute its Task.
        send(app, Message::ImportPasted);
        let loaded = import::ImportedPgn {
            games: Arc::new(chess_core::pgn::parse_pgn(text.as_bytes()).unwrap()),
            source: "Fixture".into(),
            text: text.into(),
            library_ids: Vec::new(),
        };
        send(app, Message::PgnLoaded(id, Ok(Some(loaded))));
        poll_until(app, |app| {
            !app.review.import_open && app.library_pending.is_none()
        });
        assert!(app.library_error.is_empty(), "{}", app.library_error);
    }

    fn complete_score(app: &mut App, cp: i32, best_move: &str) -> Key {
        let key = app.analysis_key().unwrap();
        app.next_request += 1;
        let id = app.next_request;
        app.active = Some((id, app.game.revision()));
        app.active_key = Some(key.clone());
        app.analyzing = true;
        app.receive(engine::Event::Info {
            id,
            info: engine::Info {
                depth: Some(16),
                score: Some(engine::Score::Cp(cp)),
                pv: vec![best_move.into()],
                ..Default::default()
            },
        });
        app.receive(engine::Event::BestMove {
            id,
            best_move: Some(best_move.into()),
        });
        assert_eq!(app.info.score, Some(engine::Score::Cp(cp)));
        assert!(app.active.is_none());
        key
    }

    fn assert_no_search(app: &App) {
        assert!(!app.auto_analysis && !app.analyzing);
        assert!(app.active.is_none() && app.background.is_none());
        assert!(app.analysis_due.is_none());
    }

    #[test]
    fn library_round_trip_restores_cursor_scores_and_keeps_distinct_imports() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| app.notice == "Local game library ready");
        import_pasted(&mut app, 1, FIRST_PGN);
        let first_id = app.review.library_id().unwrap();
        let initial_key = complete_score(&mut app, 72, "e2e4");
        send(&mut app, Message::Seek(2));
        let cursor_key = complete_score(&mut app, 118, "g1f3");
        let cursor_fen = app.game.fen();
        assert_eq!(app.game.ply(), 2);
        assert_no_search(&app);
        // Service::drop joins its ordered worker after queued writes finish.
        drop(app);

        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| {
            app.review.library_id() == Some(first_id)
                && app.game.ply() == 2
                && app.library_pending.is_none()
        });
        assert!(app.library_error.is_empty(), "{}", app.library_error);
        assert_eq!(app.game.fen(), cursor_fen);
        assert_eq!(app.game.line().len(), 4);
        assert_eq!(app.game.line()[0].uci, "e2e4");
        let batch = app.game_analysis.as_ref().unwrap();
        // Each restored final score also completes its quick-pass work unit.
        assert_eq!(batch.progress(), (4, 0, 10));
        assert_eq!(
            batch.get(&initial_key).unwrap().info.score,
            Some(engine::Score::Cp(72))
        );
        let restored = batch.get(&cursor_key).unwrap();
        assert_eq!(restored.info.score, Some(engine::Score::Cp(118)));
        assert_eq!(restored.info.depth, Some(16));
        assert_eq!(restored.best_move.as_deref(), Some("g1f3"));
        assert_eq!(app.info.score, Some(engine::Score::Cp(118)));
        assert_eq!(app.next_request, 0);
        assert_no_search(&app);

        import_pasted(&mut app, 1, SECOND_PGN);
        let second_id = app.review.library_id().unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(app.review.index_for_id(first_id), Some(0));
        assert_eq!(app.review.index_for_id(second_id), Some(1));
        assert_eq!(app.game.line()[0].uci, "d2d4");

        import_pasted(&mut app, 2, FIRST_PGN);
        assert_eq!(app.review.library_id(), Some(first_id));
        assert_eq!(app.info.score, Some(engine::Score::Cp(72)));
        app.notice.clear();
        app.select_library_game(2, 0);
        assert_eq!(app.notice, "Imported game is unavailable");
        assert_no_search(&app);
        drop(app);

        let db = Library::open(&path).unwrap();
        let imports = db.list_imports().unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].game_ids, vec![first_id]);
        assert_eq!(imports[1].game_ids, vec![second_id]);
        assert_eq!(imports[0].pgn, FIRST_PGN);
        assert_eq!(imports[1].pgn, SECOND_PGN);
    }

    #[test]
    fn corrupt_saved_score_reports_error_without_losing_the_import() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| app.notice == "Local game library ready");
        import_pasted(&mut app, 1, FIRST_PGN);
        let id = app.review.library_id().unwrap();
        let key = complete_score(&mut app, 33, "e2e4");
        drop(app);
        let mut db = Library::open(&path).unwrap();
        db.save_analysis(id, &signature(&key), 0, "corrupt payload")
            .unwrap();
        drop(db);

        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| {
            app.library_pending.is_none() && !app.library_error.is_empty()
        });
        assert!(app.library_error.contains("Invalid saved analysis"));
        assert_eq!(app.review.library_id(), Some(id));
        assert_eq!(app.game.ply(), 0);
        assert_eq!(app.game.line().len(), 4);
        assert!(app.info.score.is_none());
        assert_eq!(app.game_analysis.as_ref().unwrap().progress(), (0, 0, 10));
        assert_eq!(app.next_request, 0);
        assert_no_search(&app);
    }

    #[test]
    fn shared_cache_scores_are_saved_for_each_imported_game() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| app.notice == "Local game library ready");
        import_pasted(&mut app, 1, FIRST_PGN);
        let first_id = app.review.library_id().unwrap();
        let root_key = complete_score(&mut app, 47, "e2e4");
        send(&mut app, Message::Seek(2));
        let cursor_key = complete_score(&mut app, 88, "g1f3");
        let requests = app.next_request;

        let second_pgn = FIRST_PGN.replace("First", "Same moves, different tags");
        import_pasted(&mut app, 2, &second_pgn);
        let second_id = app.review.library_id().unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(app.game.ply(), 0);
        assert_eq!(app.info.score, Some(engine::Score::Cp(47)));
        send(&mut app, Message::Seek(2));
        assert_eq!(app.info.score, Some(engine::Score::Cp(88)));
        assert_eq!(app.next_request, requests);
        assert_no_search(&app);
        drop(app);

        // The second game's cache hits must have their own durable rows.
        let db = Library::open(&path).unwrap();
        let rows = db
            .load_analyses(second_id, &signature(&cursor_key))
            .unwrap();
        assert_eq!(
            rows.iter().map(|(ply, _)| *ply).collect::<Vec<_>>(),
            vec![0, 2]
        );
        drop(db);

        let mut app = app_with_library(&path);
        poll_until(&mut app, |app| {
            app.review.library_id() == Some(second_id)
                && app.game.ply() == 2
                && app.library_pending.is_none()
        });
        assert!(app.library_error.is_empty(), "{}", app.library_error);
        assert_eq!(app.game.line().len(), 4);
        assert_eq!(app.game.line()[0].uci, "e2e4");
        let batch = app.game_analysis.as_ref().unwrap();
        assert_eq!(batch.progress(), (4, 0, 10));
        let root = batch.get(&root_key).unwrap();
        assert_eq!(root.info.score, Some(engine::Score::Cp(47)));
        assert_eq!(root.info.depth, Some(16));
        assert_eq!(root.best_move.as_deref(), Some("e2e4"));
        let cursor = batch.get(&cursor_key).unwrap();
        assert_eq!(cursor.info.score, Some(engine::Score::Cp(88)));
        assert_eq!(cursor.info.depth, Some(16));
        assert_eq!(cursor.best_move.as_deref(), Some("g1f3"));
        assert_eq!(app.info.score, Some(engine::Score::Cp(88)));
        assert_eq!(app.next_request, 0);
        assert_no_search(&app);
    }

    #[test]
    fn stored_scores_validate_history_and_preserve_mate() {
        let app = App::default();
        let key = Key {
            fen: app.game.fen(),
            position: engine::Position {
                fen: app.game.fen(),
                moves: Vec::new(),
            },
            engine_path: "fixture".into(),
            engine_identity: engine::executable_identity(std::path::Path::new("fixture")),
            options: engine::EngineOptions::default(),
            limit: engine::Limit::TimeMs(1000),
        };
        let result = Analysis {
            info: engine::Info {
                score: Some(engine::Score::Mate(-3)),
                ..Default::default()
            },
            best_move: None,
        };
        let encoded = encode(&key, &result).unwrap();
        assert_eq!(
            decode(&key, &encoded).unwrap().info.score,
            result.info.score
        );
        let mut wrong = key;
        wrong.position.moves.push("g1f3".into());
        assert!(decode(&wrong, &encoded).is_err());
        assert!(decode(&wrong, "corrupt").is_err());
    }
}

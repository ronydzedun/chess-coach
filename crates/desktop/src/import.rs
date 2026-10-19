// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, read-only PGN loading, separated from widgets and GUI state.
use chess_core::pgn::{MAX_PGN_BYTES, PgnGame, parse_pgn};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ImportedPgn {
    pub games: Arc<Vec<PgnGame>>,
    pub source: String,
}

async fn background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("pgn-import".into())
        .spawn(move || {
            let _ = sender.send(work());
        })
        .map_err(|error| format!("Cannot start PGN import: {error}"))?;
    receiver
        .await
        .map_err(|_| "PGN import worker stopped unexpectedly".to_string())?
}

pub async fn pasted(text: String) -> Result<ImportedPgn, String> {
    background(move || {
        Ok(ImportedPgn {
            games: Arc::new(parse_pgn(text.as_bytes())?),
            source: "Pasted PGN".into(),
        })
    })
    .await
}

pub async fn file(path: PathBuf) -> Result<ImportedPgn, String> {
    background(move || read_file(path)).await
}
fn read_file(path: PathBuf) -> Result<ImportedPgn, String> {
    let metadata =
        std::fs::metadata(&path).map_err(|error| format!("Cannot inspect PGN file: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose a regular PGN file".into());
    }
    if metadata.len() > MAX_PGN_BYTES as u64 {
        return Err("PGN file exceeds the 8 MiB limit".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|file| file.take(MAX_PGN_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("Cannot read PGN file: {error}"))?;
    let games = parse_pgn(&bytes)?;
    let source = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(100)
        .collect();
    Ok(ImportedPgn {
        games: Arc::new(games),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "chess-coach-pgn-{}-{}.pgn",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    #[test]
    fn file_import_is_read_only_and_bounded() {
        let file = Fixture::new();
        let bytes = b"[White \"Alice\"]\n[Black \"Bob\"]\n\n1. e4 e5 *";
        std::fs::write(&file.0, bytes).unwrap();
        let loaded = read_file(file.0.clone()).unwrap();
        assert_eq!(loaded.games.len(), 1);
        assert_eq!(loaded.games[0].game.line().len(), 2);
        assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
        std::fs::File::create(&file.0)
            .unwrap()
            .set_len(MAX_PGN_BYTES as u64 + 1)
            .unwrap();
        assert!(read_file(file.0.clone()).unwrap_err().contains("8 MiB"));
    }
    #[test]
    fn bad_file_input_returns_an_error() {
        let file = Fixture::new();
        assert!(read_file(file.0.clone()).is_err());
        std::fs::write(&file.0, [255, 254]).unwrap();
        assert!(read_file(file.0.clone()).is_err());
        assert!(read_file(std::env::temp_dir()).is_err());
    }
}

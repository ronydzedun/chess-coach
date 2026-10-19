// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal deterministic UCI process used by the engine integration tests.
use std::io::{self, BufRead, Write};
fn main() {
    let silent = std::env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("silent");
    let mut position = String::new();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if silent {
            continue;
        }
        match line.as_str() {
            "uci" => println!("id name fixture\nuciok"),
            "isready" => println!("readyok"),
            "quit" => break,
            "stop" => println!("bestmove 0000"),
            _ if line.starts_with("position fen ") => {
                position = line[13..].to_owned();
                if let Some(path) = position.strip_prefix("pid:") {
                    std::fs::write(path, std::process::id().to_string()).unwrap();
                }
            }
            _ if line.starts_with("go ") => {
                println!("info depth 7 score cp 42 pv e2e4 e7e5");
                if position != "slow" && !position.starts_with("pid:") {
                    if position == "flood" {
                        for _ in 0..20000 {
                            println!("info depth 1 score cp 3");
                        }
                    }
                    println!("bestmove e2e4 ponder e7e5");
                }
            }
            _ => {}
        }
        io::stdout().flush().unwrap();
    }
}

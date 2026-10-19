// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal deterministic UCI process used by the engine integration tests.
use std::io::{self, BufRead, Write};
fn main() {
    let executable = std::env::current_exe().unwrap();
    let name = executable.file_name().unwrap().to_string_lossy();
    let mut position = String::new();
    let mut late = false;
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if name.contains("audit") {
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(executable.with_extension("commands"))
                .unwrap();
            writeln!(log, "{line}").unwrap();
        }
        if name.contains("silent") {
            continue;
        }
        match line.as_str() {
            "uci" => {
                println!("id name fixture");
                if name.contains("optionflood") {
                    for _ in 0..20000 {
                        println!("info string handshake diagnostic");
                    }
                }
                if name.contains("malformed") {
                    println!("option name Threads type check default true");
                } else if !name.contains("nooptions") {
                    let (threads, hash) = if name.contains("limited") {
                        (2, 32)
                    } else {
                        (1024, 4096)
                    };
                    println!("option name Threads type spin default 1 min 1 max {threads}");
                    println!("option name Hash type spin default 16 min 1 max {hash}");
                }
                println!("uciok");
            }
            "isready" => {
                if late {
                    println!("info depth 99 score cp -888 pv a7a8q");
                    late = false;
                }
                println!("readyok");
            }
            "quit" => break,
            "stop" => {
                if position == "unresponsive" {
                    continue;
                }
                if position == "late" {
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    println!("info depth 99 score cp -777 pv a7a8q\nbestmove a7a8q");
                    late = true;
                } else {
                    println!("bestmove 0000");
                }
            }
            _ if line.starts_with("position fen ") => {
                position = line[13..].to_owned();
                if let Some(value) = position.strip_prefix("record:") {
                    let (path, behavior) = value.split_once('|').unwrap();
                    std::fs::write(path, std::process::id().to_string()).unwrap();
                    position = behavior.to_owned();
                } else if let Some(path) = position.strip_prefix("pid:") {
                    std::fs::write(path, std::process::id().to_string()).unwrap();
                }
            }
            _ if line.starts_with("go ") => {
                println!("info depth 7 score cp 42 pv e2e4 e7e5");
                if !matches!(position.as_str(), "slow" | "late" | "unresponsive")
                    && !position.starts_with("pid:")
                {
                    if position == "flood" {
                        for _ in 0..20000 {
                            println!("info depth 1 score cp 3");
                        }
                    }
                    println!("bestmove e2e4 ponder e7e5");
                    if position == "exit" {
                        break;
                    }
                    if position == "idleflood" {
                        for _ in 0..20000 {
                            println!("info string idle diagnostic");
                        }
                    }
                }
            }
            _ => {}
        }
        io::stdout().flush().unwrap();
    }
}

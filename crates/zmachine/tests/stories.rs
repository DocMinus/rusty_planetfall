//! End-to-end tests against real story files. Story files are not part of
//! the repository; each test is skipped (with a note) when its file is missing.
//! Run `scripts/fetch-testdata.sh` to get the Czech conformance suite.

use std::path::{Path, PathBuf};

use zmachine::{Event, Progress, ScreenOp, Vm};

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn load(rel: &str) -> Option<Vm> {
    let path = repo_file(rel);
    match std::fs::read(&path) {
        Ok(bytes) => Some(Vm::new(bytes).expect("story loads")),
        Err(_) => {
            eprintln!("skipping: {} not found", path.display());
            None
        }
    }
}

/// Feed commands, collecting lower-window text. Panics on unexpected events.
fn play(vm: &mut Vm, commands: &[&str]) -> String {
    let mut text = String::new();
    let mut commands = commands.iter();
    loop {
        let event = vm.run().expect("vm runs");
        text.push_str(&vm.take_output().lower_text());
        match event {
            Event::NeedLine { .. } => match commands.next() {
                Some(cmd) => {
                    text.push_str(cmd);
                    text.push('\n');
                    vm.provide_line(cmd).unwrap();
                }
                None => return text,
            },
            Event::Quit => return text,
            other => panic!("unexpected event {other:?}"),
        }
    }
}

const PLANETFALL: &str = ".planetfall_historic_source/planetfall.zip";

#[test]
fn planetfall_opening() {
    let Some(mut vm) = load(PLANETFALL) else { return };
    assert!(vm.verify());
    vm.seed_random(1);
    let text = play(&mut vm, &["inventory", "wait", "wait", "wait", "wait", "wait", "wait", "wait"]);
    assert!(text.contains("Release 39 / Serial number 880501"));
    assert!(text.contains("Deck Nine"));
    assert!(text.contains("A Patrol-issue self-contained multi-purpose scrub brush"));
    assert!(text.contains("A massive explosion rocks the ship."));

    let text = play(&mut vm, &["w", "get into webbing"]);
    assert!(text.contains("Escape Pod"), "{text}");
    // `play` drained the output; the status line is refreshed before each read.
    vm.provide_line("look").unwrap();
    vm.run().unwrap();
    let status = vm.take_output().status.expect("status line updated");
    assert_eq!(status.location, "Escape Pod");
    // Strapping into the webbing scores 3; Planetfall's "moves" global is its clock.
    let Progress::Score { score, moves } = status.progress else { panic!("score game") };
    assert_eq!(score, 3);
    assert!((4900..5100).contains(&moves), "{moves}");
}

#[test]
fn planetfall_save_and_restore() {
    let Some(mut vm) = load(PLANETFALL) else { return };
    vm.seed_random(1);
    play(&mut vm, &["take brush"]);

    // Save.
    vm.provide_line("save").unwrap();
    let Event::Save(data) = vm.run().unwrap() else { panic!("expected save") };
    vm.provide_save_result(true).unwrap();
    assert!(play(&mut vm, &[]).contains("Ok."));

    // Change the world, then restore.
    let text = play(&mut vm, &["drop brush", "inventory"]);
    assert!(text.contains("Dropped.") && !text.contains("scrub brush"), "{text}");
    vm.provide_line("restore").unwrap();
    assert_eq!(vm.run().unwrap(), Event::Restore);
    vm.provide_restore(Some(&data)).unwrap();
    // Restoring returns to a single full-screen window.
    assert!(vm.take_output().ops.contains(&ScreenOp::Split(0)));
    let text = play(&mut vm, &["inventory"]);
    assert!(text.contains("Ok."), "{text}");
    assert!(text.contains("scrub brush"), "brush is back in the inventory: {text}");

    // A corrupt file fails in-game rather than crashing.
    vm.provide_line("restore").unwrap();
    assert_eq!(vm.run().unwrap(), Event::Restore);
    assert!(vm.provide_restore(Some(b"garbage")).is_err());
    assert!(play(&mut vm, &[]).contains("Failed."));
}

#[test]
fn planetfall_restart() {
    let Some(mut vm) = load(PLANETFALL) else { return };
    play(&mut vm, &["take brush"]);
    let text = play(&mut vm, &["restart", "y", "inventory"]);
    assert!(text.contains("Another routine day of drudgery"), "{text}");
    assert!(text.matches("scrub brush").count() >= 1);
}

#[test]
fn planetfall_script_turns_on_transcript() {
    let Some(mut vm) = load(PLANETFALL) else { return };
    play(&mut vm, &[]);
    vm.provide_line("script").unwrap();
    vm.run().unwrap();
    assert!(vm.transcript_on());
    assert!(vm.take_output().transcript.contains("Here begins a transcript"));
    vm.provide_line("unscript").unwrap();
    vm.run().unwrap();
    assert!(!vm.transcript_on());
}

#[test]
fn czech_v3_conformance() {
    let Some(mut vm) = load("testdata/dl/czech/czech.z3") else { return };
    let text = play(&mut vm, &[]);
    assert!(text.contains("Failed: 0"), "{text}");
    assert!(text.contains("Didn't crash: hooray!"));
}

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod headless;
mod screen;
mod settings;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use zmachine::Vm;

const USAGE: &str = "usage: rustyz [--headless] [--dump] [--seed N] [STORY_FILE]

  Without STORY_FILE, the first story file in a stories/ folder is used
  (current directory, or next to the executable).

  (no flags)   open the story in a window
  --headless   play on stdin/stdout instead
  --dump       print the story header and object names, then exit
  --seed N     make the random number generator repeatable";

struct Args {
    headless: bool,
    dump: bool,
    seed: Option<u64>,
    story: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { headless: false, dump: false, seed: None, story: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--dump" => args.dump = true,
            "--seed" => {
                let n = it.next().ok_or("--seed needs a number")?;
                args.seed = Some(n.parse().map_err(|_| format!("bad seed: {n}"))?);
            }
            "-h" | "--help" => return Err(USAGE.into()),
            s if s.starts_with('-') && !s.starts_with("-psn_") => return Err(format!("unknown option {s}\n\n{USAGE}")),
            s if s.starts_with("-psn_") => {} // macOS Finder process serial number
            _ => args.story = Some(arg.into()),
        }
    }
    Ok(args)
}

/// The first story file in a `stories/` folder: in the current directory,
/// next to the executable, or in a macOS bundle's Resources.
fn find_default_story() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let dirs = [PathBuf::from("stories"), exe_dir.join("stories"), exe_dir.join("../Resources/stories")];
    dirs.iter().find_map(|dir| {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
        files.sort();
        files.into_iter().find(|p| is_story_file(p))
    })
}

/// Story files are named all sorts of things (Planetfall's is `.zip`), so
/// look at the version byte instead of the extension.
fn is_story_file(path: &std::path::Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 64];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok()
        && (1..=8).contains(&head[0])
        && &head[..2] != b"PK"
}

pub fn load_story(path: &std::path::Path) -> Result<Vm, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Vm::new(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn dump(vm: &Vm) {
    let info = vm.info();
    println!("Version:   {}", info.version);
    println!("Release:   {} / Serial {}", info.release, info.serial);
    println!("Length:    {} bytes", info.file_length);
    println!("Checksum:  {:#06x} ({})", info.checksum, if vm.verify() { "ok" } else { "MISMATCH" });
    let names = vm.object_names();
    println!("Objects:   {}", names.len());
    for (i, name) in names.iter().enumerate() {
        println!("  {:3}  {name}", i + 1);
    }
}

fn main() -> ExitCode {
    let mut args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::FAILURE;
        }
    };
    if args.story.is_none() {
        args.story = find_default_story();
    }

    if args.headless || args.dump {
        let Some(path) = &args.story else {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        };
        let mut vm = match load_story(path) {
            Ok(vm) => vm,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        if let Some(seed) = args.seed {
            vm.seed_random(seed);
        }
        if args.dump {
            dump(&vm);
            return ExitCode::SUCCESS;
        }
        return match headless::run(vm) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("\n[Interpreter error: {e}]");
                ExitCode::FAILURE
            }
        };
    }

    match ui::run(args.story, args.seed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

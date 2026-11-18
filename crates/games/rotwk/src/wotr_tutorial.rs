use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use mint_sage_ini::living_world::{
    load_files, load_living_world, LivingWorldData, LivingWorldLoad, SUBSYSTEM_LEGEND_PATHS,
};
use mint_sage_ini::subsystem_legend::{read_legend, retail_load_order};
use mint_sage_ini::table::BlockTable;
use mint_sage_ini::{MemorySources, Reader};
use mint_sage_livingworld::stand_in::{catalog_from_object_ini, DefenderWins};
use mint_sage_livingworld::tutorial::{
    autoplay, scripted_setup, Playthrough, Task, Tutorial, TutorialEvent,
};
use mint_sage_livingworld::{Catalog, LivingWorldState};
use mint_vfs::Vfs;

struct Args {
    installs: Vec<PathBuf>,
    seed: u64,
    audio_frames: u32,
    max_frames: u64,
}

const USAGE: &str = "usage: mint-rotwk <install dir> [--install <dir>] --wotr-tutorial \
                     [--seed N] [--audio-frames N] [--max-frames N]";

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args {
        installs: Vec::new(),
        seed: 1,
        audio_frames: 15,
        max_frames: 200_000,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} wants a value"))
        };
        let number = |v: String| v.parse::<u64>().map_err(|_| format!("{arg}: not a number"));
        match arg.as_str() {
            "--wotr-tutorial" => {}
            "--install" => out.installs.push(PathBuf::from(value()?)),
            "--seed" => out.seed = number(value()?)?,
            "--audio-frames" => out.audio_frames = u32::try_from(number(value()?)?).unwrap_or(0),
            "--max-frames" => out.max_frames = number(value()?)?,
            other if !other.starts_with("--") => out.installs.insert(0, PathBuf::from(other)),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    if out.installs.is_empty() {
        return Err(USAGE.to_owned());
    }
    Ok(out)
}

pub(crate) fn load_data(sources: &mut MemorySources) -> LivingWorldLoad {
    const TUTORIAL: &str = "data/ini/campaigns/scenarios/wotrtutorial.inc";
    const INCLUDER: &str = "data/ini/campaigns/mint_wotr_tutorial.ini";
    let load = load_living_world(sources);
    let reached = load.data.campaigns.iter().any(|c| !c.tutorials.is_empty());
    if reached || !sources.paths().any(|p| p == TUTORIAL) {
        return load;
    }
    sources.insert(
        INCLUDER,
        b"#include \"Scenarios\\WOTRTutorial.inc\"\n".to_vec(),
    );
    let table = BlockTable::corpus_path();
    let reader = Reader::new(sources);
    let Some(legend) = SUBSYSTEM_LEGEND_PATHS
        .iter()
        .find_map(|path| reader.read(path, &table).ok())
    else {
        return load;
    };
    let available: BTreeSet<String> = sources
        .paths()
        .filter(|p| p.ends_with(".ini"))
        .map(str::to_owned)
        .collect();
    let mut order = retail_load_order(&read_legend(&legend), &available);
    order.push(INCLUDER.to_owned());
    load_files(sources, &order)
}

fn load(vfs: &Vfs) -> (LivingWorldData, Catalog) {
    let mut sources = MemorySources::new();
    let mut object_texts = Vec::new();
    let paths: Vec<String> = vfs
        .paths_under("data/ini/")
        .map(|p| p.as_str().to_owned())
        .collect();
    for path in paths {
        let Ok(bytes) = vfs.read(&path) else { continue };
        if path.contains("/object/") || path.contains("/gamedata") || path.ends_with("/rank.ini") {
            object_texts.push(String::from_utf8_lossy(&bytes).into_owned());
        }
        sources.insert(&path, bytes);
    }
    let load = load_data(&mut sources);
    println!(
        "wotr-tutorial: Living World load errors {}",
        load.errors.len()
    );
    let catalog = catalog_from_object_ini(object_texts.iter().map(String::as_str), &load.data);
    (load.data, catalog)
}

fn describe(task: &Task) -> String {
    format!("{task:?}")
}

fn print_events(events: &[TutorialEvent]) {
    for event in events {
        match event {
            TutorialEvent::SessionStarted {
                turn,
                phase,
                index,
                audio,
                text,
                clear_text,
            } => println!(
                "wotr-tutorial: turn {turn} {phase:?} session {index} audio {} text {} clear {clear_text}",
                audio.as_deref().unwrap_or("-"),
                text.as_deref().unwrap_or("-"),
            ),
            TutorialEvent::TaskBegan { task, text } => println!(
                "wotr-tutorial:   task {} hint {}",
                describe(task),
                text.as_deref().unwrap_or("-")
            ),
            TutorialEvent::ScriptedMove {
                army,
                region,
                result,
            } => println!("wotr-tutorial:   scripted move {army} -> region {} {result:?}", region.0),
            TutorialEvent::CompletionPopup { title, text } => println!(
                "wotr-tutorial: EndTutorial popup {title} / {text}; the headless run closes it \
                 automatically in the next update"
            ),
            TutorialEvent::Ended => {
                println!("wotr-tutorial: popup closed, the session ends back to the shell");
            }
            TutorialEvent::SessionDone { .. } => {}
        }
    }
}

fn play(data: &LivingWorldData, catalog: &Catalog, args: &Args) -> Result<Playthrough, String> {
    let campaign = data
        .campaigns
        .iter()
        .find(|c| !c.tutorials.is_empty())
        .map(|c| c.name.clone())
        .ok_or(
            "no campaign with a Tutorial block; mount an install that includes wotrtutorial.inc",
        )?;
    let setup = scripted_setup(data, &campaign, args.seed).map_err(|e| e.to_string())?;
    let mut state = LivingWorldState::start(data, catalog, &setup)
        .map_err(|e| format!("setup refused: {e}"))?;
    let mut tutorial = Tutorial::new(data, &state).map_err(|e| e.to_string())?;
    let audio_frames = args.audio_frames;
    let run = autoplay(
        &mut tutorial,
        &mut state,
        Some(&mut DefenderWins),
        &mut |_| audio_frames,
        args.max_frames,
    );
    if !run.finished {
        println!(
            "wotr-tutorial: stopped at turn {} {:?} waiting on {:?}",
            state.turn,
            state.phase,
            tutorial.active_task(&state)
        );
        println!("wotr-tutorial: first refusal {:?}", run.refused.first());
    }
    Ok(run)
}

pub(crate) fn command(args: &[String]) -> ExitCode {
    let args = match parse(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let Some((primary, extras)) = args.installs.split_first() else {
        return ExitCode::FAILURE;
    };
    let vfs = crate::mount_primary_last(primary, extras);
    let (data, catalog) = load(&vfs);
    let mut runs = Vec::new();
    for pass in 1..=2 {
        match play(&data, &catalog, &args) {
            Ok(run) => {
                if pass == 1 {
                    print_events(&run.tutorial);
                }
                println!(
                    "wotr-tutorial: pass {pass}: {} frames, reached EndTutorial {}, {} boundaries, {} refused",
                    run.frames,
                    run.finished,
                    run.hashes.len(),
                    run.refused.len()
                );
                let list: Vec<String> = run.hashes.iter().map(|h| format!("{h:016x}")).collect();
                println!("wotr-tutorial: pass {pass} hashes {}", list.join(" "));
                runs.push(run);
            }
            Err(message) => {
                eprintln!("error: {message}");
                return ExitCode::FAILURE;
            }
        }
    }
    let same = matches!(runs.as_slice(), [a, b] if a.hashes == b.hashes && a.frames == b.frames);
    println!("wotr-tutorial: both passes identical {same}");
    if same && runs.iter().all(|r| r.finished) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

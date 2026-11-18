use std::process::ExitCode;

use mint_sage_fx_viewer::{cli, load, run_headless, run_window, Error};

fn main() -> ExitCode {
    let options = match cli::parse_arguments() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };
    if options.help {
        println!("{}\n\n{}", cli::USAGE, cli::KEYS);
        return ExitCode::SUCCESS;
    }

    let loaded = match load(&options.installs) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            if matches!(error, Error::NoInstallGiven) {
                eprintln!("\n{}", cli::USAGE);
            }
            return ExitCode::FAILURE;
        }
    };

    if options.list.is_some() || options.tree.is_some() {
        eprintln!("{}", loaded.summary);
    }

    if let Some((category, filter)) = options.list.as_ref() {
        let roots = loaded.tree.roots(*category, filter);
        for root in &roots {
            if let Some(node) = loaded.tree.node(*root) {
                println!("{}", node.line());
            }
        }
        println!("{} root(s) in {}", roots.len(), category.label());
        return ExitCode::SUCCESS;
    }

    if let Some(name) = options.tree.as_ref() {
        let Some(root) = loaded.tree.find_root(name) else {
            eprintln!("no FXList or particle system named {name}");
            return ExitCode::FAILURE;
        };
        print!("{}", loaded.tree.render(root));
        return ExitCode::SUCCESS;
    }

    if let Some(name) = options.play.clone() {
        if let Err(error) = run_headless(loaded, &options, &name) {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    if let Err(error) = run_window(loaded, &options) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

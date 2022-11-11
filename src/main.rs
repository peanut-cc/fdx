use std::{env, path::{Path, PathBuf}};

use anyhow::{Result,anyhow, Context};
use error::print_error;
use exit_codes::ExitCode;
use normpath::PathExt;
mod dir_entry;

mod app;
mod error;
mod exit_codes;
mod filesystem;
mod config;
mod walk;
mod output;

fn main() {
    let result = run();
    match result {
        Ok(exit_code) => {
            exit_code.exit();
        }
        Err(err) => {
            eprintln!("[fd error]: {:#}", err);
            ExitCode::GeneralError.exit();
        }
    }
}

fn run() -> Result<ExitCode> {
    let matches = app::build_app().get_matches_from(env::args_os());
    set_working_dir(&matches);
    // let pattern = extract_search_pattern(&matches)?;
    let search_paths = extract_search_paths(&matches)?;
    println!("{:?}", search_paths);
    walk::scan(&search_paths)
}

fn set_working_dir(matches: &clap::ArgMatches) -> Result<()> {
    if let Some(base_directory) = matches.value_of_os("base-directory") {
        let base_directory =Path::new(base_directory);
        if !filesystem::is_existing_directory(base_directory) {
            return Err(anyhow!(
                "The '--base-directory' path '{}' is not a directory.",
                base_directory.to_string_lossy()
            ));
        }
        env::set_current_dir(base_directory).with_context(|| {
            format!(
                "Could not set '{}' as the current working directory",
                base_directory.to_string_lossy()
            )
        })?;
    }
    Ok(())
}

fn extract_search_pattern(matches: &clap::ArgMatches) -> Result<&'_ str> {
    let pattern = matches
        .value_of_os("pattern")
        .map(|p| {
            p.to_str()
                .ok_or_else(|| anyhow!("The search pattern includes invalid UTF-8 sequences."))
        })
        .transpose()?
        .unwrap_or("");
    Ok(pattern)
}

fn extract_search_paths(matches: &clap::ArgMatches) -> Result<Vec<PathBuf>> {
    let parameter_paths = matches
        .values_of_os("path")
        .or_else(|| matches.values_of_os("search-path"));

    let mut search_paths = match parameter_paths {
        Some(paths) => paths
            .filter_map(|path| {
                let path_buffer = PathBuf::from(path);
                if filesystem::is_existing_directory(&path_buffer) {
                    Some(path_buffer)
                } else {
                    print_error(format!(
                        "Search path '{}' is not a directory.",
                        path_buffer.to_string_lossy(),
                    ));
                    None
                }
            })
            .collect(),
        None => {
            let current_directory = Path::new(".");
            ensure_current_directory_exists(current_directory)?;
            vec![current_directory.to_path_buf()]
        }
    };
    if search_paths.is_empty() {
        return Err(anyhow!("No valid search paths given."));
    };
    if matches.is_present("absolute-path") {
        update_to_absolute_paths(&mut search_paths);
    }
    Ok(search_paths)

}

fn update_to_absolute_paths(search_paths: &mut [PathBuf]) {
    for buffer in search_paths.iter_mut() {
        *buffer = filesystem::absolute_path(buffer.normalize().unwrap().as_path()).unwrap();
    }
}

fn ensure_current_directory_exists(current_directory: &Path) -> Result<()> {
    if filesystem::is_existing_directory(current_directory) {
        Ok(())
    } else {
        Err(anyhow!(
            "Could not retrieve current directory (has it been deleted?)."
        ))
    }
}
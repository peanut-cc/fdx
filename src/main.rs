use std::{env, path::{Path, PathBuf}, sync::Arc};

use anyhow::{Result,anyhow, Context};
use error::print_error;
use exit_codes::ExitCode;
use globset::GlobBuilder;
use normpath::PathExt;
use regex::bytes::RegexBuilder;
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
    let pattern = extract_search_pattern(&matches)?;
    println!("{}", pattern);
    // ensure_search_pattern_is_not_a_path(&matches, pattern)?;
    let pattern_regex = build_pattern_regex(&matches, pattern)?;
    let re = build_regex(pattern_regex)?;
    let search_paths = extract_search_paths(&matches)?;
    println!("{:?}", search_paths);
    walk::scan(&search_paths, Arc::new(re))
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

/// Detect if the user accidentally supplied a path instead of a search pattern
fn ensure_search_pattern_is_not_a_path(matches: &clap::ArgMatches, pattern: &str) -> Result<()> {
    if !matches.is_present("full-path")
        && pattern.contains(std::path::MAIN_SEPARATOR)
        && Path::new(pattern).is_dir()
    {
        Err(anyhow!(
            "The search pattern '{pattern}' contains a path-separation character ('{sep}') \
             and will not lead to any search results.\n\n\
             If you want to search for all files inside the '{pattern}' directory, use a match-all pattern:\n\n  \
             fd . '{pattern}'\n\n\
             Instead, if you want your pattern to match the full file path, use:\n\n  \
             fd --full-path '{pattern}'",
            pattern = pattern,
            sep = std::path::MAIN_SEPARATOR,
        ))
    } else {
        Ok(())
    }
}

fn build_pattern_regex(matches: &clap::ArgMatches, pattern: &str) -> Result<String> {
    Ok(if matches.is_present("glob") && !pattern.is_empty() {
        let glob = GlobBuilder::new(pattern).literal_separator(true).build()?;
        glob.regex().to_owned()
    } else {
        String::from(pattern)
    })
}

fn build_regex(pattern_regex: String) -> Result<regex::bytes::Regex> {
    RegexBuilder::new(&pattern_regex)
        .case_insensitive(false)
        .dot_matches_new_line(true)
        .build()
        .map_err(|e| {
            anyhow!(
                "{}\n\nNote: You can use the '--fixed-strings' option to search for a \
                 literal string instead of a regular expression. Alternatively, you can \
                 also use the '--glob' option to match on a glob pattern.",
                e.to_string()
            )
        })
}

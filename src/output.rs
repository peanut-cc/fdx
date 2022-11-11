use std::io::{Write, self};

use lscolors::Style;

use crate::{dir_entry::DirEntry, config::Config, error::print_error, exit_codes::ExitCode};


pub fn print_entry<W: Write>(stdout: &mut W, entry: &DirEntry) {
    let r = print_entry_uncolorized(stdout, entry);
    if let Err(e) = r {
        if e.kind() == ::std::io::ErrorKind::BrokenPipe {
            // Exit gracefully in case of a broken pipe (e.g. 'fd ... | head -n 3').
            ExitCode::Success.exit();
        } else {
            print_error(format!("Could not write to output: {}", e));
            ExitCode::GeneralError.exit();
        }
    }
}


// TODO: this function is performance critical and can probably be optimized
fn print_entry_uncolorized_base<W: Write>(
    stdout: &mut W,
    entry: &DirEntry,
) -> io::Result<()> {
    let separator =  "\n";
    let path = entry.stripped_path();

    let mut path_string = path.to_string_lossy();
    // if let Some(ref separator) = config.path_separator {
    //     *path_string.to_mut() = replace_path_separator(&path_string, separator);
    // }
    write!(stdout, "{}", path_string)?;
    print_trailing_slash(stdout, entry, None)?;
    write!(stdout, "{}", separator)
}

#[inline]
fn print_trailing_slash<W: Write>(
    stdout: &mut W,
    entry: &DirEntry,
    style: Option<&Style>,
) -> io::Result<()> {
    if entry.file_type().map_or(false, |ft| ft.is_dir()) {
        write!(
            stdout,
            "{}",
            style
                .map(Style::to_ansi_term_style)
                .unwrap_or_default()
                .paint(std::path::MAIN_SEPARATOR.to_string())
        )?;
    }
    Ok(())
}

#[cfg(unix)]
fn print_entry_uncolorized<W: Write>(
    stdout: &mut W,
    entry: &DirEntry,
) -> io::Result<()> {

    print_entry_uncolorized_base(stdout, entry)
    // if config.interactive_terminal || config.path_separator.is_some() {
    //     // Fall back to the base implementation
    //     print_entry_uncolorized_base(stdout, entry, config)
    // } else {
    //     // Print path as raw bytes, allowing invalid UTF-8 filenames to be passed to other processes
    //     let separator = if config.null_separator { b"\0" } else { b"\n" };
    //     stdout.write_all(entry.stripped_path(config).as_os_str().as_bytes())?;
    //     print_trailing_slash(stdout, entry, config, None)?;
    //     stdout.write_all(separator)
    // }
}

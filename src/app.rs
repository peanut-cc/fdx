use clap::{Command, ColorChoice, crate_version, Arg};




// build_app clap 构建命令行参数用
pub fn build_app() -> Command<'static> {
    let clap_color_choice = ColorChoice::Auto;

    let app = Command::new("fdx")
        .version(crate_version!())
        .color(clap_color_choice)
        .arg(
            Arg::new("glob")
                .long("glob")
                .short('g')
                .overrides_with("glob")
                .help("Glob-based search (default: regular expression)")
                .long_help("Perform a glob-based search instead of a regular expression search."),
        )
        .arg(
            Arg::new("absolute-path")
                .long("absolute-path")
                .short('a')
                .overrides_with("absolute-path")
                .help("Show absolute instead of relative paths")
                .long_help(
                    "Shows the full path starting from the root as opposed to relative paths. \
                     The flag can be overridden with --relative-path.",
                ),
        )
        .arg(
            Arg::new("relative-path")
                .long("relative-path")
                .overrides_with("absolute-path")
                .hide(true)
                .long_help(
                    "Overrides --absolute-path.",
                ),
        )
        .arg(
            Arg::new("pattern")
            .allow_invalid_utf8(true)
            .help(
                "the search pattern (a regular expression, unless '--glob' is used; optional)",
            ).long_help(
                "the search pattern which is either a regular expression (default) or a glob \
                 pattern (if --glob is used). If no pattern has been specified, every entry \
                 is considered a match. If your pattern starts with a dash (-), make sure to \
                 pass '--' first, or it will be considered as a flag (fd -- '-foo').")
        )
        .arg(
            Arg::new("path")
                // .multiple_occurrences(true)
                .allow_invalid_utf8(true)
                .help("the root directory for the filesystem search (optional)")
                .long_help(
                    "The directory where the filesystem search is rooted (optional). If \
                         omitted, search the current working directory.",
                ),
        )
        .arg(
            Arg::new("base-directory")
                .long("base-directory")
                .takes_value(true)
                .value_name("path")
                .number_of_values(1)
                .allow_invalid_utf8(true)
                .hide_short_help(true)
                .help("Change current working directory")
                .long_help(
                    "Change the current working directory of fd to the provided path. This \
                         means that search results will be shown with respect to the given base \
                         path. Note that relative paths which are passed to fd via the positional \
                         <path> argument or the '--search-path' option will also be resolved \
                         relative to this directory.",
                ),
        );
    app
}
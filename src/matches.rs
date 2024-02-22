use std::process;
use clap::{arg, command};
use regex::Regex;

pub struct Flags {
    pub debug: bool,
    pub progress: bool,
    pub timer: bool,
}

pub fn get_arch_matches() -> (String, Flags) {
    let arg_matches = command!()
        .arg_required_else_help(true)
        .arg(arg!([url] "Required url to operate on, including the protocol (so http or https)."))
        .arg(arg!(-d --debug "Turn debugging information on.")
            .required(false))
        .arg(arg!(-p --progress "Show a progress on-liner.")
            .required(false))
        .arg(arg!(-t --timer "Time execution.")
            .required(false))
        .get_matches();

    let base_url = arg_matches.get_one::<String>("url").unwrap();
    check_base_url(base_url);

    let flags = Flags {
        debug: arg_matches.get_flag("debug"),
        progress: arg_matches.get_flag("progress"),
        timer: arg_matches.get_flag("timer"),
    };

    (base_url.to_string(), flags)
}

fn check_base_url(base_url: &String) {
    let regex = Regex::new(r"^https?://[0-9A-Za-z.:/]+$").unwrap();
    if !(regex.is_match(base_url)) {
        println!("{base_url} is not a valid url.");
        process::exit(1);
    }
}



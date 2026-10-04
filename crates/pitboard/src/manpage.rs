//! The man page, with every command set out on it.
//!
//! clap_mangen lists each subcommand as a page of its own, `pitboard-status(1)`, which is
//! how a program that installs a page per command is read. Pitboard installs one page,
//! `pitboard.1`, so a list of references sent `man` to pages that are not there. Its own
//! sections are kept; the commands are written here in their place.

use clap::{Arg, Command};
use roff::{Inline, Roff, bold, italic, roman};
use std::io::Write;

pub fn render(mut cmd: Command, w: &mut dyn Write) -> std::io::Result<()> {
    cmd.build();
    let man = clap_mangen::Man::new(cmd.clone());
    man.render_title(w)?;
    man.render_name_section(w)?;
    man.render_synopsis_section(w)?;
    man.render_description_section(w)?;
    man.render_options_section(w)?;
    commands(&cmd).to_writer(w)?;
    man.render_version_section(w)
}

fn commands(cmd: &Command) -> Roff {
    let mut roff = Roff::new();
    roff.control("SH", ["COMMANDS"]);
    leaves(&mut roff, &mut vec![cmd]);
    roff
}

/// Every command somebody can type, however deep: `pitboard desktop live-usage enable` is
/// written out whole, and `pitboard desktop`, which does nothing by itself, is not listed.
fn leaves<'a>(roff: &mut Roff, path: &mut Vec<&'a Command>) {
    let last = *path.last().expect("a command");
    let nested: Vec<&'a Command> = shown(last).collect();
    if nested.is_empty() {
        command(roff, path);
        return;
    }
    for inner in nested {
        path.push(inner);
        leaves(roff, path);
        path.pop();
    }
}

/// The subcommands somebody types: not hidden ones, and not clap's own `help`.
fn shown(cmd: &Command) -> impl Iterator<Item = &Command> {
    cmd.get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
}

/// One command: how it is typed, what it does, and its arguments. `path` runs from the
/// program to the command, so a nested one is written out whole.
fn command(roff: &mut Roff, path: &[&Command]) {
    let cmd = path.last().expect("a command");
    let name = path
        .iter()
        .map(|c| c.get_name())
        .collect::<Vec<_>>()
        .join(" ");
    let args: Vec<&Arg> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set() && !a.is_global_set() && a.get_id() != "help")
        .collect();

    let mut usage = vec![bold(name)];
    for arg in &args {
        usage.push(roman(" "));
        if arg.is_positional() {
            usage.push(italic(value_name(arg)));
        } else {
            usage.push(roman("["));
            usage.extend(flag(arg));
            usage.push(roman("]"));
        }
    }
    roff.control("TP", []);
    roff.text(usage);
    if let Some(about) = cmd.get_long_about().or_else(|| cmd.get_about()) {
        roff.text([roman(about.to_string())]);
    }
    if args.is_empty() {
        return;
    }
    roff.control("RS", []);
    for arg in args {
        let header = if arg.is_positional() {
            vec![italic(value_name(arg))]
        } else {
            flag(arg)
        };
        let mut body = Vec::new();
        if let Some(help) = arg.get_long_help().or_else(|| arg.get_help()) {
            body.push(help.to_string());
        }
        let possible: Vec<String> = arg
            .get_possible_values()
            .iter()
            .filter(|v| !v.is_hide_set())
            .map(|v| v.get_name().to_string())
            .collect();
        if !possible.is_empty() {
            body.push(format!("[possible values: {}]", possible.join(", ")));
        }
        let defaults: Vec<String> = arg
            .get_default_values()
            .iter()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        if !defaults.is_empty() && arg.get_action().takes_values() {
            body.push(format!("[default: {}]", defaults.join(", ")));
        }
        roff.control("TP", []);
        roff.text(header);
        roff.text([roman(body.join(" "))]);
    }
    roff.control("RE", []);
}

/// `-y, --yes`, or `-n, --lines <LINES>` for one that takes a value.
fn flag(arg: &Arg) -> Vec<Inline> {
    let mut out = Vec::new();
    if let Some(short) = arg.get_short() {
        out.push(bold(format!("-{short}")));
        if arg.get_long().is_some() {
            out.push(roman(", "));
        }
    }
    if let Some(long) = arg.get_long() {
        out.push(bold(format!("--{long}")));
    }
    if arg.get_action().takes_values() {
        out.push(roman(" "));
        out.push(italic(value_name(arg)));
    }
    out
}

fn value_name(arg: &Arg) -> String {
    let name = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map(ToString::to_string)
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
    format!("<{name}>")
}

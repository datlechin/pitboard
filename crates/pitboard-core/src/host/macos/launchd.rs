//! Pitboard's jobs on macOS: LaunchAgents, which run inside the login session so the keychain
//! is unlocked.

use super::super::unix::service::{self, Control};
use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Scheduler;
use crate::schedule::{EVERY_SECONDS, Job, Location, RESTART_AFTER_SECONDS};
use crate::service::Permit;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What launchd calls each job, so a person can find it without Pitboard telling them.
const LABEL: &str = "com.datlechin.pitboard.renew";
const WATCH_LABEL: &str = "com.datlechin.pitboard.watch";

const LAUNCHCTL: &str = "/bin/launchctl";
const LOG: &str = "watch.log";

#[derive(Debug)]
pub(super) struct Launchd {
    control: Arc<dyn Control>,
}

impl Launchd {
    pub(super) fn new(control: Arc<dyn Control>) -> Launchd {
        Launchd { control }
    }

    fn label(job: Job) -> &'static str {
        match job {
            Job::Renew => LABEL,
            Job::Watch => WATCH_LABEL,
        }
    }

    fn agent(ctx: &Context, job: Job) -> PathBuf {
        ctx.home()
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", Self::label(job)))
    }

    fn log_folder(home: &Path) -> PathBuf {
        home.join("Library/Logs/Pitboard")
    }

    /// launchd's session for this user, which `bootstrap` and `bootout` name.
    fn domain() -> String {
        // SAFETY: `getuid` cannot fail and touches no memory of this process.
        format!("gui/{}", unsafe { libc::getuid() })
    }
}

impl Scheduler for Launchd {
    fn location(&self, ctx: &Context, job: Job) -> Location {
        Location::File(Self::agent(ctx, job))
    }

    fn installed(&self, ctx: &Context, job: Job) -> bool {
        Self::agent(ctx, job).is_file()
    }

    fn program(&self, ctx: &Context, job: Job) -> Option<PathBuf> {
        let body = std::fs::read_to_string(Self::agent(ctx, job)).ok()?;
        arguments(&body).into_iter().next().map(PathBuf::from)
    }

    fn options(&self, ctx: &Context, job: Job) -> Vec<String> {
        let Ok(body) = std::fs::read_to_string(Self::agent(ctx, job)) else {
            return Vec::new();
        };
        let arguments = arguments(&body);
        let marked = runs(job);
        match arguments.get(1..=marked.len()) {
            Some(words) if words == marked => arguments[1 + marked.len()..].to_vec(),
            _ => Vec::new(),
        }
    }

    fn environment(&self, ctx: &Context, job: Job) -> Vec<(String, String)> {
        if !self.installed(ctx, job) {
            return Vec::new();
        }
        std::fs::read_to_string(Self::agent(ctx, job))
            .map(|body| given(&body))
            .unwrap_or_default()
    }

    fn put(
        &self,
        ctx: &Context,
        permit: Permit,
        job: Job,
        program: &Path,
        options: &[String],
        environment: &[(String, String)],
    ) -> Result<()> {
        if let Some(refused) = unwritable(program, environment) {
            return Err(refused);
        }
        let path = Self::agent(ctx, job);
        let before = std::fs::read_to_string(&path).ok();
        if job == Job::Watch {
            let folder = Self::log_folder(ctx.home());
            crate::host::fs::create_private_dir(permit, &folder).map_err(|source| {
                Error::HomeUnwritable {
                    path: folder,
                    source,
                }
            })?;
        }
        service::write(
            permit,
            &path,
            &plist(job, program, options, environment, ctx.home()),
        )?;
        // `bootstrap` is launchd's own word for this, and replaces the deprecated `load`.
        let domain = Self::domain();
        let target = path.to_string_lossy();
        let _ = self
            .control
            .run(permit, LAUNCHCTL, &["bootout", &domain, &target]);
        if let Err(refused) =
            self.control
                .start(permit, LAUNCHCTL, &["bootstrap", &domain, &target])
        {
            service::restore(permit, &path, before.as_deref());
            if before.is_some() {
                let _ = self
                    .control
                    .start(permit, LAUNCHCTL, &["bootstrap", &domain, &target]);
            }
            return Err(refused);
        }
        Ok(())
    }

    fn remove(&self, ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
        let path = Self::agent(ctx, job);
        if !path.is_file() {
            return Ok(false);
        }
        let _ = self.control.run(
            permit,
            LAUNCHCTL,
            &["bootout", &Self::domain(), &path.to_string_lossy()],
        );
        service::remove(permit, &path)?;
        if job == Job::Watch {
            let folder = Self::log_folder(ctx.home());
            service::remove(permit, &folder.join(LOG))?;
            // Kept where something else is in it.
            let _ = crate::host::fs::remove_dir(permit, &folder);
        }
        Ok(true)
    }

    /// launchd puts the label of the job it starts in `XPC_SERVICE_NAME`.
    #[allow(
        clippy::disallowed_methods,
        reason = "what launchd started this process as, which no front end passes on"
    )]
    fn started_this_process(&self, job: Job, said: Option<&str>) -> bool {
        said.map(str::to_owned)
            .or_else(|| std::env::var("XPC_SERVICE_NAME").ok())
            .as_deref()
            == Some(Self::label(job))
    }
}

/// What a job's agent runs Pitboard with before its options. launchd names the job it starts,
/// so renewal's runs plain `renew`; the watch service carries its marker, as on every system.
fn runs(job: Job) -> &'static [&'static str] {
    match job {
        Job::Renew => &["renew"],
        Job::Watch => &crate::schedule::WATCH_RUN,
    }
}

/// launchd's own format. Renewal's `RunAtLoad` is off: installing it is not a reason to talk to
/// Anthropic that second, and the first run comes at the first interval. The watch service
/// runs at load, which is at each login too, and launchd starts it again a minute after it
/// fails or crashes: a panic exits 101, and a run an update left behind exits on a state file
/// the new build wrote. Its lines go to a log in `home`'s `Library/Logs`, where Console finds a
/// user agent's.
///
/// The job runs with launchd's environment, not the person's shell, so a variable they set
/// would not reach it. `environment` is what it is given of the installing Pitboard's,
/// `PITBOARD_NO_ARGV` and the proxy variables, under `EnvironmentVariables`, which is left out
/// where there is nothing to give.
fn plist(
    job: Job,
    program: &Path,
    options: &[String],
    environment: &[(String, String)],
    home: &Path,
) -> String {
    let environment = if environment.is_empty() {
        String::new()
    } else {
        format!(
            "  <key>EnvironmentVariables</key>\n  <dict>\n{}  </dict>\n",
            environment
                .iter()
                .map(|(name, value)| format!(
                    "    <key>{}</key><string>{}</string>\n",
                    escape(name),
                    escape(value)
                ))
                .collect::<String>()
        )
    };
    let arguments = std::iter::once(program.to_string_lossy().into_owned())
        .chain(runs(job).iter().map(|word| (*word).to_owned()))
        .chain(options.iter().cloned())
        .map(|argument| format!("    <string>{}</string>\n", escape(&argument)))
        .collect::<String>();
    let when = match job {
        Job::Renew => format!(
            "  <key>StartInterval</key><integer>{EVERY_SECONDS}</integer>\n  \
             <key>RunAtLoad</key><false/>\n  \
             <key>LowPriorityIO</key><true/>\n  \
             <key>ProcessType</key><string>Background</string>\n"
        ),
        Job::Watch => {
            let log = escape(&Launchd::log_folder(home).join(LOG).to_string_lossy());
            format!(
                "  <key>RunAtLoad</key><true/>\n  \
                 <key>KeepAlive</key>\n  <dict>\n    \
                 <key>SuccessfulExit</key><false/>\n    <key>Crashed</key><true/>\n  </dict>\n  \
                 <key>ThrottleInterval</key><integer>{RESTART_AFTER_SECONDS}</integer>\n  \
                 <key>StandardOutPath</key><string>{log}</string>\n  \
                 <key>StandardErrorPath</key><string>{log}</string>\n"
            )
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key>
  <array>
{arguments}  </array>
{when}{environment}</dict>
</plist>
"#,
        Launchd::label(job)
    )
}

/// The strings of the `ProgramArguments` array a plist [`plist`] wrote, the program first.
fn arguments(body: &str) -> Vec<String> {
    let Some(array) = body
        .split_once("<key>ProgramArguments</key>")
        .and_then(|(_, after)| after.split_once("</array>"))
        .map(|(array, _)| array)
    else {
        return Vec::new();
    };
    array
        .split("<string>")
        .skip(1)
        .filter_map(|after| after.split_once("</string>"))
        .map(|(argument, _)| unescape(argument))
        .collect()
}

/// The variables a plist [`plist`] wrote gives its job, in the order written. Only what
/// `plist` writes is read: each key with the string right after it, in the dictionary after
/// `EnvironmentVariables`. Neither can hold a `<` of its own, which [`escape`] writes as
/// `&lt;`, so each ends where its closing tag is.
fn given(body: &str) -> Vec<(String, String)> {
    let Some(mut rest) = body
        .split_once("<key>EnvironmentVariables</key>")
        .and_then(|(_, after)| after.split_once("<dict>"))
        .and_then(|(_, after)| after.split_once("</dict>"))
        .map(|(dict, _)| dict)
    else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    while let Some((_, after)) = rest.split_once("<key>") {
        let Some((name, after)) = after.split_once("</key>") else {
            break;
        };
        let Some((value, after)) = after
            .strip_prefix("<string>")
            .and_then(|after| after.split_once("</string>"))
        else {
            break;
        };
        pairs.push((unescape(name), unescape(value)));
        rest = after;
    }
    pairs
}

/// A path, a name or a value as XML text. An app can be kept in a folder whose name has an
/// ampersand in it, a proxy's password can hold one, and launchd refuses a plist that is not
/// well formed. A carriage return is written as a reference, since XML reads one written as
/// it is as a line feed.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;")
}

/// What [`escape`] wrote, read back.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#13;", "\r")
        .replace("&amp;", "&")
}

/// Whether XML 1.0 can hold `c` in a plist, as itself or as a reference: every character but
/// the control characters other than a tab, a line feed and a carriage return, and U+FFFE
/// and U+FFFF.
fn xml_holds(c: char) -> bool {
    !matches!(
        c,
        '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}'
    )
}

/// The refusal for a program or a variable [`plist`] could not write, naming which, where
/// one holds a character XML 1.0 cannot hold. launchd would refuse the plist, and saying
/// which holds it is what lets a person take it out. systemd's unit writes such a character
/// as an escape instead.
fn unwritable(program: &Path, environment: &[(String, String)]) -> Option<Error> {
    let program = program.to_string_lossy();
    std::iter::once((
        "the path of the program it runs".to_string(),
        program.as_ref(),
    ))
    .chain(environment.iter().flat_map(|(name, value)| {
        [
            (format!("the name {name}"), name.as_str()),
            (name.clone(), value.as_str()),
        ]
    }))
    .find_map(|(what, text)| {
        text.chars()
            .find(|c| !xml_holds(*c))
            .map(|c| Error::ScheduleRefused {
                detail: format!(
                    "a LaunchAgent cannot hold the character U+{:04X} that {what} holds, so \
                     nothing was scheduled. Take it out, then install the schedule again.",
                    u32::from(c)
                ),
            })
    })
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::schedule::{install, installed_program, repair};
    use std::os::unix::fs::PermissionsExt;

    /// launchd stops a job's own process when it unloads the job, and a repair unloads the
    /// schedule before loading it again. Made from inside the schedule's own job, as by an
    /// app the old schedule started, it would leave nothing loaded, so it is left to the app
    /// once it is opened any other way.
    #[test]
    fn a_schedule_is_not_repaired_from_inside_its_own_job() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-launchd-inside-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".pitboard")).expect("a Pitboard home");
        let app = home.join("Applications/Pitboard.app/Contents/MacOS/Pitboard");
        let bundled = home.join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        for program in [&app, &bundled] {
            std::fs::create_dir_all(program.parent().unwrap()).expect("its directory");
            std::fs::write(program, "").expect("a program");
        }
        let ctx = Context::new(home.clone()).with_memory_stores(MemoryHost::new());
        install(
            &ctx.clone().with_schedule_program(app.clone()),
            Permit::for_a_test(),
        )
        .expect("0.3.0's schedule");
        let the_app = ctx.clone().with_schedule_program(bundled.clone());

        assert!(
            !repair(
                &the_app.clone().with_scheduled_job(LABEL.into()),
                Permit::for_a_test()
            )
            .expect("nothing to do"),
            "started by the schedule"
        );
        assert_eq!(installed_program(&ctx), Some(app));
        assert_eq!(
            crate::audit::read(&ctx, 1)
                .iter()
                .map(|e| e.subject.as_str())
                .collect::<Vec<_>>(),
            ["install"],
            "and nothing recorded"
        );

        let opened = the_app.with_scheduled_job("application.com.datlechin.pitboard.1.2".into());
        assert!(
            repair(&opened, Permit::for_a_test()).expect("repaired"),
            "opened from Finder"
        );
        assert_eq!(installed_program(&ctx), Some(bundled));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The path is written as XML text and read back as the path it was.
    #[test]
    fn a_path_launchd_has_to_escape_reads_back_as_it_was() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-launchd-escape-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let bundled = home.join("Tools&Apps/Pitboard.app/Contents/Helpers/pitboard");
        std::fs::create_dir_all(bundled.parent().unwrap()).expect("its directory");
        std::fs::write(&bundled, "").expect("a program");
        let ctx = Context::new(home.clone())
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(bundled.clone());
        install(&ctx, Permit::for_a_test()).expect("installed");
        let written = std::fs::read_to_string(Launchd::agent(&ctx, Job::Renew)).expect("the agent");
        assert!(written.contains("/Tools&amp;Apps/"), "{written}");
        assert!(!written.contains("/Tools&Apps/"), "{written}");
        assert_eq!(installed_program(&ctx), Some(bundled));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// What it runs is one verb with no arguments, and what it does not do is as much the
    /// point as what it does.
    #[test]
    fn the_agent_runs_one_verb_and_does_not_run_at_load() {
        let body = plist(
            Job::Renew,
            Path::new("/usr/local/bin/pitboard"),
            &[],
            &[],
            Path::new("/Users/x"),
        );
        assert!(body.contains("<string>/usr/local/bin/pitboard</string>"));
        assert!(body.contains("<string>renew</string>"));
        assert!(!body.contains("status"), "it never asks for usage");
        assert!(!body.contains("use"), "and it never switches");
        assert!(
            body.contains("<key>RunAtLoad</key><false/>"),
            "installing it is not a reason to talk to Anthropic that second"
        );
        assert!(body.contains(&format!("<integer>{EVERY_SECONDS}</integer>")));
    }

    // At load, which is at each login too, and again a minute after it fails or crashes, which a
    // panic's exit and an older run meeting a newer state file both are.
    #[test]
    fn the_watch_agent_runs_at_load_and_again_a_minute_after_it_fails() {
        let at = ["--at".to_string(), "90".to_string()];
        let body = plist(
            Job::Watch,
            Path::new("/usr/local/bin/pitboard"),
            &at,
            &[],
            Path::new("/Users/x & y"),
        );
        assert!(
            body.contains(
                "  <key>Label</key><string>com.datlechin.pitboard.watch</string>\n  \
                 <key>ProgramArguments</key>\n  <array>\n    \
                 <string>/usr/local/bin/pitboard</string>\n    <string>watch</string>\n    \
                 <string>--scheduled</string>\n    <string>--at</string>\n    \
                 <string>90</string>\n  </array>\n  \
                 <key>RunAtLoad</key><true/>\n  \
                 <key>KeepAlive</key>\n  <dict>\n    \
                 <key>SuccessfulExit</key><false/>\n    <key>Crashed</key><true/>\n  </dict>\n  \
                 <key>ThrottleInterval</key><integer>60</integer>\n  \
                 <key>StandardOutPath</key>\
                 <string>/Users/x &amp; y/Library/Logs/Pitboard/watch.log</string>\n  \
                 <key>StandardErrorPath</key>\
                 <string>/Users/x &amp; y/Library/Logs/Pitboard/watch.log</string>\n\
                 </dict>\n"
            ),
            "{body}"
        );
        assert!(!body.contains("StartInterval"), "{body}");
        assert_eq!(
            arguments(&body),
            [
                "/usr/local/bin/pitboard",
                "watch",
                "--scheduled",
                "--at",
                "90"
            ]
        );
        let renewal = plist(
            Job::Renew,
            Path::new("/usr/local/bin/pitboard"),
            &[],
            &[],
            Path::new("/Users/x"),
        );
        assert!(!renewal.contains("Standard"), "{renewal}");
    }

    // launchd opens the log but makes no folder for it, and the log goes with the agent.
    #[test]
    fn the_watch_agent_keeps_its_lines_in_a_private_log_that_goes_with_it() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-launchd-log-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch home");
        let ctx = Context::new(home.clone()).with_memory_stores(MemoryHost::new());
        let launchd = Launchd::new(Arc::new(service::Pretend {
            refuse_start: Arc::default(),
        }));
        let log = home.join("Library/Logs/Pitboard/watch.log");
        let at = ["--at".to_string(), "90".to_string()];
        launchd
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                Path::new("/usr/local/bin/pitboard"),
                &at,
                &[],
            )
            .expect("installed");
        let folder = log.parent().expect("its folder");
        assert_eq!(
            crate::host::fs::access(folder).map(|access| access.described),
            Some("mode 700".to_string())
        );
        std::fs::write(&log, "Watching Claude Code.\n").expect("launchd's");

        assert!(
            launchd
                .remove(&ctx, Permit::for_a_test(), Job::Watch)
                .expect("taken away")
        );
        assert!(!folder.exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// launchd refuses a plist that is not well formed, and a folder's name can hold any of
    /// the characters XML gives a meaning to.
    #[test]
    fn the_agent_writes_a_path_as_xml_text() {
        let body = plist(
            Job::Renew,
            Path::new("/Users/x/A&B <old>/Pitboard.app"),
            &[],
            &[],
            Path::new("/Users/x"),
        );
        assert!(
            body.contains("<string>/Users/x/A&amp;B &lt;old&gt;/Pitboard.app</string>"),
            "{body}"
        );
    }

    /// The pairs a job is given, as `install` hands them over.
    fn pairs(given: &[(&str, &str)]) -> Vec<(String, String)> {
        given
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// The job runs with launchd's environment, not the shell's, so what it is given of the
    /// installing Pitboard's, `PITBOARD_NO_ARGV` and the proxy variables, is written into it
    /// as XML text, and nothing is written where there is nothing to give. What was written
    /// reads back as it was, and the program is still read back too.
    #[test]
    fn the_agent_gives_its_job_the_variables_it_is_handed_and_reads_them_back() {
        let program = Path::new("/usr/local/bin/pitboard");
        let handed = pairs(&[
            ("PITBOARD_NO_ARGV", "1"),
            ("ALL_PROXY", "socks5h://alice:p&ss<w>rd@127.0.0.1:1080"),
            ("HTTPS_PROXY", "http://proxy.example.com:3128"),
            ("no_proxy", ""),
            ("NO_PROXY", ".anthropic.com,localhost"),
        ]);
        let body = plist(Job::Renew, program, &[], &handed, Path::new("/Users/x"));
        assert!(
            body.contains(
                "  <key>EnvironmentVariables</key>\n  <dict>\n    \
                 <key>PITBOARD_NO_ARGV</key><string>1</string>\n    \
                 <key>ALL_PROXY</key><string>socks5h://alice:p&amp;ss&lt;w&gt;rd@127.0.0.1:1080\
                 </string>\n    \
                 <key>HTTPS_PROXY</key><string>http://proxy.example.com:3128</string>\n    \
                 <key>no_proxy</key><string></string>\n    \
                 <key>NO_PROXY</key><string>.anthropic.com,localhost</string>\n  </dict>\n\
                 </dict>\n"
            ),
            "{body}"
        );
        assert_eq!(given(&body), handed);
        assert!(
            !plist(Job::Renew, program, &[], &[], Path::new("/Users/x"))
                .contains("EnvironmentVariables")
        );
        assert_eq!(
            given(&plist(Job::Renew, program, &[], &[], Path::new("/Users/x"))),
            Vec::new()
        );
        let (_, after) = body
            .split_once("<key>ProgramArguments</key>")
            .expect("its arguments");
        assert!(
            after
                .trim_start()
                .starts_with("<array>\n    <string>/usr/local/bin/pitboard")
        );
    }

    /// A variable holding a character XML 1.0 cannot hold, which launchd would refuse the
    /// whole plist for, is refused before anything is written, naming the variable. A
    /// carriage return, which XML would read back as a line feed, is written as a reference
    /// and reads back as itself.
    #[test]
    fn a_variable_a_plist_cannot_hold_is_refused_by_name() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-launchd-unwritable-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its directory")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let home_text = home.to_string_lossy().into_owned();
        let env: crate::context::Environment = [
            ("HOME", home_text.as_str()),
            ("HTTPS_PROXY", "http://proxy.example.com:3128"),
            ("ALL_PROXY", "socks5h://alice:s3\u{1}cret@127.0.0.1:1080"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env)
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(program);
        let refused = install(&ctx, Permit::for_a_test()).expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        let said = refused.to_string();
        assert!(
            said.contains("U+0001 that ALL_PROXY holds") && !said.contains("s3"),
            "{said}"
        );
        assert!(
            !Launchd::agent(&ctx, Job::Renew).exists(),
            "nothing was written"
        );
        let _ = std::fs::remove_dir_all(&home);

        let handed = pairs(&[("NO_PROXY", "a\rb")]);
        let body = plist(
            Job::Renew,
            Path::new("/usr/local/bin/pitboard"),
            &[],
            &handed,
            Path::new("/Users/x"),
        );
        assert!(body.contains("<string>a&#13;b</string>"), "{body}");
        assert_eq!(given(&body), handed);
        assert_eq!(
            unwritable(Path::new("/usr/local/bin/pitboard"), &handed).map(|e| e.to_string()),
            None
        );
    }

    /// `pitboard schedule install` writes the proxy variables it was started with into the
    /// agent, and the agent only the person can read, mode 600, since a proxy's address can
    /// hold a password: over an agent that was open to others too, which kept its mode when
    /// Pitboard wrote it before.
    #[test]
    fn an_installed_agent_carries_the_proxy_and_only_its_owner_can_read_it() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-launchd-proxy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its directory")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let home_text = home.to_string_lossy().into_owned();
        let env: crate::context::Environment = [
            ("HOME", home_text.as_str()),
            ("ALL_PROXY", "socks5h://alice:s3cret@127.0.0.1:1080"),
            ("no_proxy", "localhost"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env)
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(program.clone());
        let agent = Launchd::agent(&ctx, Job::Renew);
        std::fs::create_dir_all(agent.parent().expect("its directory")).expect("made");
        std::fs::write(&agent, "an agent open to others\n").expect("written");
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o644))
            .expect("opened to others");

        install(&ctx, Permit::for_a_test()).expect("installed");
        let written = std::fs::read_to_string(&agent).expect("the agent");
        assert!(
            written.contains(
                "<key>ALL_PROXY</key><string>socks5h://alice:s3cret@127.0.0.1:1080</string>\n    \
                 <key>no_proxy</key><string>localhost</string>\n"
            ),
            "{written}"
        );
        assert_eq!(
            crate::host::fs::access(&agent).map(|access| access.described),
            Some("mode 600".to_string())
        );
        assert_eq!(installed_program(&ctx), Some(program));
        let _ = std::fs::remove_dir_all(&home);
    }
}

// The http open command split as Codex's webbrowser splits it (`codex_login_is_driveable`).

pub fn command_args(cmdline: &str, url: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut push = |token: &str| {
        if matches!(token, "%0" | "%1") {
            tokens.push(url.to_string());
        } else {
            tokens.push(token.to_string());
        }
    };
    let mut start: Option<usize> = None;
    let mut in_quotes = false;
    let mut idx = 0;
    for ch in cmdline.chars() {
        idx += ch.len_utf8();
        match ch {
            '"' => {
                if let Some(s) = start {
                    push(&cmdline[s..idx - 1]);
                    start = None;
                    in_quotes = false;
                } else {
                    start = Some(idx);
                    in_quotes = true;
                }
            }
            ' ' if !in_quotes => {
                if let Some(s) = start {
                    push(&cmdline[s..idx - 1]);
                    start = None;
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(idx - ch.len_utf8());
                }
            }
        }
    }
    if let Some(s) = start {
        push(&cmdline[s..]);
    }
    tokens
}

pub fn file_url(path: &str) -> String {
    let forward = path.replace('\\', "/");
    let encoded: String = forward
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '#' => "%23".to_string(),
            '%' => "%25".to_string(),
            c => c.to_string(),
        })
        .collect();
    format!("file:///{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_command_becomes_its_program_and_the_url() {
        let cmd = r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --single-argument %1"#;
        assert_eq!(
            command_args(cmd, "file:///C:/s/p.html"),
            [
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                "--single-argument",
                "file:///C:/s/p.html"
            ]
        );
    }

    #[test]
    fn an_unquoted_command_splits_on_spaces_and_takes_percent_zero_too() {
        assert_eq!(
            command_args(r"C:\b\browser.exe -osint -url %0", "u"),
            [r"C:\b\browser.exe", "-osint", "-url", "u"]
        );
        assert!(command_args("", "u").is_empty());
    }

    #[test]
    fn a_local_page_is_a_file_url() {
        assert_eq!(
            file_url(r"D:\a\_temp\pitboard probe\pitboard-probe-standin.html"),
            "file:///D:/a/_temp/pitboard%20probe/pitboard-probe-standin.html"
        );
    }
}

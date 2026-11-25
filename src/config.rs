//! The config file: flags voxl reads before its command line, so a voxl started at login
//! (`--install-autostart`) runs with them, and changing them means editing one file.
//!
//! One flag per line as on the command line (`--name=value`, `--name value` or `--name`),
//! with `#` comments and blank lines. Flags on the command line win over the file's.

use std::path::PathBuf;

/// Where voxl keeps its files: %LOCALAPPDATA%\voxl on Windows, $XDG_CONFIG_HOME/voxl or
/// ~/.config/voxl elsewhere.
pub(crate) fn config_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.unwrap_or_else(|| PathBuf::from(".")).join("voxl")
}

/// The config file: VOXL_CONFIG, else voxl.conf in [`config_dir`].
pub fn config_path() -> PathBuf {
    std::env::var_os("VOXL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir().join("voxl.conf"))
}

/// The config file's flags, each as one `--name=value` argument; none if there is no file.
pub fn config_args() -> Result<Vec<String>, String> {
    let path = config_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("reading {}: {e}", path.display())),
    };
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(flag) = line.strip_prefix("--") else {
            return Err(format!(
                "line {}: expected a flag starting with --, got {line:?}",
                n + 1
            ));
        };
        let (name, value) = match flag.split_once(|c: char| c == '=' || c.is_whitespace()) {
            Some((name, value)) => (name, unquote(value.trim())),
            None => (flag, "true"),
        };
        if name.is_empty() {
            return Err(format!("line {}: a flag needs a name", n + 1));
        }
        args.push(format!("--{name}={value}"));
    }
    Ok(args)
}

/// A value in matching single or double quotes, as a shell would take it.
fn unquote(v: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_flags_like_the_command_line() {
        let text = "
# streaming
--addr=0.0.0.0
--tls
  --preset superfast
--basic-auth-password 'a b;-'
--title=\"My Mac\"
";
        assert_eq!(
            parse(text).unwrap(),
            [
                "--addr=0.0.0.0",
                "--tls=true",
                "--preset=superfast",
                "--basic-auth-password=a b;-",
                "--title=My Mac",
            ]
        );
    }

    #[test]
    fn rejects_lines_that_are_not_flags() {
        assert!(parse("addr=0.0.0.0").unwrap_err().contains("line 1"));
        assert!(parse("--tls\n--=x").unwrap_err().contains("line 2"));
    }
}

//! `--install-autostart` and `--uninstall-autostart`: start voxl whenever this user logs
//! in, with the flags in the config file. On macOS that's a launchd agent, which also
//! starts voxl again if it crashes and writes its output to ~/Library/Logs/voxl.log.

/// The launchd label, and the agent's file name.
#[cfg(target_os = "macos")]
const LABEL: &str = "com.voxl";

/// Which of the two the arguments ask for, if either.
#[derive(Debug, PartialEq)]
pub enum Request {
    Install,
    Uninstall,
}

pub fn request(args: &[String]) -> Option<Request> {
    if args.iter().any(|a| a == "--install-autostart") {
        Some(Request::Install)
    } else if args.iter().any(|a| a == "--uninstall-autostart") {
        Some(Request::Uninstall)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
mod launchd {
    use super::LABEL;
    use std::path::PathBuf;
    use std::process::Command;

    fn home() -> Result<PathBuf, String> {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set".to_string())
    }

    fn agent_path() -> Result<PathBuf, String> {
        Ok(home()?
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn domain() -> String {
        format!("gui/{}", unsafe { libc::getuid() })
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    pub(super) fn plist(exe: &str, log: &str) -> String {
        let (exe, log) = (xml_escape(exe), xml_escape(log));
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ThrottleInterval</key>
	<integer>10</integer>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
</dict>
</plist>
"#
        )
    }

    fn launchctl(args: &[&str]) -> Result<std::process::Output, String> {
        Command::new("launchctl")
            .args(args)
            .output()
            .map_err(|e| format!("running launchctl: {e}"))
    }

    fn loaded() -> Result<bool, String> {
        Ok(launchctl(&["print", &format!("{}/{LABEL}", domain())])?
            .status
            .success())
    }

    fn unload() -> Result<(), String> {
        if !loaded()? {
            return Ok(());
        }
        let out = launchctl(&["bootout", &format!("{}/{LABEL}", domain())])?;
        if !out.status.success() {
            return Err(format!(
                "launchctl bootout: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }

    pub fn install() -> Result<(), String> {
        let exe = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|e| format!("finding voxl's own path: {e}"))?;
        let log = home()?.join("Library/Logs/voxl.log");
        let path = agent_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, plist(&exe.to_string_lossy(), &log.to_string_lossy()))
            .map_err(|e| format!("writing {}: {e}", path.display()))?;
        unload()?;
        let out = launchctl(&["bootstrap", &domain(), &path.to_string_lossy()])?;
        if !out.status.success() {
            return Err(format!(
                "launchctl bootstrap: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let config = crate::config::config_path();
        println!("voxl now starts when you log in: {}", path.display());
        println!("  runs     {}", exe.display());
        println!(
            "  flags    {}{}",
            config.display(),
            if config.exists() {
                ""
            } else {
                " (missing: voxl runs with its defaults)"
            }
        );
        println!("  log      {}", log.display());
        println!("  restart  launchctl kickstart -k {}/{LABEL}", domain());
        println!(
            "voxl itself now needs Screen Recording and Accessibility in System Settings >\n\
             Privacy & Security (a terminal's permission doesn't carry over), granted again\n\
             whenever the voxl binary is rebuilt."
        );
        Ok(())
    }

    pub fn uninstall() -> Result<(), String> {
        unload()?;
        let path = agent_path()?;
        match std::fs::remove_file(&path) {
            Ok(()) => println!("removed {}; voxl no longer starts at login", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("voxl was not set to start at login")
            }
            Err(e) => return Err(format!("removing {}: {e}", path.display())),
        }
        Ok(())
    }
}

/// Carries out the request, printing what it did.
pub fn handle(request: Request) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return match request {
        Request::Install => launchd::install(),
        Request::Uninstall => launchd::uninstall(),
    };
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err("starting at login is only set up on macOS so far".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recognizes_the_flags() {
        assert_eq!(
            request(&args(&["--install-autostart"])),
            Some(Request::Install)
        );
        assert_eq!(
            request(&args(&["--uninstall-autostart"])),
            Some(Request::Uninstall)
        );
        assert_eq!(request(&args(&["--port=6901"])), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_escapes_paths() {
        let p = launchd::plist("/a&b/voxl", "/logs/voxl.log");
        assert!(p.contains("<string>/a&amp;b/voxl</string>"));
        assert!(p.contains("<string>com.voxl</string>"));
    }
}

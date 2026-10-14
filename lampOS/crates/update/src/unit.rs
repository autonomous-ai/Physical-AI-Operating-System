//! A part's unit file, as far as an update reads and changes it: the
//! program its `ExecStart=` starts, in the folder named by the build.
//!
//! A part is installed as `/opt/lampOS/<folder>/<sha256>/<binary>` and its
//! unit names that path (docs/joining.md, "Installing"). The 64 hex
//! characters are the build. Changing a part's build is writing another
//! build in their place, everywhere the unit names that folder: a part's
//! files beside its binary (the controls' settings) are named through the
//! same folder.

/// Where every part is installed.
pub const INSTALLED: &str = "/opt/lampOS/";

/// The program a unit starts, when it is a part installed by build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// The folder of its builds: `/opt/lampOS/voice/`, or on lamp-4ace
    /// `/opt/lampOS/releases/` for the voice.
    pub builds: String,
    /// The build: 64 lowercase hex characters.
    pub build: String,
    /// The binary's name: `lamp-voice`.
    pub binary: String,
}

impl Program {
    /// The build's folder, as the unit names it: `/opt/lampOS/voice/<sha256>/`.
    pub fn folder(&self) -> String {
        folder(&self.builds, &self.build)
    }
}

/// A build's folder: `<builds><build>/`.
pub fn folder(builds: &str, build: &str) -> String {
    format!("{builds}{build}/")
}

/// Whether a path is a build's folder: `/opt/lampOS/<folder>/<64 hex>/`.
pub fn is_build_folder(path: &str) -> bool {
    path.strip_prefix(INSTALLED)
        .and_then(|rest| rest.strip_suffix('/'))
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(folder, build)| {
            crate::manifest::is_token(folder)
                && build.len() == 64
                && crate::manifest::is_lower_hex(build)
        })
}

/// The program of the unit's first `ExecStart=`, if it is
/// `/opt/lampOS/<one folder>/<64 hex>/lamp-<name>`. Anything else is a
/// unit this program does not update.
pub fn program(text: &str) -> Option<Program> {
    let line = text
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("ExecStart="))?;
    // systemd's prefixes: - @ : + ! say how to run it, not what.
    let command = line.trim_start_matches(['-', '@', ':', '+', '!']);
    let path = command.split_whitespace().next()?;
    let rest = path.strip_prefix(INSTALLED)?;
    let mut pieces = rest.split('/');
    let (folder, build, binary) = (pieces.next()?, pieces.next()?, pieces.next()?);
    if pieces.next().is_some()
        || !crate::manifest::is_token(folder)
        || build.len() != 64
        || !crate::manifest::is_lower_hex(build)
        || !crate::manifest::is_binary_name(binary)
    {
        return None;
    }
    Some(Program {
        builds: format!("{INSTALLED}{folder}/"),
        build: build.to_owned(),
        binary: binary.to_owned(),
    })
}

/// The unit with its program's folder at `build` instead: every place it
/// names the old build's folder.
pub fn with_build(text: &str, program: &Program, build: &str) -> String {
    text.replace(&program.folder(), &folder(&program.builds, build))
}

/// What a unit says once its comments and blank lines are left out: two
/// units that differ only there are the same unit to systemd.
fn meaning(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| {
            let line = line.trim_start();
            !line.is_empty() && !line.starts_with('#') && !line.starts_with(';')
        })
        .collect()
}

/// Whether two units would make systemd do the same.
pub fn same(a: &str, b: &str) -> bool {
    meaning(a) == meaning(b)
}

/// The lines only in `before` (`- `) and only in `after` (`+ `), in order,
/// for a person to read before an update is applied.
pub fn differences(before: &str, after: &str) -> Vec<String> {
    let (before, after) = (meaning(before), meaning(after));
    let mut lines: Vec<String> = before
        .iter()
        .filter(|line| !after.contains(line))
        .map(|line| format!("- {line}"))
        .collect();
    lines.extend(
        after
            .iter()
            .filter(|line| !before.contains(line))
            .map(|line| format!("+ {line}")),
    );
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn lamp_4aces_own_units_are_read() {
        // As they are in the repository: what the lamp runs.
        let voice = program(include_str!("../../../deploy/lamp-4ace/lamp-voice.service")).unwrap();
        assert_eq!(voice.builds, "/opt/lampOS/releases/");
        assert_eq!(voice.binary, "lamp-voice");
        let controls = program(include_str!(
            "../../../deploy/lamp-4ace/lamp-controls.service"
        ))
        .unwrap();
        assert_eq!(controls.builds, "/opt/lampOS/controls/");
        assert_eq!(controls.binary, "lamp-controls");
        // A template's placeholder is not a build.
        assert_eq!(
            program(include_str!("../../../deploy/units/lamp-voice.service")),
            None
        );
    }

    #[test]
    fn a_new_build_is_written_everywhere_the_folder_is_named() {
        let text = format!(
            "[Service]\nExecStart=/opt/lampOS/controls/{A}/lamp-controls serve --config /opt/lampOS/controls/{A}/controls.json\n"
        );
        let program = program(&text).unwrap();
        assert_eq!(program.build, A);
        let changed = with_build(&text, &program, B);
        assert_eq!(changed, text.replace(A, B));
        assert_eq!(super::program(&changed).unwrap().build, B);
        assert!(is_build_folder(&program.folder()));
        assert!(!is_build_folder("/opt/lampOS/"));
        assert!(!is_build_folder(&format!("/opt/lampOS/../{A}/")));
        assert!(!is_build_folder(&format!("/etc/{A}/")));
    }

    #[test]
    fn only_a_part_installed_by_build_is_a_program() {
        for text in [
            "ExecStart=/usr/bin/amixer set Speaker 80%\n".to_owned(),
            format!("ExecStart=/opt/lampOS/voice/{A}/bash\n"),
            format!("ExecStart=/opt/lampOS/voice/{}/lamp-voice\n", &A[..60]),
            format!("ExecStart=/opt/lampOS/a/b/{A}/lamp-voice\n"),
            "[Service]\n".to_owned(),
        ] {
            assert_eq!(program(&text), None, "{text}");
        }
        let prefixed = format!("ExecStart=-/opt/lampOS/voice/{A}/lamp-voice serve\n");
        assert_eq!(program(&prefixed).unwrap().binary, "lamp-voice");
    }

    #[test]
    fn comments_do_not_make_a_unit_another() {
        let a = "# one\n[Service]\nExecStart=/x\n\n";
        let b = "[Service]\n# two\nExecStart=/x\n";
        assert!(same(a, b));
        assert!(!same(a, "[Service]\nExecStart=/x --on\n"));
        assert_eq!(
            differences(a, "[Service]\nExecStart=/x --on\n"),
            vec!["- ExecStart=/x", "+ ExecStart=/x --on"]
        );
    }
}

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

struct RevealCommand {
    program: &'static str,
    args: Vec<OsString>,
}

pub fn reveal(path: &Path) -> Result<(), String> {
    let command = native_reveal_command(path)?;
    let status = Command::new(command.program)
        .args(command.args)
        .status()
        .map_err(|error| format!("failed to start the system file manager: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("system file manager exited with status {status}"))
    }
}

#[cfg(target_os = "macos")]
fn native_reveal_command(path: &Path) -> Result<RevealCommand, String> {
    Ok(RevealCommand {
        program: "open",
        args: vec![OsString::from("-R"), path.as_os_str().to_owned()],
    })
}

#[cfg(target_os = "windows")]
fn native_reveal_command(path: &Path) -> Result<RevealCommand, String> {
    let mut selection = OsString::from("/select,");
    selection.push(path.as_os_str());
    Ok(RevealCommand {
        program: "explorer.exe",
        args: vec![selection],
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_reveal_command(_path: &Path) -> Result<RevealCommand, String> {
    Err("revealing a photo is supported only on macOS and Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn builds_a_finder_reveal_command_without_shell_parsing() {
        let path = Path::new("/tmp/photo name.jpg");
        let command = native_reveal_command(path).unwrap();

        assert_eq!(command.program, "open");
        assert_eq!(
            command.args,
            vec![OsString::from("-R"), path.as_os_str().to_owned()]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn builds_an_explorer_selection_command_without_shell_parsing() {
        let path = Path::new(r"C:\Photos\photo name.jpg");
        let command = native_reveal_command(path).unwrap();

        assert_eq!(command.program, "explorer.exe");
        assert_eq!(
            command.args,
            vec![OsString::from(r"/select,C:\Photos\photo name.jpg")]
        );
    }
}

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

struct FileManagerCommand {
    program: &'static str,
    args: Vec<OsString>,
}

pub fn reveal(path: &Path) -> Result<(), String> {
    run_file_manager_command(native_reveal_command(path)?)
}

pub fn open_directory(path: &Path) -> Result<(), String> {
    run_file_manager_command(native_open_directory_command(path)?)
}

fn run_file_manager_command(command: FileManagerCommand) -> Result<(), String> {
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
fn native_reveal_command(path: &Path) -> Result<FileManagerCommand, String> {
    Ok(FileManagerCommand {
        program: "open",
        args: vec![OsString::from("-R"), path.as_os_str().to_owned()],
    })
}

#[cfg(target_os = "macos")]
fn native_open_directory_command(path: &Path) -> Result<FileManagerCommand, String> {
    Ok(FileManagerCommand {
        program: "open",
        args: vec![path.as_os_str().to_owned()],
    })
}

#[cfg(target_os = "windows")]
fn native_reveal_command(path: &Path) -> Result<FileManagerCommand, String> {
    let mut selection = OsString::from("/select,");
    selection.push(path.as_os_str());
    Ok(FileManagerCommand {
        program: "explorer.exe",
        args: vec![selection],
    })
}

#[cfg(target_os = "windows")]
fn native_open_directory_command(path: &Path) -> Result<FileManagerCommand, String> {
    Ok(FileManagerCommand {
        program: "explorer.exe",
        args: vec![path.as_os_str().to_owned()],
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_reveal_command(_path: &Path) -> Result<FileManagerCommand, String> {
    Err("revealing a photo is supported only on macOS and Windows".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_open_directory_command(_path: &Path) -> Result<FileManagerCommand, String> {
    Err("opening a photo directory is supported only on macOS and Windows".into())
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

    #[cfg(target_os = "macos")]
    #[test]
    fn builds_a_finder_open_directory_command_without_shell_parsing() {
        let path = Path::new("/tmp/photo folder");
        let command = native_open_directory_command(path).unwrap();

        assert_eq!(command.program, "open");
        assert_eq!(command.args, vec![path.as_os_str().to_owned()]);
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

    #[cfg(target_os = "windows")]
    #[test]
    fn builds_an_explorer_open_directory_command_without_shell_parsing() {
        let path = Path::new(r"C:\Photos\photo folder");
        let command = native_open_directory_command(path).unwrap();

        assert_eq!(command.program, "explorer.exe");
        assert_eq!(command.args, vec![path.as_os_str().to_owned()]);
    }
}

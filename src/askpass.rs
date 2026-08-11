use std::process::Command;
use std::io::{self, Write};

/// Escape Pango markup special characters so dynamic text cannot break or
/// misformat the zenity dialog markup.
fn escape_pango(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn prompt_password() -> io::Result<String> {
    #[cfg(target_os = "macos")]
    {
        let script = r#"
            tell application "System Events"
                activate
                display dialog "sudo-me needs your password to continue:" \
                    default answer "" \
                    with title "sudo-me Authentication" \
                    with hidden answer \
                    with icon POSIX file "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/FileVaultIcon.icns" \
                    buttons {"Cancel", "OK"} default button "OK"
                return text returned of result
            end tell
        "#;
        let output = Command::new("osascript").arg("-e").arg(script).output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }

    #[cfg(unix)]
    {
        // Try GUI first if display is available
        if std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok() {
            if let Ok(output) = Command::new("zenity")
                .args(&["--password", "--title=sudo-me Authentication"])
                .output()
            {
                if output.status.success() {
                    return Ok(String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string());
                }
            }
            
            if let Ok(output) = Command::new("kdialog")
                .args(&["--password", "sudo-me Authentication"])
                .output()
            {
                if output.status.success() {
                    return Ok(String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_string());
                }
            }
        }

        // Fallback to TTY
        use std::fs::OpenOptions;
        use std::io::Read;
        use std::os::unix::io::AsRawFd;
        use libc::{tcgetattr, tcsetattr, termios, ECHO, ECHONL, TCSANOW};
        
        if let Ok(mut tty) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
            let fd = tty.as_raw_fd();
            let mut termios_old: termios = unsafe { std::mem::zeroed() };
            let got_termios = unsafe { tcgetattr(fd, &mut termios_old) } == 0;
            if got_termios {
                let mut termios_new = termios_old;
                termios_new.c_lflag &= !(ECHO);
                termios_new.c_lflag |= ECHONL;
                unsafe { tcsetattr(fd, TCSANOW, &termios_new); }
            }
            let result = (|| -> io::Result<String> {
                write!(tty, "Password: ")?;
                let mut buf = [0; 1024];
                let mut password = String::new();
                if let Ok(n) = tty.read(&mut buf) {
                    password = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                }
                Ok(password)
            })();
            // Restore echo even if the read/write above errored.
            if got_termios {
                unsafe { tcsetattr(fd, TCSANOW, &termios_old); }
            }
            return result;
        }
        Ok(String::new())
    }

    #[cfg(windows)]
    {
        // On Windows, we usually don't prompt for password manually, 
        // we use ShellExecute with 'runas' to elevate.
        // But if we need a password for something else, we'd need a different approach.
        Ok(String::new())
    }

    #[cfg(not(any(unix, windows)))]
    Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
}

pub fn prompt_confirm(command: &str, args: &[String]) -> io::Result<bool> {
    // Force-deny hook (testability, safe direction only): only `=0` (deny) is
    // honored. There is deliberately no force-allow path — confirmation can
    // never be bypassed via the environment.
    if std::env::var("SUDO_ME_CONFIRM").as_deref() == Ok("0") {
        return Ok(false);
    }

    let esc_cmd = escape_pango(command);
    let esc_args = escape_pango(&args.join(" "));
    let message = format!("AI Agent wants to run as root: {} {}. Allow?", esc_cmd, esc_args);

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"
            tell application "System Events"
                activate
                display dialog "{}" \
                    with title "sudo-me Confirmation" \
                    buttons {{"Deny", "Allow"}} default button "Allow"
                return button returned of result
            end tell
            "#,
            message.replace('"', "\\\"")
        );
        let output = Command::new("osascript").arg("-e").arg(&script).output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim() == "Allow");
        }
    }

    #[cfg(unix)]
    {
        if std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok() {
            if let Ok(status) = Command::new("zenity")
                .args(&["--question", "--text", &message, "--title=sudo-me Confirmation"])
                .status()
            {
                return Ok(status.success());
            }
            
            if let Ok(status) = Command::new("kdialog")
                .args(&["--yesno", &message, "--title", "sudo-me Confirmation"])
                .status()
            {
                return Ok(status.success());
            }
        }

        // Fallback to TTY
        use std::fs::OpenOptions;
        use std::io::Read;
        
        if let Ok(mut tty) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
            write!(tty, "{} [y/N]: ", message)?;
            let mut input = [0; 1];
            if tty.read_exact(&mut input).is_ok() {
                let char = input[0] as char;
                return Ok(char == 'y' || char == 'Y');
            }
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let msg_u16: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title_u16: Vec<u16> = "sudo-me Confirmation\0".encode_utf16().collect();
        
        unsafe {
            let result = MessageBoxW(0, msg_u16.as_ptr(), title_u16.as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST);
            return Ok(result == IDYES);
        }
    }

    Ok(false)
}

#[cfg(windows)]
pub fn elevate_self() -> io::Result<()> {
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW;

    let current_exe = std::env::current_exe()?;
    let current_exe_str = current_exe.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Invalid path"))?;
    
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let file: Vec<u16> = format!("{}\0", current_exe_str).encode_utf16().collect();
    let parameters: Vec<u16> = "daemon-windows\0".encode_utf16().collect(); // We'll need to add this subcommand

    unsafe {
        let result = ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            ptr::null(),
            SW_SHOW,
        );
        if result as usize <= 32 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

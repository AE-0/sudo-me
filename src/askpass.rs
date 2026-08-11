use std::process::Command;
use std::io::{self, Write};
use crate::config::Config;

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
        
        if let Ok(mut tty) = OpenOptions::new().read(true).write(true).open("/dev/tty") {
            write!(tty, "Password: ")?;
            let mut password = String::new();
            // Note: This doesn't hide characters, but it's a fallback
            let mut buf = [0; 1024];
            if let Ok(n) = tty.read(&mut buf) {
                password = String::from_utf8_lossy(&buf[..n]).trim().to_string();
            }
            return Ok(password);
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

pub fn prompt_confirm(command: &str, args: &[String], config: &Config) -> io::Result<bool> {
    let inner = format!("AI Agent wants to run as root: {} {}. Allow?", command, args.join(" "));
    // The font is a trusted config value; escape defensively anyway.
    let message = match &config.confirm_font {
        Some(f) => format!(
            "<span font_desc=\"{}\">{}</span>",
            f.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
            inner
        ),
        None => inner,
    };

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
            let mut zenity_args = vec![
                "--question".to_string(),
                "--text".to_string(),
                message.clone(),
                "--title=sudo-me Confirmation".to_string(),
            ];
            if let Some(w) = config.confirm_width {
                zenity_args.push(format!("--width={}", w));
            }
            if let Some(h) = config.confirm_height {
                zenity_args.push(format!("--height={}", h));
            }
            if let Ok(status) = Command::new("zenity").args(&zenity_args).status() {
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

use std::process::Command;
use std::io::{self, Write};

pub fn prompt_password() -> io::Result<String> {
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
    eprint!("Password: ");
    io::stderr().flush()?;
    
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    Ok(password.trim_end_matches('\n').to_string())
}

pub fn prompt_confirm(command: &str, args: &[String]) -> io::Result<bool> {
    if let Ok(val) = std::env::var("SUDO_ME_CONFIRM") {
        return Ok(val == "1" || val.to_lowercase() == "true");
    }
    let message = format!("AI Agent wants to run as root: {} {}. Allow?", command, args.join(" "));
    
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
    
    Ok(false)
}

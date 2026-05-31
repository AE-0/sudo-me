mod askpass;
mod cli;
mod config;
mod daemon;
mod ipc;
mod token;

use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: sudo-me <init|run|approve-config|--askpass>");
        std::process::exit(1);
    }
    
    match args[1].as_str() {
        "--askpass" => {
            match askpass::prompt_password() {
                Ok(pw) => println!("{}", pw),
                Err(e) => {
                    eprintln!("Error reading password: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "init" => {
            let token = token::generate_token();
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).expect("Could not find home directory");
            
            #[cfg(unix)]
            {
                let socket_dir = ipc::unix::create_secure_socket_dir().expect("Failed to create socket dir");
                let socket_path = socket_dir.join("sudo-me.sock");
                let current_exe = env::current_exe().unwrap();
                let uid = unsafe { libc::getuid() }.to_string();
                
                // Spawn daemon via sudo
                let mut cmd = Command::new("sudo");
                cmd.env("SUDO_ASKPASS", &current_exe);
                cmd.arg("-A");
                cmd.arg(&current_exe);
                cmd.arg("daemon");
                cmd.arg(&socket_path);
                cmd.arg(&token);
                cmd.arg(&home);
                cmd.arg(&uid);
                
                // Pass GUI env vars
                for var in &["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"] {
                    if let Ok(val) = env::var(var) {
                        cmd.env(var, val);
                    }
                }
                
                cmd.stdout(std::process::Stdio::piped());
                cmd.stderr(std::process::Stdio::null());
                
                let mut child = cmd.spawn().expect("Failed to spawn daemon");
                
                // Wait for READY signal
                use std::io::BufRead;
                if let Some(stdout) = child.stdout.take() {
                    let reader = std::io::BufReader::new(stdout);
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            if line.starts_with("READY") {
                                break;
                            }
                        }
                    }
                }
                
                save_session(&socket_path, &token, &home);
                println!("export SUDO_ME_SOCK={}", socket_path.display());
                println!("export SUDO_ME_TOKEN={}", token);
            }

            #[cfg(windows)]
            {
                let pipe_name = format!(r"\\.\pipe\sudo-me-{}", token);
                let _uid = env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
                
                // Elevate self to run daemon
                askpass::elevate_self().expect("Failed to elevate");
                
                // We don't have a good way to wait for READY on Windows easily without more IPC,
                // so we'll just sleep a bit or assume it works.
                std::thread::sleep(std::time::Duration::from_millis(500));
                
                save_session(&std::path::PathBuf::from(&pipe_name), &token, &home);
                println!("set SUDO_ME_SOCK={}", pipe_name);
                println!("set SUDO_ME_TOKEN={}", token);
            }
        }
        "daemon" => {
            if args.len() < 6 {
                eprintln!("Usage: sudo-me daemon <socket_path> <token> <home> <uid>");
                std::process::exit(1);
            }
            let socket_path = std::path::Path::new(&args[2]);
            let token = &args[3];
            let home = &args[4];
            let uid = &args[5];
            
            if let Err(e) = daemon::run_daemon(socket_path, token, home, uid) {
                eprintln!("Daemon error: {}", e);
                std::process::exit(1);
            }
        }
        #[cfg(windows)]
        "daemon-windows" => {
            // On Windows, we get the token from the pipe name or env
            // For simplicity, let's assume we can find it.
            // This needs more robust implementation.
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: sudo-me run <command> [args...]");
                std::process::exit(1);
            }
            
            let (socket_path, token) = load_session();
            let command = &args[2];
            let cmd_args = args[3..].to_vec();
            
            match cli::run_command(&socket_path, &token, command, cmd_args) {
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("Error connecting to sudo-me daemon: {}", e);
                    eprintln!("The daemon might have stopped. Try running `sudo-me init` again.");
                    std::process::exit(1);
                }
            }
        }
        "approve-config" => {
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).expect("Could not find home directory");
            #[cfg(unix)]
            {
                let current_exe = env::current_exe().unwrap();
                let uid = unsafe { libc::getuid() }.to_string();
                let mut cmd = Command::new("sudo");
                cmd.env("SUDO_ASKPASS", &current_exe);
                cmd.arg("-A");
                cmd.arg(&current_exe);
                cmd.arg("approve-config-daemon");
                cmd.arg(&home);
                cmd.arg(&uid);
                let status = cmd.status().expect("Failed to run sudo");
                if status.success() {
                    println!("Configuration approved.");
                }
            }
            #[cfg(windows)]
            {
                let _home = home;
                // Similar to init, elevate to run approve-config-daemon
            }
        }
        "approve-config-daemon" => {
            if args.len() < 4 {
                eprintln!("Usage: sudo-me approve-config-daemon <home> <uid>");
                std::process::exit(1);
            }
            let home = &args[2];
            let uid = &args[3];
            daemon::approve_config(home, uid).expect("Failed to approve config");
        }
        _ => {
            // Handle sudo calling us as SUDO_ASKPASS
            match askpass::prompt_password() {
                Ok(pw) => println!("{}", pw),
                Err(e) => {
                    eprintln!("Error reading password: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn save_session(socket_path: &std::path::Path, token: &str, home: &str) {
    let session_file = std::path::PathBuf::from(home).join(".sudo-me");
    let content = format!("SUDO_ME_SOCK={}\nSUDO_ME_TOKEN={}\n", socket_path.display(), token);
    if std::fs::write(&session_file, content).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&session_file) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&session_file, perms);
            }
        }
    }
}

fn load_session() -> (String, String) {
    match (env::var("SUDO_ME_SOCK"), env::var("SUDO_ME_TOKEN")) {
        (Ok(s), Ok(t)) => (s, t),
        _ => {
            let mut loaded_sock = None;
            let mut loaded_token = None;
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).expect("Could not find home directory");
            let session_file = std::path::PathBuf::from(home).join(".sudo-me");
            if let Ok(content) = std::fs::read_to_string(&session_file) {
                for line in content.lines() {
                    if let Some(s) = line.strip_prefix("SUDO_ME_SOCK=") {
                        loaded_sock = Some(s.to_string());
                    } else if let Some(t) = line.strip_prefix("SUDO_ME_TOKEN=") {
                        loaded_token = Some(t.to_string());
                    }
                }
            }
            match (loaded_sock, loaded_token) {
                (Some(s), Some(t)) => (s, t),
                _ => {
                    eprintln!("Error: Session not found. Please run `sudo-me init` first.");
                    std::process::exit(1);
                }
            }
        }
    }
}

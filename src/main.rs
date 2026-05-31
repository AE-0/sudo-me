use sudo_me::{askpass, cli, daemon, ipc, token};
use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: sudo-me <init|run|--askpass>");
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
            let socket_dir = ipc::unix::create_secure_socket_dir().expect("Failed to create socket dir");
            let socket_path = socket_dir.join("sudo-me.sock");
            
            let current_exe = env::current_exe().unwrap();
            let home = env::var("HOME").expect("HOME not set");
            let uid = unsafe { libc::getuid() }.to_string();
            
            // Spawn daemon via sudo
            let mut cmd = Command::new("sudo");
            cmd.env("SUDO_ASKPASS", &current_exe);
            cmd.arg("-A");
            
            // Pass GUI env vars to root daemon
            if let Ok(display) = env::var("DISPLAY") { cmd.env("DISPLAY", display); }
            if let Ok(wayland) = env::var("WAYLAND_DISPLAY") { cmd.env("WAYLAND_DISPLAY", wayland); }
            if let Ok(xauth) = env::var("XAUTHORITY") { cmd.env("XAUTHORITY", xauth); }

            cmd.arg(&current_exe);
            cmd.arg("daemon");
            cmd.arg(&socket_path);
            cmd.arg(&token);
            cmd.arg(&home);
            cmd.arg(&uid);
            
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::null());
            
            let mut child = cmd.spawn().expect("Failed to spawn daemon");
            
            // Wait for READY signal from daemon
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
            
            // Save session to ~/.sudo-me
            let session_file = std::path::PathBuf::from(&home).join(".sudo-me");
            let content = format!("SUDO_ME_SOCK={}\nSUDO_ME_TOKEN={}\n", socket_path.display(), token);
            if std::fs::write(&session_file, content).is_ok() {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(&session_file) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o600);
                    let _ = std::fs::set_permissions(&session_file, perms);
                }
            }
            
            println!("export SUDO_ME_SOCK={}", socket_path.display());
            println!("export SUDO_ME_TOKEN={}", token);
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
        "approve-config" => {
            let current_exe = env::current_exe().unwrap();
            let home = env::var("HOME").expect("HOME not set");
            let uid = unsafe { libc::getuid() }.to_string();

            let mut cmd = Command::new("sudo");
            cmd.env("SUDO_ASKPASS", &current_exe);
            cmd.arg("-A");
            cmd.arg(&current_exe);
            cmd.arg("approve-config-daemon");
            cmd.arg(&home);
            cmd.arg(&uid);

            let status = cmd.status().expect("Failed to run approve-config-daemon");
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        "approve-config-daemon" => {
            if args.len() < 4 {
                eprintln!("Usage: sudo-me approve-config-daemon <home> <uid>");
                std::process::exit(1);
            }
            let home = &args[2];
            let uid = &args[3];
            if let Err(e) = daemon::approve_config(home, uid) {
                eprintln!("Error approving config: {}", e);
                std::process::exit(1);
            }
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: sudo-me run <command> [args...]");
                std::process::exit(1);
            }
            
            let (socket_path, token) = match (env::var("SUDO_ME_SOCK"), env::var("SUDO_ME_TOKEN")) {
                (Ok(s), Ok(t)) => (s, t),
                _ => {
                    let mut loaded_sock = None;
                    let mut loaded_token = None;
                    if let Ok(home) = env::var("HOME") {
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
                    }
                    match (loaded_sock, loaded_token) {
                        (Some(s), Some(t)) => (s, t),
                        _ => {
                            eprintln!("Error: Session not found. Please run `sudo-me init` first.");
                            std::process::exit(1);
                        }
                    }
                }
            };
            
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
        _ => {
            // If the argument is not a known command, it might be sudo calling us as SUDO_ASKPASS
            // sudo passes the prompt string as the first argument.
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

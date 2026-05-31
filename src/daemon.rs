use std::os::unix::net::UnixListener;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::path::{Path, PathBuf};
use crate::ipc::{IpcRequest, IpcResponse};
use crate::config::Config;
use crate::askpass;
use sha2::{Sha256, Digest};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn run_daemon(socket_path: &Path, token: &str, home_dir: &str, uid: &str) -> std::io::Result<()> {
    let config_path = PathBuf::from(home_dir).join(".config/sudo-me/config.toml");
    let config = Config::load_or_create(&config_path);
    
    // Hash config
    let config_content = std::fs::read(&config_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&config_content);
    let current_hash = format!("{:x}", hasher.finalize());
    
    let hash_dir_str = std::env::var("SUDO_ME_HASH_DIR").unwrap_or_else(|_| "/var/lib/sudo-me/hashes".to_string());
    let hash_dir = PathBuf::from(hash_dir_str);
    let hash_file = hash_dir.join(format!("{}.hash", uid));
    
    if hash_file.exists() {
        let saved_hash = std::fs::read_to_string(&hash_file)?;
        if saved_hash.trim() != current_hash {
            eprintln!("FATAL: Configuration hash mismatch!");
            eprintln!("The configuration file at {} has changed.", config_path.display());
            eprintln!("Please run `sudo-me approve-config` to authorize the new configuration.");
            if std::env::var("SUDO_ME_TEST_MODE").is_ok() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Hash mismatch"));
            }
            std::process::exit(1);
        }
    } else {
        std::fs::create_dir_all(&hash_dir)?;
        std::fs::write(&hash_file, &current_hash)?;
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hash_file)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&hash_file, perms)?;
    }

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let ttl_seconds = config.ttl_seconds;
    
    let last_activity_clone = Arc::clone(&last_activity);
    let check_interval = if ttl_seconds <= 5 { 1 } else { 10 };
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_secs(check_interval));
            let last = *last_activity_clone.lock().unwrap();
            if last.elapsed().as_secs() > ttl_seconds {
                println!("TTL expired, shutting down.");
                std::process::exit(0);
            }
        }
    });

    let listener = UnixListener::bind(socket_path)?;
    
    // Allow anyone to connect to the socket (directory permissions restrict access)
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(socket_path)?.permissions();
    perms.set_mode(0o666);
    std::fs::set_permissions(socket_path, perms)?;
    
    // Signal readiness
    println!("READY {} {}", socket_path.display(), token);
    
    let jit_confirm = config.jit_confirm;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let expected_token = token.to_string();
                let last_activity = Arc::clone(&last_activity);
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    
                    if reader.read_line(&mut line).is_ok() {
                        if let Ok(req) = serde_json::from_str::<IpcRequest>(&line) {
                            if req.token != expected_token {
                                let resp = IpcResponse::Error("Invalid token".to_string());
                                let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                                return;
                            }
                            
                            // Update last activity
                            {
                                let mut last = last_activity.lock().unwrap();
                                *last = Instant::now();
                            }

                            if jit_confirm {
                                match askpass::prompt_confirm(&req.command, &req.args) {
                                    Ok(true) => {},
                                    _ => {
                                        let resp = IpcResponse::Error("User denied command execution".to_string());
                                        let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                                        return;
                                    }
                                }
                            }
                            
                            let mut cmd = Command::new(&req.command);
                            cmd.args(&req.args);
                            cmd.stdout(Stdio::piped());
                            cmd.stderr(Stdio::piped());
                            
                            match cmd.spawn() {
                                Ok(mut child) => {
                                    let mut stdout = child.stdout.take().unwrap();
                                    let mut stderr = child.stderr.take().unwrap();
                                    let mut stream_out = stream.try_clone().unwrap();
                                    let mut stream_err = stream.try_clone().unwrap();
                                    
                                    let out_thread = thread::spawn(move || {
                                        use std::io::Read;
                                        let mut buf = [0; 1024];
                                        while let Ok(n) = stdout.read(&mut buf) {
                                            if n == 0 { break; }
                                            let out = String::from_utf8_lossy(&buf[..n]).to_string();
                                            let resp = IpcResponse::Output(out);
                                            let _ = writeln!(stream_out, "{}", serde_json::to_string(&resp).unwrap());
                                        }
                                    });
                                    
                                    let err_thread = thread::spawn(move || {
                                        use std::io::Read;
                                        let mut buf = [0; 1024];
                                        while let Ok(n) = stderr.read(&mut buf) {
                                            if n == 0 { break; }
                                            let err = String::from_utf8_lossy(&buf[..n]).to_string();
                                            let resp = IpcResponse::Error(err);
                                            let _ = writeln!(stream_err, "{}", serde_json::to_string(&resp).unwrap());
                                        }
                                    });
                                    
                                    let status = child.wait().unwrap();
                                    let _ = out_thread.join();
                                    let _ = err_thread.join();
                                    
                                    let resp = IpcResponse::Exit(status.code().unwrap_or(1));
                                    let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                                }
                                Err(e) => {
                                    let resp = IpcResponse::Error(e.to_string());
                                    let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                                }
                            }
                        }
                    }
                });
            }
            Err(err) => {
                eprintln!("Connection failed: {}", err);
                break;
            }
        }
    }
    
    Ok(())
}

pub fn approve_config(home_dir: &str, uid: &str) -> std::io::Result<()> {
    let config_path = PathBuf::from(home_dir).join(".config/sudo-me/config.toml");
    if !config_path.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Config file not found"));
    }
    
    let config_content = std::fs::read(&config_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&config_content);
    let current_hash = format!("{:x}", hasher.finalize());
    
    let hash_dir_str = std::env::var("SUDO_ME_HASH_DIR").unwrap_or_else(|_| "/var/lib/sudo-me/hashes".to_string());
    let hash_dir = PathBuf::from(hash_dir_str);
    let hash_file = hash_dir.join(format!("{}.hash", uid));
    
    std::fs::create_dir_all(&hash_dir)?;
    std::fs::write(&hash_file, &current_hash)?;
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&hash_file)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&hash_file, perms)?;
    
    println!("Configuration approved and hash saved.");
    Ok(())
}

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::sync::{Arc, Mutex};
use crate::ipc::{IpcRequest, IpcResponse};
use crate::config::Config;
use crate::askpass;
use sha2::{Sha256, Digest};

pub fn run_daemon(socket_path: &Path, token: &str, home_dir: &str, uid: &str) -> std::io::Result<()> {
    let config = crate::config::Config::load_or_create(home_dir);
    
    // Verify TOFU hash
    verify_config_hash(home_dir, uid)?;

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    
    // TTL thread
    let ttl_seconds = config.ttl_seconds;
    if ttl_seconds > 0 {
        let last_activity_clone = Arc::clone(&last_activity);
        thread::spawn(move || {
            loop {
                let elapsed = last_activity_clone.lock().unwrap().elapsed().as_secs();
                if elapsed >= ttl_seconds {
                    std::process::exit(0);
                }
                // Check more frequently for short TTLs
                let sleep_secs = if ttl_seconds <= 5 { 1 } else { 5 };
                thread::sleep(std::time::Duration::from_secs(sleep_secs));
            }
        });
    }

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixListener;
        let listener = UnixListener::bind(socket_path)?;
        
        // Allow anyone to connect to the socket (directory permissions restrict access)
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o666);
        std::fs::set_permissions(socket_path, perms)?;
        
        // Signal readiness
        println!("READY {} {}", socket_path.display(), token);
        
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let token = token.to_string();
                    let config = config.clone();
                    let last_activity = Arc::clone(&last_activity);
                    thread::spawn(move || {
                        handle_client(Box::new(stream), &token, &config, last_activity);
                    });
                }
                Err(_) => break,
            }
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
        use std::os::windows::io::FromRawHandle;
        use std::fs::File;

        let pipe_name = socket_path.to_str().unwrap();
        
        // Signal readiness
        println!("READY {} {}", pipe_name, token);

        loop {
            unsafe {
                let handle = crate::ipc::windows::create_secure_pipe(pipe_name)?;
                if ConnectNamedPipe(handle, std::ptr::null_mut()) != 0 || std::io::Error::last_os_error().raw_os_error() == Some(997) {
                    let file = File::from_raw_handle(handle as _);
                    let token = token.to_string();
                    let config = config.clone();
                    let last_activity = Arc::clone(&last_activity);
                    thread::spawn(move || {
                        handle_client(Box::new(file), &token, &config, last_activity);
                    });
                } else {
                    CloseHandle(handle);
                }
            }
        }
    }
    
    Ok(())
}

fn handle_client(mut stream: Box<dyn ReadWrite>, expected_token: &str, config: &Config, last_activity: Arc<Mutex<Instant>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    
    if reader.read_line(&mut line).is_ok() {
        *last_activity.lock().unwrap() = Instant::now();
        
        if let Ok(req) = serde_json::from_str::<IpcRequest>(&line) {
            if req.token != expected_token {
                let resp = IpcResponse::Error("Invalid token".to_string());
                let _ = writeln!(stream, "{}", serde_json::to_string(&resp).unwrap());
                return;
            }
            
            // JIT Confirmation
            if config.jit_confirm {
                match askpass::prompt_confirm(&req.command, &req.args) {
                    Ok(true) => (),
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
}

trait ReadWrite: std::io::Read + std::io::Write + Send {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWrite>>;
}

#[cfg(unix)]
impl ReadWrite for std::os::unix::net::UnixStream {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWrite>> {
        Ok(Box::new(self.try_clone()?))
    }
}

#[cfg(windows)]
impl ReadWrite for std::fs::File {
    fn try_clone(&self) -> std::io::Result<Box<dyn ReadWrite>> {
        Ok(Box::new(self.try_clone()?))
    }
}

fn get_hash_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SUDO_ME_HASH_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    { PathBuf::from("/var/db/sudo-me/hashes") }
    #[cfg(windows)]
    { 
        let appdata = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(appdata).join("sudo-me").join("hashes")
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    { PathBuf::from("/var/lib/sudo-me/hashes") }
}

fn verify_config_hash(home_dir: &str, uid: &str) -> std::io::Result<()> {
    let config_path = PathBuf::from(home_dir).join(".config").join("sudo-me").join("config.toml");
    if !config_path.exists() {
        return Ok(());
    }

    let mut hasher = Sha256::new();
    let content = std::fs::read(&config_path)?;
    hasher.update(&content);
    let current_hash = format!("{:x}", hasher.finalize());

    let hash_dir = get_hash_dir();
    let hash_file = hash_dir.join(format!("{}.hash", uid));

    if hash_file.exists() {
        let pinned_hash = std::fs::read_to_string(&hash_file)?;
        if current_hash != pinned_hash {
            if std::env::var("SUDO_ME_TEST_MODE").is_ok() {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Config hash mismatch"));
            }
            eprintln!("FATAL: Security configuration has changed since last run.");
            eprintln!("If you made this change, you must explicitly approve it by running: sudo-me approve-config");
            std::process::exit(1);
        }
    } else {
        // First run, pin the hash
        approve_config(home_dir, uid)?;
    }

    Ok(())
}

pub fn approve_config(home_dir: &str, uid: &str) -> std::io::Result<()> {
    let config_path = PathBuf::from(home_dir).join(".config").join("sudo-me").join("config.toml");
    if !config_path.exists() {
        crate::config::Config::load_or_create(home_dir);
    }

    let mut hasher = Sha256::new();
    let content = std::fs::read(&config_path)?;
    hasher.update(&content);
    let current_hash = format!("{:x}", hasher.finalize());

    let hash_dir = get_hash_dir();
    std::fs::create_dir_all(&hash_dir)?;
    
    let hash_file = hash_dir.join(format!("{}.hash", uid));
    std::fs::write(&hash_file, current_hash)?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hash_file)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&hash_file, perms)?;
    }

    Ok(())
}

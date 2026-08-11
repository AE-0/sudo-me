use sudo_me::{daemon, cli, ipc};
use std::thread;
use std::fs;
use std::time::Duration;
use std::env;
use std::sync::Mutex;

// Tests in this binary share process-global env vars (SUDO_ME_HASH_DIR,
// SUDO_ME_AUDIT_LOG, SUDO_ME_TEST_MODE, SUDO_ME_CONFIRM) and spawn
// long-running daemon threads whose cleanup removes files other tests rely
// on. Running them concurrently races the env vars and hash files, which can
// make verify_config_hash take the first-run branch and hang run_daemon in
// the listener loop. Serialize the whole binary with one lock.
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_tofu_hash_mismatch() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let socket_dir = ipc::unix::create_secure_socket_dir().unwrap();
    let socket_path = socket_dir.join("tofu.sock");
    let home_dir = socket_dir.to_str().unwrap();
    let hash_dir = socket_dir.join("hashes");
    let uid = "1001";
    let test_token = "test-token-tofu";

    // Create config file
    let config_dir = socket_dir.join(".config/sudo-me");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "jit_confirm = false\nttl_seconds = 60").unwrap();

    // Set env var for hash dir
    env::set_var("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap());

    // Approve initial config
    daemon::approve_config(home_dir, uid).unwrap();

    // Modify config file to cause mismatch
    fs::write(&config_path, "jit_confirm = true\nttl_seconds = 60").unwrap();

    // Try to run daemon - it should return an error in test mode
    env::set_var("SUDO_ME_TEST_MODE", "1");
    let result = daemon::run_daemon(&socket_path, test_token, home_dir, uid);
    assert!(result.is_err());
    env::remove_var("SUDO_ME_TEST_MODE");

    let _ = fs::remove_dir_all(socket_dir);
}

#[test]
fn test_ttl_expiration() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let socket_dir = ipc::unix::create_secure_socket_dir().unwrap();
    let socket_path = socket_dir.join("ttl.sock");
    let home_dir = socket_dir.to_str().unwrap();
    let hash_dir = socket_dir.join("hashes");
    let uid = "1002";
    let test_token = "test-token-ttl";

    // Create config file with short TTL
    let config_dir = socket_dir.join(".config/sudo-me");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "jit_confirm = false\nttl_seconds = 1").unwrap();

    env::set_var("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap());

    // Run daemon in a separate process so we can check if it exits
    let current_exe = env::current_exe().unwrap();
    let mut child = std::process::Command::new(current_exe)
        .arg("daemon")
        .arg(&socket_path)
        .arg(test_token)
        .arg(home_dir)
        .arg(uid)
        .env("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap())
        .spawn()
        .unwrap();

    // Wait for TTL to expire (1s TTL + 1s check interval + buffer)
    thread::sleep(Duration::from_secs(3));

    // Check if process has exited
    match child.try_wait().unwrap() {
        Some(status) => assert!(status.success()),
        None => {
            child.kill().unwrap();
            panic!("Daemon did not exit after TTL expired");
        }
    }

    let _ = fs::remove_dir_all(socket_dir);
}

#[test]
fn test_jit_confirmation_denial() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let socket_dir = ipc::unix::create_secure_socket_dir().unwrap();
    let socket_path = socket_dir.join("jit.sock");
    let home_dir = socket_dir.to_str().unwrap();
    let hash_dir = socket_dir.join("hashes");
    let uid = "1003";
    let test_token = "test-token-jit";

    // Create config file with jit_confirm = true
    let config_dir = socket_dir.join(".config/sudo-me");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "jit_confirm = true\nttl_seconds = 60").unwrap();

    env::set_var("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap());
    
    // Force denial via env var
    env::set_var("SUDO_ME_CONFIRM", "0");

    // Audit log at a temp path so we can assert the DENY record
    let audit_path = socket_dir.join("audit.log");
    let audit_path_str = audit_path.to_str().unwrap().to_string();
    env::set_var("SUDO_ME_AUDIT_LOG", &audit_path_str);

    let socket_path_clone = socket_path.clone();
    let home_dir_clone = home_dir.to_string();
    
    thread::spawn(move || {
        let _ = daemon::run_daemon(&socket_path_clone, test_token, &home_dir_clone, uid);
    });

    thread::sleep(Duration::from_millis(500));

    // This should return an error because of denial
    let result = cli::run_command(
        socket_path.to_str().unwrap(),
        test_token,
        "echo",
        vec!["should-not-run".to_string()]
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);

    // The denied command must be audited
    let audit = fs::read_to_string(&audit_path).unwrap();
    assert!(audit.contains("| echo | should-not-run | DENY | -"), "audit log: {}", audit);

    let _ = fs::remove_dir_all(socket_dir);
}

#[test]
fn test_audit_log_allow() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let socket_dir = ipc::unix::create_secure_socket_dir().unwrap();
    let socket_path = socket_dir.join("audit-allow.sock");
    let home_dir = socket_dir.to_str().unwrap();
    let hash_dir = socket_dir.join("hashes");
    let uid = "1004";
    let test_token = "test-token-audit-allow";

    // Create config file with jit_confirm = false
    let config_dir = socket_dir.join(".config/sudo-me");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "jit_confirm = false\nttl_seconds = 60").unwrap();

    env::set_var("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap());

    // Audit log at a temp path so we can assert the ALLOW record
    let audit_path = socket_dir.join("audit.log");
    let audit_path_str = audit_path.to_str().unwrap().to_string();
    env::set_var("SUDO_ME_AUDIT_LOG", &audit_path_str);

    let socket_path_clone = socket_path.clone();
    let home_dir_clone = home_dir.to_string();

    thread::spawn(move || {
        let _ = daemon::run_daemon(&socket_path_clone, test_token, &home_dir_clone, uid);
    });

    thread::sleep(Duration::from_millis(500));

    let result = cli::run_command(
        socket_path.to_str().unwrap(),
        test_token,
        "echo",
        vec!["hello".to_string()]
    ).unwrap();
    assert_eq!(result, 0);

    // The allowed command must be audited with its exit code
    let audit = fs::read_to_string(&audit_path).unwrap();
    assert!(audit.contains("| echo | hello | ALLOW | 0"), "audit log: {}", audit);

    let _ = fs::remove_dir_all(socket_dir);
}

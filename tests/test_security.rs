use sudo_me::{daemon, cli, ipc};
use std::thread;
use std::fs;
use std::time::Duration;
use std::env;

#[test]
fn test_tofu_hash_mismatch() {
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

    let _ = fs::remove_dir_all(socket_dir);
}

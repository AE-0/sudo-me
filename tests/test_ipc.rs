use sudo_me::{cli, token, ipc};
use std::thread;
use std::fs;
use std::time::Duration;
use std::env;

#[test]
fn test_token_generation() {
    let t = token::generate_token();
    assert_eq!(t.len(), 32);
}

#[test]
fn test_ipc_flow() {
    let socket_dir = ipc::unix::create_secure_socket_dir().unwrap();
    let socket_path = socket_dir.join("test.sock");
    let home_dir = socket_dir.to_str().unwrap();
    let hash_dir = socket_dir.join("hashes");
    let uid = "1000";
    let test_token = "test-token-1234567890123456789012";

    // Create config file
    let config_dir = socket_dir.join(".config/sudo-me");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, "jit_confirm = false\nttl_seconds = 60").unwrap();

    // Set env var for hash dir
    env::set_var("SUDO_ME_HASH_DIR", hash_dir.to_str().unwrap());

    let socket_path_clone = socket_path.clone();
    let home_dir_clone = home_dir.to_string();
    
    thread::spawn(move || {
        use sudo_me::daemon;
        // We don't care if this fails at the end of the test when the socket is deleted
        let _ = daemon::run_daemon(&socket_path_clone, test_token, &home_dir_clone, uid);
    });

    // Wait for daemon to start
    thread::sleep(Duration::from_millis(500));

    let result = cli::run_command(
        socket_path.to_str().unwrap(),
        test_token,
        "echo",
        vec!["hello".to_string()]
    ).unwrap();

    assert_eq!(result, 0);
    
    // Cleanup
    let _ = fs::remove_dir_all(socket_dir);
}

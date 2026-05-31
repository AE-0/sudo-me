use std::os::unix::net::UnixStream;
use std::io::{Write, BufRead, BufReader};
use crate::ipc::{IpcRequest, IpcResponse};

pub fn run_command(socket_path: &str, token: &str, command: &str, args: Vec<String>) -> std::io::Result<i32> {
    let mut stream = UnixStream::connect(socket_path)?;
    
    let req = IpcRequest {
        token: token.to_string(),
        command: command.to_string(),
        args,
    };
    
    let req_json = serde_json::to_string(&req).unwrap();
    writeln!(stream, "{}", req_json)?;
    
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
            match resp {
                IpcResponse::Output(out) => print!("{}", out),
                IpcResponse::Error(err) => eprintln!("{}", err),
                IpcResponse::Exit(code) => return Ok(code),
            }
        }
    }
    
    Ok(1)
}

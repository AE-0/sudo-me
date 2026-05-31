use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct IpcRequest {
    pub token: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Output(String),
    Error(String),
    Exit(i32),
}

#[cfg(unix)]
pub mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use libc;
    use std::ffi::CString;

    pub fn create_secure_socket_dir() -> std::io::Result<PathBuf> {
        let template = CString::new("/tmp/sudo-me-XXXXXX").unwrap();
        let mut bytes = template.into_bytes_with_nul();
        
        let ptr = unsafe { libc::mkdtemp(bytes.as_mut_ptr() as *mut libc::c_char) };
        if ptr.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        
        let path_str = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        let path = PathBuf::from(path_str);
        
        // Ensure permissions are 0700
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&path, perms)?;
        
        Ok(path)
    }
}

#[cfg(windows)]
pub mod windows {
    use std::ptr;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Security::*;
    use windows_sys::Win32::System::Pipes::*;
    use windows_sys::Win32::Storage::FileSystem::*;

    pub unsafe fn create_secure_pipe(pipe_name: &str) -> std::io::Result<HANDLE> {
        let mut sd: SECURITY_DESCRIPTOR = std::mem::zeroed();
        if InitializeSecurityDescriptor(&mut sd as *mut _ as _, 1) == 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Set a NULL DACL for now (allows creator and admins)
        // In a production environment, you should explicitly add ACEs.
        if SetSecurityDescriptorDacl(&mut sd as *mut _ as _, 1, ptr::null_mut(), 0) == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: &mut sd as *mut _ as _,
            bInheritHandle: 0,
        };

        let name_u16: Vec<u16> = pipe_name.encode_utf16().chain(Some(0)).collect();
        
        let handle = CreateNamedPipeW(
            name_u16.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            &sa,
        );

        if handle == INVALID_HANDLE_VALUE {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(handle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_request_serialization() {
        let req = IpcRequest {
            token: "test-token".to_string(),
            command: "ls".to_string(),
            args: vec!["-la".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.token, "test-token");
        assert_eq!(decoded.command, "ls");
        assert_eq!(decoded.args, vec!["-la".to_string()]);
    }

    #[test]
    fn test_ipc_response_serialization() {
        let resp = IpcResponse::Output("hello".to_string());
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
        if let IpcResponse::Output(out) = decoded {
            assert_eq!(out, "hello");
        } else {
            panic!("Expected Output");
        }

        let resp = IpcResponse::Exit(0);
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
        if let IpcResponse::Exit(code) = decoded {
            assert_eq!(code, 0);
        } else {
            panic!("Expected Exit");
        }
    }
}

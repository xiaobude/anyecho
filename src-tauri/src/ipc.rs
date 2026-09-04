use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::thread;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, SetNamedPipeHandleState,
    PIPE_READMODE_MESSAGE,
};
use windows::core::HSTRING;

use crate::engine::SharedEngine;


const PIPE_NAME: &str = r"\\.\pipe\anyecho_ipc";
const PIPE_BUFFER_SIZE: u32 = 64 * 1024;

#[derive(Deserialize, Serialize)]
pub struct IpcRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    200
}

#[derive(Serialize, Deserialize)]
pub struct IpcResponse {
    pub results: Vec<IpcSearchResult>,
    pub total: usize,
    pub search_time_us: u64,
}

#[derive(Serialize, Deserialize)]
pub struct IpcSearchResult {
    pub name: String,
    pub path: String,
    pub ext: String,
    pub size: u64,
    pub mtime: i64,
    pub is_directory: bool,
}

pub fn start_ipc_server(engine: SharedEngine) {
    thread::Builder::new()
        .name("anyecho-ipc-server".to_string())
        .spawn(move || {
            ipc_server_loop(engine);
        })
        .ok();
}

fn ipc_server_loop(engine: SharedEngine) {
    let pipe_name = HSTRING::from(PIPE_NAME);

    loop {
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                &pipe_name,
                windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX,
                windows::Win32::System::Pipes::PIPE_TYPE_MESSAGE
                    | windows::Win32::System::Pipes::PIPE_READMODE_MESSAGE
                    | windows::Win32::System::Pipes::PIPE_WAIT,

                1,
                PIPE_BUFFER_SIZE,
                PIPE_BUFFER_SIZE,
                0,
                None,
            )
        };

        if pipe_handle == INVALID_HANDLE_VALUE {
            tracing::warn!("Failed to create named pipe, retrying...");
            thread::sleep(std::time::Duration::from_secs(5));
            continue;
        }

        tracing::debug!("IPC server waiting for connection...");

        let _ = unsafe { ConnectNamedPipe(pipe_handle, None) };

        handle_client(pipe_handle, &engine);

        unsafe {
            let _ = DisconnectNamedPipe(pipe_handle);
            let _ = CloseHandle(pipe_handle);
        }
    }
}

fn handle_client(pipe_handle: HANDLE, engine: &SharedEngine) {
    let raw_handle = pipe_handle.0 as *mut _;
    let mut file = unsafe { File::from_raw_handle(raw_handle) };

    let mut line = String::new();
    let read_ok = {
        let mut reader = BufReader::new(&file);
        reader.read_line(&mut line).is_ok()
    };

    if read_ok {
        match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => {
                let response = execute_search(engine, &request);
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = file.write_all(json.as_bytes());
                    let _ = file.write_all(b"\n");
                    let _ = file.flush();
                }
            }
            Err(e) => {
                let err_resp = format!("{{\"error\":\"{}\"}}\n", e);
                let _ = file.write_all(err_resp.as_bytes());
                let _ = file.flush();
            }
        }
    }

    // ⚡ 修复 Windows 句柄双重释放：
    // File::drop 默认会自动对底层 raw_handle 调用 CloseHandle。
    // 此处使用 forget 转移所有权，避免双重释放，统一由 ipc_server_loop 调用 DisconnectNamedPipe 和 CloseHandle。
    std::mem::forget(file);
}

fn execute_search(engine: &SharedEngine, request: &IpcRequest) -> IpcResponse {
    let start = std::time::Instant::now();
    let eng = engine.read();

    // ⚡ 修复目录被全部吞掉问题：直接复用底层已有的 search 方法
    // 自动原生支持目录、自动应用黑名单排除规则 (Exclusions) 和大小/时间计算
    let search_res = eng.search(&request.query, 0, request.limit);

    let results: Vec<IpcSearchResult> = search_res
        .items
        .into_iter()
        .map(|f| IpcSearchResult {
            name: f.name,
            path: f.full_path,
            ext: f.ext,
            size: f.size,
            mtime: f.mtime,
            is_directory: f.is_directory,
        })
        .collect();

    IpcResponse {
        results,
        total: search_res.total_matches,
        search_time_us: start.elapsed().as_micros() as u64,
    }
}


pub fn try_ipc_search(query: &str, limit: usize) -> Option<IpcResponse> {
    let pipe_name = HSTRING::from(PIPE_NAME);

    let handle = unsafe {
        CreateFileW(
            &pipe_name,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    let Ok(handle) = handle else {
        return None;
    };

    if handle == INVALID_HANDLE_VALUE {
        return None;
    }

    unsafe {
        let mode = PIPE_READMODE_MESSAGE;
        let _ = SetNamedPipeHandleState(handle, Some(&mode), None, None);
    }

    let raw_handle = handle.0 as *mut _;
    let mut file = unsafe { File::from_raw_handle(raw_handle) };

    let request = IpcRequest {
        query: query.to_string(),
        limit,
    };

    let json = serde_json::to_string(&request).ok()?;
    let mut buf = json.into_bytes();
    buf.push(b'\n');

    if file.write_all(&buf).is_err() {
        return None;
    }
    let _ = file.flush();

    let mut reader = BufReader::new(&file);
    let mut response_line = String::new();
    if reader.read_line(&mut response_line).is_err() {
        return None;
    }

    serde_json::from_str(&response_line).ok()
}

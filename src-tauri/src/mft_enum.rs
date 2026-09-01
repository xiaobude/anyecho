use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetVolumeInformationW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const FSCTL_ENUM_USN_DATA: u32 = 0x000900B3;
const GENERIC_READ: u32 = 0x80000000;

const SYSTEM_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "$mft",
    "$mftmirr",
    "$logfile",
    "$volume",
    "$bitmap",
    "$boot",
    "$badclus",
    "$secure",
    "$upcase",
    "$extend",
];

#[repr(C)]
struct MftEnumDataV0 {
    start_file_reference_number: u64,
    low_usn: i64,
    high_usn: i64,
}

#[repr(C)]
struct UsnRecordV2 {
    record_length: u32,
    major_version: u16,
    minor_version: u16,
    file_reference_number: u64,
    parent_file_reference_number: u64,
    usn: i64,
    time_stamp: i64,
    reason: u32,
    source_info: u32,
    security_id: u32,
    file_attributes: u32,
    file_name_length: u16,
    file_name_offset: u16,
}

pub struct FileEntry {
    pub frn: u64,
    pub parent_frn: u64,
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub file_attributes: u32,
    pub is_directory: bool,
}

/// 申请提升 SeBackupPrivilege 权限
pub fn enable_backup_privilege() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_ok() {
            let mut luid = LUID::default();
            if LookupPrivilegeValueW(None, w!("SeBackupPrivilege"), &mut luid).is_ok() {
                let mut tp = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES {
                        Luid: luid,
                        Attributes: SE_PRIVILEGE_ENABLED,
                    }],
                };
                let _ = AdjustTokenPrivileges(token, false, Some(&mut tp), 0, None, None);
            }
            let _ = CloseHandle(token);
        }
    }
}

pub fn detect_ntfs_volumes() -> Result<Vec<char>, String> {
    let mut volumes = Vec::new();

    for letter_code in b'A'..=b'Z' {
        let letter = letter_code as char;
        let root = format!("{}:\\", letter);
        let root_wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        let mut fs_name = [0u16; 64];

        let result = unsafe {
            GetVolumeInformationW(
                PCWSTR(root_wide.as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut fs_name),
            )
        };

        if result.is_ok() {
            let fs_name_str = OsString::from_wide(
                &fs_name[..fs_name.iter().position(|&c| c == 0).unwrap_or(0)],
            )
            .to_string_lossy()
            .to_lowercase();

            if fs_name_str == "ntfs" {
                volumes.push(letter);
                tracing::info!("Detected NTFS volume: {}:", letter);
            }
        }
    }

    Ok(volumes)
}

fn open_volume_handle(drive_letter: &char) -> Result<HANDLE, String> {
    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let wide: Vec<u16> = volume_path.encode_utf16().chain(std::iter::once(0)).collect();

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|e| format!("Failed to open volume {}:: {}", drive_letter, e))?;

    Ok(handle)
}

pub fn enumerate_volume(drive_letter: &char) -> Result<Vec<FileEntry>, String> {
    enable_backup_privilege();

    match enumerate_volume_usn(drive_letter) {
        Ok(entries) => Ok(entries),
        Err(e) => {
            tracing::warn!(
                "USN Journal access failed for volume {}: ({}), falling back to standard directory walk...",
                drive_letter,
                e
            );
            enumerate_volume_via_fs(drive_letter)
        }
    }
}

fn enumerate_volume_usn(drive_letter: &char) -> Result<Vec<FileEntry>, String> {
    let handle = open_volume_handle(drive_letter)?;
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut skipped_system = 0usize;

    let mut enum_data = MftEnumDataV0 {
        start_file_reference_number: 0,
        low_usn: 0,
        high_usn: i64::MAX,
    };

    let mut output_buf = vec![0u8; 65536];

    loop {
        let mut bytes_returned = 0u32;

        let result = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_ENUM_USN_DATA,
                Some(&enum_data as *const _ as *const _),
                std::mem::size_of::<MftEnumDataV0>() as u32,
                Some(output_buf.as_mut_ptr() as *mut _),
                output_buf.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        if result.is_err() {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(1006) || err.raw_os_error() == Some(122) || err.raw_os_error() == Some(38) {
                break;
            }
            let _ = unsafe { CloseHandle(handle) };
            return Err(format!("DeviceIoControl failed: {}", err));
        }

        if bytes_returned <= 8 {
            break;
        }

        let next_frn = u64::from_le_bytes(output_buf[0..8].try_into().unwrap());
        if next_frn == 0 {
            break;
        }

        let mut offset = 8usize;
        while offset + std::mem::size_of::<UsnRecordV2>() <= bytes_returned as usize {
            let record = unsafe {
                &*(output_buf.as_ptr().add(offset) as *const UsnRecordV2)
            };

            if record.record_length == 0 {
                break;
            }

            let name_offset = record.file_name_offset as usize;
            let name_len = record.file_name_length as usize;

            if name_len > 0 && offset + name_offset + name_len <= bytes_returned as usize {
                let name_ptr =
                    unsafe { output_buf.as_ptr().add(offset + name_offset) as *const u16 };
                let name_slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len / 2) };
                let name = OsString::from_wide(name_slice)
                    .to_string_lossy()
                    .to_string();

                let is_dir = record.file_attributes & 0x10 != 0;
                let is_system_dir =
                    is_dir && SYSTEM_DIRS.iter().any(|&sys| name.to_lowercase() == sys);

                if !is_system_dir {
                    entries.push(FileEntry {
                        frn: record.file_reference_number,
                        parent_frn: record.parent_file_reference_number,
                        name,
                        size: 0,
                        mtime: record.time_stamp,
                        file_attributes: record.file_attributes,
                        is_directory: is_dir,
                    });
                } else {
                    skipped_system += 1;
                }
            }

            offset += record.record_length as usize;
        }

        enum_data.start_file_reference_number = next_frn;
    }

    let _ = unsafe { CloseHandle(handle) };

    tracing::info!(
        "Volume {}: {} entries enumerated via USN, {} system dirs skipped",
        drive_letter,
        entries.len(),
        skipped_system
    );

    Ok(entries)
}

/// 普通文件系统递归遍历 fallback（当非管理员无权直读 USN 卷时生效）
fn enumerate_volume_via_fs(drive_letter: &char) -> Result<Vec<FileEntry>, String> {
    let root_path = format!("{}:\\", drive_letter);
    let mut entries = Vec::new();
    let mut counter: u64 = 100;

    let mut stack = vec![(Path::new(&root_path).to_path_buf(), 5u64)];

    while let Some((dir_path, parent_frn)) = stack.pop() {

        let read_dir = match std::fs::read_dir(&dir_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for entry_res in read_dir {
            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let name = entry.file_name().to_string_lossy().to_string();
            let is_system = SYSTEM_DIRS.iter().any(|&sys| name.to_lowercase() == sys);
            if is_system {
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            let is_dir = file_type.is_dir();
            let current_frn = counter;
            counter += 1;

            let (size, mtime, attrs) = match entry.metadata() {
                Ok(m) => {
                    let sz = m.len();
                    let mt = m.modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    (sz, mt, if is_dir { 0x10 } else { 0x20 })
                }
                Err(_) => (0, 0, if is_dir { 0x10 } else { 0x20 }),
            };

            entries.push(FileEntry {
                frn: current_frn,
                parent_frn,
                name,
                size,
                mtime,
                file_attributes: attrs,
                is_directory: is_dir,
            });

            if is_dir {
                stack.push((entry.path(), current_frn));
            }
        }
    }

    tracing::info!(
        "Volume {}: {} entries enumerated via filesystem fallback",
        drive_letter,
        entries.len()
    );

    Ok(entries)
}

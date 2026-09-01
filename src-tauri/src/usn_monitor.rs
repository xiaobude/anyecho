use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, ERROR_JOURNAL_NOT_ACTIVE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;

use crate::engine::matcher::IndexedFile;
use crate::engine::pinyin::extract_pinyin;
use crate::engine::SharedEngine;

const FSCTL_CREATE_USN_JOURNAL: u32 = 0x000900E7;
const FSCTL_QUERY_USN_JOURNAL: u32 = 0x000900F4;
const FSCTL_READ_USN_JOURNAL: u32 = 0x000900FB;

const USN_REASON_FILE_CREATE: u32 = 0x00000001;
const USN_REASON_FILE_DELETE: u32 = 0x00000002;
const USN_REASON_FILE_MODIFY: u32 = 0x00000004;
const USN_REASON_RENAME_OLD_NAME: u32 = 0x00000020;
const USN_REASON_RENAME_NEW_NAME: u32 = 0x00000040;

const GAP_THRESHOLD: i64 = 1_000_000;

#[repr(C)]
struct UsnJournalV0 {
    journal_id: i64,
    low_usn: i64,
    high_usn: i64,
    next_usn: i64,
    first_usn: i64,
    max_size: i64,
    max_usn: i64,
    minimum_supported_version: u32,
}

#[repr(C)]
struct CreateUsnJournalV0 {
    max_size: i64,
    allocation_delta: i64,
}

#[repr(C)]
struct ReadUsnJournalDataV0 {
    start_usn: i64,
    reason_mask: u32,
    return_only_on_close: u32,
    timeout: u32,
    bytes_to_wait_for: u32,
    journal_id: i64,
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

#[derive(Debug, Clone)]
pub enum UsnChange {
    Create {
        frn: u64,
        parent_frn: u64,
        name: String,
        file_attributes: u32,
        mtime: i64,
        volume: char,
    },
    Delete {
        frn: u64,
    },
    Modify {
        frn: u64,
        file_attributes: u32,
        mtime: i64,
    },
    RenameOld {
        frn: u64,
        name: String,
    },
    RenameNew {
        frn: u64,
        parent_frn: u64,
        name: String,
        file_attributes: u32,
        mtime: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JournalState {
    Active,
    GapDetected { expected_usn: i64, actual_usn: i64 },
    Rescanning,
    Inactive,
}

pub struct VolumeMonitor {
    drive_letter: char,
    state: JournalState,
    next_usn: i64,
    journal_id: i64,
    shutdown: Arc<AtomicBool>,
}

impl VolumeMonitor {
    pub fn new(drive_letter: char, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            drive_letter,
            state: JournalState::Inactive,
            next_usn: 0,
            journal_id: 0,
            shutdown,
        }
    }

    pub fn run(&mut self, tx: Sender<(char, Vec<UsnChange>)>) {
        tracing::info!("[{}:] USN monitor starting", self.drive_letter);

        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                tracing::info!("[{}:] USN monitor shutting down", self.drive_letter);
                break;
            }

            match self.state {
                JournalState::Inactive => {
                    match self.ensure_journal() {
                        Ok(()) => {
                            self.state = JournalState::Active;
                            tracing::info!(
                                "[{}:] Journal active, next_usn={}",
                                self.drive_letter,
                                self.next_usn
                            );
                        }
                        Err(e) => {
                            tracing::error!("[{}:] Failed to create journal: {}", self.drive_letter, e);
                            std::thread::sleep(Duration::from_secs(5));
                        }
                    }
                }
                JournalState::Active => {
                    match self.read_changes() {
                        Ok(changes) => {
                            if !changes.is_empty() {
                                tracing::debug!(
                                    "[{}:] Read {} USN changes",
                                    self.drive_letter,
                                    changes.len()
                                );
                                let _ = tx.send((self.drive_letter, changes));
                            }
                        }
                        Err(UsnError::JournalNotActive) => {
                            tracing::warn!("[{}:] Journal not active, resetting", self.drive_letter);
                            self.state = JournalState::Inactive;
                            self.next_usn = 0;
                        }
                        Err(UsnError::Gap { expected, actual }) => {
                            tracing::warn!(
                                "[{}:] USN gap detected: expected={}, actual={}",
                                self.drive_letter,
                                expected,
                                actual
                            );
                            self.state = JournalState::GapDetected {
                                expected_usn: expected,
                                actual_usn: actual,
                            };
                        }
                        Err(UsnError::Other(e)) => {
                            tracing::error!("[{}:] Read error: {}", self.drive_letter, e);
                            std::thread::sleep(Duration::from_secs(1));
                        }
                    }
                }
                JournalState::GapDetected { .. } => {
                    tracing::info!("[{}:] Gap detected, triggering rescan", self.drive_letter);
                    self.state = JournalState::Rescanning;
                }
                JournalState::Rescanning => {
                    match self.rescan_volume(&tx) {
                        Ok(()) => {
                            self.state = JournalState::Active;
                            tracing::info!("[{}:] Rescan complete, resumed monitoring", self.drive_letter);
                        }
                        Err(e) => {
                            tracing::error!("[{}:] Rescan failed: {}", self.drive_letter, e);
                            std::thread::sleep(Duration::from_secs(5));
                        }
                    }
                }
            }
        }
    }

    fn ensure_journal(&mut self) -> Result<(), String> {
        let handle = open_volume(&self.drive_letter)?;

        match query_journal(handle) {
            Ok((journal_id, next_usn)) => {
                self.journal_id = journal_id;
                self.next_usn = next_usn;
                let _ = unsafe { CloseHandle(handle) };
                Ok(())
            }
            Err(_) => {
                tracing::info!("[{}:] Creating USN journal", self.drive_letter);
                create_journal(handle)?;
                let (journal_id, next_usn) = query_journal(handle)
                    .map_err(|e| format!("Failed to query after create: {e}"))?;
                self.journal_id = journal_id;
                self.next_usn = next_usn;
                let _ = unsafe { CloseHandle(handle) };
                Ok(())
            }
        }
    }

    fn read_changes(&mut self) -> Result<Vec<UsnChange>, UsnError> {
        let handle = open_volume(&self.drive_letter).map_err(UsnError::Other)?;
        let mut changes = Vec::new();
        let mut output_buf = vec![0u8; 65536];

        let read_data = ReadUsnJournalDataV0 {
            start_usn: self.next_usn,
            reason_mask: USN_REASON_FILE_CREATE
                | USN_REASON_FILE_DELETE
                | USN_REASON_FILE_MODIFY
                | USN_REASON_RENAME_OLD_NAME
                | USN_REASON_RENAME_NEW_NAME,
            return_only_on_close: 0,
            timeout: 500,
            bytes_to_wait_for: 0,
            journal_id: self.journal_id,
        };

        let mut bytes_returned = 0u32;

        let result = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_READ_USN_JOURNAL,
                Some(&read_data as *const _ as *const _),
                std::mem::size_of::<ReadUsnJournalDataV0>() as u32,
                Some(output_buf.as_mut_ptr() as *mut _),
                output_buf.len() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        let _ = unsafe { CloseHandle(handle) };

        if result.is_err() {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_JOURNAL_NOT_ACTIVE.0 as i32) {
                return Err(UsnError::JournalNotActive);
            }
            return Err(UsnError::Other(format!("DeviceIoControl failed: {}", err)));
        }

        if bytes_returned < 8 {
            return Ok(changes);
        }

        let new_next_usn = i64::from_le_bytes(output_buf[0..8].try_into().unwrap());

        if new_next_usn <= self.next_usn {
            return Ok(changes);
        }

        if new_next_usn - self.next_usn > GAP_THRESHOLD && self.next_usn > 0 {
            self.next_usn = new_next_usn;
            return Err(UsnError::Gap {
                expected: self.next_usn,
                actual: new_next_usn,
            });
        }

        self.next_usn = new_next_usn;

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
                let name_ptr = unsafe {
                    output_buf.as_ptr().add(offset + name_offset) as *const u16
                };
                let name_slice =
                    unsafe { std::slice::from_raw_parts(name_ptr, name_len / 2) };
                let name = OsString::from_wide(name_slice)
                    .to_string_lossy()
                    .to_string();

                let change = self.classify_change(record, &name);
                if let Some(c) = change {
                    changes.push(c);
                }
            }

            offset += record.record_length as usize;
        }

        Ok(changes)
    }

    fn classify_change(&self, record: &UsnRecordV2, name: &str) -> Option<UsnChange> {
        let reason = record.reason;

        if reason & USN_REASON_FILE_CREATE != 0 {
            Some(UsnChange::Create {
                frn: record.file_reference_number,
                parent_frn: record.parent_file_reference_number,
                name: name.to_string(),
                file_attributes: record.file_attributes,
                mtime: record.time_stamp,
                volume: self.drive_letter,
            })
        } else if reason & USN_REASON_FILE_DELETE != 0 {
            Some(UsnChange::Delete {
                frn: record.file_reference_number,
            })
        } else if reason & USN_REASON_RENAME_OLD_NAME != 0 {
            Some(UsnChange::RenameOld {
                frn: record.file_reference_number,
                name: name.to_string(),
            })
        } else if reason & USN_REASON_RENAME_NEW_NAME != 0 {
            Some(UsnChange::RenameNew {
                frn: record.file_reference_number,
                parent_frn: record.parent_file_reference_number,
                name: name.to_string(),
                file_attributes: record.file_attributes,
                mtime: record.time_stamp,
            })
        } else if reason & USN_REASON_FILE_MODIFY != 0 {
            Some(UsnChange::Modify {
                frn: record.file_reference_number,
                file_attributes: record.file_attributes,
                mtime: record.time_stamp,
            })
        } else {
            None
        }
    }

    fn rescan_volume(
        &mut self,
        _tx: &Sender<(char, Vec<UsnChange>)>,
    ) -> Result<(), String> {
        tracing::info!("[{}:] Full rescan triggered", self.drive_letter);
        let entries = crate::mft_enum::enumerate_volume(&self.drive_letter)?;
        tracing::info!(
            "[{}:] Rescan found {} entries",
            self.drive_letter,
            entries.len()
        );

        let handle = open_volume(&self.drive_letter)?;
        if let Ok((journal_id, next_usn)) = query_journal(handle) {
            self.journal_id = journal_id;
            self.next_usn = next_usn;
        }
        let _ = unsafe { CloseHandle(handle) };

        Ok(())
    }
}

enum UsnError {
    JournalNotActive,
    Gap { expected: i64, actual: i64 },
    Other(String),
}

fn open_volume(drive_letter: &char) -> Result<HANDLE, String> {
    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let wide: Vec<u16> = volume_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            0x80000000,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|e| format!("Failed to open volume {}:: {}", drive_letter, e))
}


fn query_journal(handle: HANDLE) -> Result<(i64, i64), String> {
    let mut journal_info = UsnJournalV0 {
        journal_id: 0,
        low_usn: 0,
        high_usn: 0,
        next_usn: 0,
        first_usn: 0,
        max_size: 0,
        max_usn: 0,
        minimum_supported_version: 0,
    };
    let mut bytes_returned = 0u32;

    let result = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut journal_info as *mut _ as *mut _),
            std::mem::size_of::<UsnJournalV0>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    };

    if result.is_err() {
        let err = std::io::Error::last_os_error();
        return Err(format!("Query journal failed: {}", err));
    }

    Ok((journal_info.journal_id, journal_info.next_usn))
}

fn create_journal(handle: HANDLE) -> Result<(), String> {
    let create_data = CreateUsnJournalV0 {
        max_size: 10 * 1024 * 1024,
        allocation_delta: 1 * 1024 * 1024,
    };
    let mut bytes_returned = 0u32;

    let result = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_CREATE_USN_JOURNAL,
            Some(&create_data as *const _ as *const _),
            std::mem::size_of::<CreateUsnJournalV0>() as u32,
            None,
            0,
            Some(&mut bytes_returned),
            None,
        )
    };

    if result.is_err() {
        let err = std::io::Error::last_os_error();
        return Err(format!("Create journal failed: {}", err));
    }

    Ok(())
}

pub struct UsnMonitorManager {
    shutdown: Arc<AtomicBool>,
    change_rx: Receiver<(char, Vec<UsnChange>)>,
    change_tx: Sender<(char, Vec<UsnChange>)>,
    engine: SharedEngine,
}

impl UsnMonitorManager {
    pub fn new(engine: SharedEngine) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            change_rx: rx,
            change_tx: tx,
            engine,
        }
    }

    pub fn start_monitoring(&self, volumes: Vec<char>) {
        for letter in volumes {
            let mut monitor = VolumeMonitor::new(letter, self.shutdown.clone());
            let tx = self.change_tx.clone();
            std::thread::spawn(move || {
                monitor.run(tx);
            });
        }

        let engine = self.engine.clone();
        let rx = self.change_rx.clone();
        let shutdown = self.shutdown.clone();

        std::thread::spawn(move || {
            batch_processor(engine, rx, shutdown);
        });
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::Relaxed)
    }
}

fn batch_processor(
    engine: SharedEngine,
    rx: Receiver<(char, Vec<UsnChange>)>,
    shutdown: Arc<AtomicBool>,
) {
    let mut pending: Vec<(char, UsnChange)> = Vec::new();
    let mut rename_buffer: HashMap<(char, u64), String> = HashMap::new();

    loop {
        if shutdown.load(Ordering::Relaxed) && rx.is_empty() {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((volume, changes)) => {
                for change in changes {
                    pending.push((volume, change));
                }
                while let Ok((volume, changes)) = rx.try_recv() {
                    for change in changes {
                        pending.push((volume, change));
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }

        if pending.is_empty() {
            continue;
        }

        let mut eng = engine.write();
        let batch_size = pending.len();

        for (volume, change) in pending.drain(..) {
            match change {
                UsnChange::Create {
                    frn,
                    parent_frn,
                    name,
                    file_attributes,
                    mtime,
                    volume: vol,
                } => {
                    if eng.lookup_frn(frn).is_some() {
                        continue;
                    }
                    let is_dir = file_attributes & 0x10 != 0;
                    let name_lower = name.to_lowercase();
                    let full_path_lower;
                    let full_path;

                    if let Some(parent_idx) = eng.lookup_frn(parent_frn) {
                        let parent_path = &eng.files_ref()[parent_idx].full_path;
                        full_path = format!("{}\\{}", parent_path, name);
                    } else {
                        full_path = format!("{}:\\{}", vol, name);
                    }
                    full_path_lower = full_path.to_lowercase();

                    let (pinyin_first, pinyin_full) = extract_pinyin(&name);
                    let ext = if is_dir {
                        String::new()
                    } else {
                        name.rsplit_once('.')
                            .map(|(_, e)| e.to_lowercase())
                            .unwrap_or_default()
                    };

                    eng.add_file(IndexedFile {
                        name,
                        full_path,
                        name_lower,
                        full_path_lower,
                        pinyin_first,
                        pinyin_full,
                        ext,
                        size: 0,
                        mtime,
                        is_directory: is_dir,
                        file_attributes,
                        frn,
                        parent_frn,
                        volume: vol,
                    });
                }
                UsnChange::Delete { frn } => {
                    eng.remove_file(frn);
                }
                UsnChange::Modify {
                    frn,
                    file_attributes,
                    mtime,
                } => {
                    eng.update_file(frn, None, None, Some(mtime), Some(file_attributes));
                }
                UsnChange::RenameOld { frn, name } => {
                    rename_buffer.insert((volume, frn), name);
                }
                UsnChange::RenameNew {
                    frn,
                    parent_frn,
                    name,
                    file_attributes,
                    mtime,
                } => {
                    rename_buffer.remove(&(volume, frn));
                    let old_parent = eng.lookup_frn(frn).and_then(|idx| {
                        eng.files_ref().get(idx).map(|f| f.parent_frn)
                    });
                    let parent_changed = old_parent != Some(parent_frn);
                    eng.apply_rename(
                        frn,
                        name,
                        if parent_changed { Some(parent_frn) } else { None },
                    );
                    eng.update_file(frn, None, None, Some(mtime), Some(file_attributes));
                }
            }
        }

        tracing::debug!("Applied batch of {} USN changes", batch_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_change() {
        let monitor = VolumeMonitor::new('C', Arc::new(AtomicBool::new(false)));

        let create_record = UsnRecordV2 {
            record_length: 80,
            major_version: 2,
            minor_version: 0,
            file_reference_number: 100,
            parent_file_reference_number: 5,
            usn: 1000,
            time_stamp: 1700000000,
            reason: USN_REASON_FILE_CREATE,
            source_info: 0,
            security_id: 0,
            file_attributes: 0x20, // ARCHIVE
            file_name_length: 16,
            file_name_offset: 60,
        };

        let change = monitor.classify_change(&create_record, "test.docx");
        assert!(matches!(change, Some(UsnChange::Create { frn: 100, parent_frn: 5, .. })));

        let delete_record = UsnRecordV2 {
            reason: USN_REASON_FILE_DELETE,
            file_reference_number: 100,
            ..create_record
        };
        let change_del = monitor.classify_change(&delete_record, "test.docx");
        assert!(matches!(change_del, Some(UsnChange::Delete { frn: 100 })));

        let rename_new_record = UsnRecordV2 {
            reason: USN_REASON_RENAME_NEW_NAME,
            file_reference_number: 100,
            parent_file_reference_number: 6,
            ..create_record
        };
        let change_rename = monitor.classify_change(&rename_new_record, "renamed.docx");
        assert!(matches!(change_rename, Some(UsnChange::RenameNew { frn: 100, parent_frn: 6, .. })));
    }

    #[test]
    fn test_engine_dynamic_mutations() {
        let mut engine = crate::engine::SearchEngine::new();

        engine.add_file(IndexedFile {
            name: "folder".to_string(),
            full_path: "C:\\folder".to_string(),
            name_lower: "folder".to_string(),
            full_path_lower: "c:\\folder".to_string(),
            pinyin_first: None,
            pinyin_full: None,
            ext: "".to_string(),
            size: 0,
            mtime: 0,
            is_directory: true,
            file_attributes: 0x10,
            frn: 10,
            parent_frn: 0,
            volume: 'C',
        });

        engine.add_file(IndexedFile {
            name: "child.txt".to_string(),
            full_path: "C:\\folder\\child.txt".to_string(),
            name_lower: "child.txt".to_string(),
            full_path_lower: "c:\\folder\\child.txt".to_string(),
            pinyin_first: None,
            pinyin_full: None,
            ext: "txt".to_string(),
            size: 100,
            mtime: 0,
            is_directory: false,
            file_attributes: 0x20,
            frn: 20,
            parent_frn: 10,
            volume: 'C',
        });

        assert_eq!(engine.total_count(), 2);

        // Rename folder -> "new_folder" and check path cascade on child
        engine.apply_rename(10, "new_folder".to_string(), None);
        let child_idx = engine.lookup_frn(20).unwrap();
        assert_eq!(engine.get_full_path(child_idx), Some("C:\\new_folder\\child.txt"));

        // Delete child
        engine.remove_file(20);
        assert_eq!(engine.total_count(), 1);
        assert_eq!(engine.lookup_frn(20), None);
    }
}


use std::time::Instant;
use crate::mft_enum;
use crate::path_tree::{self, RawNodeInput};

#[allow(dead_code)]
pub fn scan_all_volumes() -> Result<crate::ScanResult, String> {
    let (res, _) = scan_all_volumes_with_files()?;
    Ok(res)
}

pub fn scan_all_volumes_with_files() -> Result<(crate::ScanResult, Vec<path_tree::ResolvedFile>), String> {
    let start = Instant::now();

    let volumes = mft_enum::detect_ntfs_volumes()
        .map_err(|e| format!("Failed to detect volumes: {e}"))?;

    if volumes.is_empty() {
        return Err("No NTFS volumes found".to_string());
    }

    tracing::info!("Found {} NTFS volume(s): {:?}", volumes.len(), volumes);

    let mut all_raw_nodes: Vec<RawNodeInput> = Vec::new();

    for drive_letter in &volumes {
        tracing::info!("Scanning volume {}:", drive_letter);
        match mft_enum::enumerate_volume(drive_letter) {
            Ok(entries) => {
                tracing::info!("Volume {}: {} entries", drive_letter, entries.len());
                for entry in entries {
                    all_raw_nodes.push(RawNodeInput {
                        frn: entry.frn,
                        parent_frn: entry.parent_frn,
                        name: entry.name,
                        is_directory: entry.is_directory,
                        file_attributes: entry.file_attributes,
                        size: entry.size,
                        mtime: entry.mtime,
                        volume: *drive_letter,
                    });
                }
            }
            Err(e) => {
                tracing::error!("Failed to enumerate volume {}: {}", drive_letter, e);
            }
        }
    }


    tracing::info!("Building path tree for {} nodes...", all_raw_nodes.len());
    let resolved = path_tree::build_path_tree(all_raw_nodes);
    tracing::info!("Path tree built: {} resolved paths", resolved.len());

    let elapsed = start.elapsed().as_millis() as u64;
    tracing::info!("Scan complete: {} files in {}ms", resolved.len(), elapsed);

    Ok((
        crate::ScanResult {
            count: resolved.len(),
            time_ms: elapsed,
        },
        resolved,
    ))
}



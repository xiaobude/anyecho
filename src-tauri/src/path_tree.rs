use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub full_path: String,
    pub name: String,
    pub frn: u64,
    pub parent_frn: u64,
    pub is_directory: bool,
    pub file_attributes: u32,
    pub size: u64,
    pub mtime: i64,
    pub volume: char,
}

struct RawNode {
    frn: u64,
    parent_frn: u64,
    name: String,
    is_directory: bool,
    file_attributes: u32,
    size: u64,
    mtime: i64,
    volume: char,
}

pub fn build_path_tree(nodes: Vec<RawNodeInput>) -> Vec<ResolvedFile> {
    // 关键：以 (volume, frn) 为键，彻底隔离多盘符 (C:, D:, Z:) 间 FRN 重号冲突
    let mut frn_to_idx: HashMap<(char, u64), usize> = HashMap::with_capacity(nodes.len());
    let mut raw_nodes: Vec<RawNode> = Vec::with_capacity(nodes.len());

    for (idx, input) in nodes.into_iter().enumerate() {
        frn_to_idx.insert((input.volume, input.frn), idx);
        raw_nodes.push(RawNode {
            frn: input.frn,
            parent_frn: input.parent_frn,
            name: input.name,
            is_directory: input.is_directory,
            file_attributes: input.file_attributes,
            size: input.size,
            mtime: input.mtime,
            volume: input.volume,
        });
    }

    let mut resolved = Vec::with_capacity(raw_nodes.len());

    for node in &raw_nodes {
        let name_trim = node.name.trim();
        // 过滤掉根目录自身（如 "." 或 "C:"）
        if name_trim == "." || name_trim == format!("{}:", node.volume) || name_trim.is_empty() {
            continue;
        }

        let mut path_parts: Vec<&str> = vec![&node.name];
        let mut current_frn = node.parent_frn;
        let mut depth = 0u32;

        while depth < 256 {
            let frn_low = current_frn & 0x0000_FFFF_FFFF_FFFF;
            // NTFS 根目录 FRN 通常为 5
            if frn_low == 0 || frn_low == 0x0000_FFFF_FFFF_FFFF || frn_low == 5 {
                break;
            }
            if current_frn == node.frn {
                break;
            }

            if let Some(&parent_idx) = frn_to_idx.get(&(node.volume, current_frn)) {
                let parent = &raw_nodes[parent_idx];
                let p_name = parent.name.trim();
                if p_name != "." && p_name != format!("{}:", node.volume) && !p_name.is_empty() {
                    path_parts.push(&parent.name);
                }
                let parent_pfrn_low = parent.parent_frn & 0x0000_FFFF_FFFF_FFFF;
                if parent.parent_frn == current_frn || parent.parent_frn == parent.frn || parent_pfrn_low == 5 || parent_pfrn_low == 0 {
                    break;
                }
                current_frn = parent.parent_frn;
            } else {
                break;
            }
            depth += 1;
        }

        path_parts.reverse();
        let sub_path = path_parts.join("\\");
        let full_path = if sub_path.is_empty() {
            format!("{}:\\", node.volume)
        } else {
            format!("{}:\\{}", node.volume, sub_path)
        };

        resolved.push(ResolvedFile {
            full_path,
            name: node.name.clone(),
            frn: node.frn,
            parent_frn: node.parent_frn,
            is_directory: node.is_directory,
            file_attributes: node.file_attributes,
            size: node.size,
            mtime: node.mtime,
            volume: node.volume,
        });
    }

    resolved
}

pub struct RawNodeInput {
    pub frn: u64,
    pub parent_frn: u64,
    pub name: String,
    pub is_directory: bool,
    pub file_attributes: u32,
    pub size: u64,
    pub mtime: i64,
    pub volume: char,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_volume_path_tree_isolation() {
        let nodes = vec![
            // C 盘上的文件夹与文件
            RawNodeInput {
                frn: 10,
                parent_frn: 5,
                name: "Users".to_string(),
                is_directory: true,
                file_attributes: 16,
                size: 0,
                mtime: 0,
                volume: 'C',
            },
            RawNodeInput {
                frn: 20,
                parent_frn: 10,
                name: "test.txt".to_string(),
                is_directory: false,
                file_attributes: 32,
                size: 100,
                mtime: 0,
                volume: 'C',
            },
            // Z 盘上具有相同 FRN (10, 20) 的完全不同文件
            RawNodeInput {
                frn: 10,
                parent_frn: 5,
                name: "Photos".to_string(),
                is_directory: true,
                file_attributes: 16,
                size: 0,
                mtime: 0,
                volume: 'Z',
            },
            RawNodeInput {
                frn: 20,
                parent_frn: 10,
                name: "photo.jpg".to_string(),
                is_directory: false,
                file_attributes: 32,
                size: 200,
                mtime: 0,
                volume: 'Z',
            },
        ];

        let resolved = build_path_tree(nodes);
        assert_eq!(resolved.len(), 4);

        let c_file = resolved.iter().find(|f| f.name == "test.txt").unwrap();
        assert_eq!(c_file.full_path, "C:\\Users\\test.txt");

        let z_file = resolved.iter().find(|f| f.name == "photo.jpg").unwrap();
        assert_eq!(z_file.full_path, "Z:\\Photos\\photo.jpg");
    }
}

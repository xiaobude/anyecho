export interface SearchItem {
  name: string;
  full_path: string;
  size: number;
  mtime: number;
  is_directory: boolean;
  ext: string;
}

export interface SearchResponse {
  items: SearchItem[];
  total_matches: number;
  total_files: number;
  total_bytes: number;
  search_time_us: number;
}


export interface ScanResult {
  count: number;
  time_ms: number;
}

export type FilterCategory = 'all' | 'ai' | 'folder' | 'doc' | 'image' | 'video' | 'audio' | 'app' | 'archive';

export interface ColumnWidths {
  index: number;
  name: number;
  type: number;
  path: number;
  size: number;
  date: number;
}



export interface FilterOption {
  id: FilterCategory;
  label: string;
  icon: string;
  queryPrefix: string;
}

export interface ContentMatch {
  file_path: string;
  file_name: string;
  line_number: number;
  line_text: string;
  match_start: number;
  match_end: number;
}

export interface ContentSearchResponse {
  matches: ContentMatch[];
  files_searched: number;
  total_matches: number;
  search_time_us: number;
  is_complete: boolean;
}

export interface PreviewLine {
  line_number: number;
  text: string;
  is_match: boolean;
}

export interface ContentPreview {
  file_path: string;
  lines: PreviewLine[];
  keyword: string;
}

export interface SearchHistoryEntry {
  id: number;
  query: string;
  result_count: number;
  searched_at: number;
}

export interface Favorite {
  id: number;
  file_path: string;
  file_name: string;
  added_at: number;
}

export interface ExclusionRule {
  id: number;
  pattern: string;
  is_regex: boolean;
  created_at: number;
}

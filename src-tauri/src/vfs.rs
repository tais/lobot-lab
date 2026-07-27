use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

use crate::model::DataRootDto;

#[derive(Clone, Debug)]
pub struct Vfs {
    roots: Vec<PathBuf>,
    archives: Vec<Vec<SlfArchive>>,
}

#[derive(Clone, Debug)]
struct SlfArchive {
    path: PathBuf,
    entries: HashMap<String, SlfEntry>,
}

#[derive(Clone, Copy, Debug)]
struct SlfEntry {
    offset: u64,
    length: usize,
}

impl Vfs {
    pub fn new(roots: Vec<String>) -> Result<Self, String> {
        if roots.is_empty() {
            return Err("Choose at least one JA2 Data directory".into());
        }
        let mut seen = HashSet::new();
        let mut resolved = Vec::new();
        for root in roots {
            let path = PathBuf::from(root);
            if !path.is_dir() {
                return Err(format!(
                    "Data root does not exist or is not a directory: {}",
                    path.display()
                ));
            }
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            if seen.insert(canonical.clone()) {
                resolved.push(canonical);
            }
        }
        let mut archives = Vec::new();
        for root in &resolved {
            let mut root_archives = Vec::new();
            let Ok(entries) = fs::read_dir(root) else {
                archives.push(root_archives);
                continue;
            };
            let mut slf_paths: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("slf"))
                })
                .collect();
            slf_paths.sort();
            for path in slf_paths {
                if let Ok(archive) = SlfArchive::open(path) {
                    root_archives.push(archive);
                }
            }
            archives.push(root_archives);
        }
        Ok(Self {
            roots: resolved,
            archives,
        })
    }

    pub fn root_dtos(&self) -> Vec<DataRootDto> {
        self.roots
            .iter()
            .map(|path| DataRootDto {
                path: path.to_string_lossy().into_owned(),
                label: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Data")
                    .to_string(),
            })
            .collect()
    }

    pub fn exists(&self, virtual_path: &str) -> bool {
        self.roots.iter().enumerate().rev().any(|(index, root)| {
            case_insensitive_join(root, virtual_path).is_some_and(|path| path.is_file())
                || self.archive_entry_at(index, virtual_path).is_some()
        })
    }

    pub fn read(&self, virtual_path: &str) -> Result<(Vec<u8>, PathBuf), String> {
        for (index, root) in self.roots.iter().enumerate().rev() {
            if let Some(path) =
                case_insensitive_join(root, virtual_path).filter(|path| path.is_file())
            {
                let bytes = fs::read(&path)
                    .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
                return Ok((bytes, path));
            }
            if let Some((archive, entry)) = self.archive_entry_at(index, virtual_path) {
                let mut file = File::open(&archive.path).map_err(|error| {
                    format!("Could not open {}: {error}", archive.path.display())
                })?;
                return file
                    .seek(SeekFrom::Start(entry.offset))
                    .and_then(|_| {
                        let mut bytes = vec![0; entry.length];
                        file.read_exact(&mut bytes).map(|_| bytes)
                    })
                    .map(|bytes| (bytes, archive.path.clone()))
                    .map_err(|error| {
                        format!(
                            "Could not read {virtual_path} from {}: {error}",
                            archive.path.display()
                        )
                    });
            }
        }
        Err(format!(
            "File not found in the active Data roots or SLF libraries: {virtual_path}"
        ))
    }

    pub fn read_text(&self, virtual_path: &str) -> Result<(String, PathBuf), String> {
        let (bytes, path) = self.read(virtual_path)?;
        let text = String::from_utf8_lossy(&bytes)
            .trim_start_matches('\u{feff}')
            .to_string();
        Ok((text, path))
    }

    fn archive_entry_at(
        &self,
        root_index: usize,
        virtual_path: &str,
    ) -> Option<(&SlfArchive, SlfEntry)> {
        let key = normalize_virtual_path(virtual_path);
        self.archives
            .get(root_index)?
            .iter()
            .rev()
            .find_map(|archive| {
                archive
                    .entries
                    .get(&key)
                    .copied()
                    .map(|entry| (archive, entry))
            })
    }
}

impl SlfArchive {
    fn open(path: PathBuf) -> Result<Self, String> {
        const HEADER_SIZE: u64 = 532;
        const ENTRY_SIZE: u64 = 280;
        let mut file = File::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let length = file
            .metadata()
            .map_err(|error| format!("{}: {error}", path.display()))?
            .len();
        if length < HEADER_SIZE {
            return Err(format!(
                "{} is too small to be an SLF library",
                path.display()
            ));
        }
        let mut header = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut header)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let count = i32::from_le_bytes(header[512..516].try_into().unwrap());
        if count < 0 || count > 1_000_000 {
            return Err(format!("{} has an invalid SLF entry count", path.display()));
        }
        let table_size = count as u64 * ENTRY_SIZE;
        if table_size > length.saturating_sub(HEADER_SIZE) {
            return Err(format!("{} has a truncated SLF directory", path.display()));
        }
        let mount = c_string(&header[256..512]).replace('\\', "/");
        file.seek(SeekFrom::Start(length - table_size))
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut entries = HashMap::new();
        let mut raw = [0u8; ENTRY_SIZE as usize];
        for _ in 0..count {
            file.read_exact(&mut raw)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let state = u32::from_le_bytes(raw[264..268].try_into().unwrap());
            if state != 0 {
                continue;
            }
            let name = c_string(&raw[..256]).replace('\\', "/");
            if name.is_empty() {
                continue;
            }
            let offset = u32::from_le_bytes(raw[256..260].try_into().unwrap()) as u64;
            let entry_length = u32::from_le_bytes(raw[260..264].try_into().unwrap()) as usize;
            if offset.saturating_add(entry_length as u64) > length {
                continue;
            }
            entries.insert(
                normalize_virtual_path(&format!("{mount}/{name}")),
                SlfEntry {
                    offset,
                    length: entry_length,
                },
            );
        }
        Ok(Self { path, entries })
    }
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn normalize_virtual_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

fn case_insensitive_join(root: &Path, virtual_path: &str) -> Option<PathBuf> {
    let normalized = virtual_path.replace('\\', "/");
    let mut current = root.to_path_buf();
    for component in Path::new(&normalized).components() {
        let wanted = match component {
            Component::Normal(value) => value.to_str()?,
            Component::CurDir => continue,
            _ => return None,
        };
        let exact = current.join(wanted);
        if exact.exists() {
            current = exact;
            continue;
        }
        let wanted_lower = wanted.to_lowercase();
        let entry = fs::read_dir(&current)
            .ok()?
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().to_lowercase() == wanted_lower)?;
        current = entry.path();
    }
    Some(current)
}

pub fn discover_data_roots(install_path: &str) -> Result<Vec<String>, String> {
    let selected = PathBuf::from(install_path);
    if !selected.is_dir() {
        return Err(format!("Not a directory: {}", selected.display()));
    }
    let selected_data_root = selected.join("TableData").is_dir();
    let selected_name = selected
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let standard_data_root =
        selected_data_root && matches!(selected_name.as_str(), "data" | "data-1.13");
    if selected_data_root && !standard_data_root {
        return Ok(vec![canonical_string(&selected)]);
    }
    let install = if standard_data_root {
        selected.parent().unwrap_or(&selected).to_path_buf()
    } else {
        selected.clone()
    };

    let mut configs: Vec<PathBuf> = fs::read_dir(&install)
        .map_err(|error| format!("Could not inspect {}: {error}", install.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.to_lowercase().starts_with("vfs_config")
                        && name.to_lowercase().ends_with(".ini")
                })
        })
        .collect();
    configs.sort_by_key(|path| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if name.contains("ja2113") {
            0
        } else {
            1
        }
    });

    if let Some(config) = configs.first() {
        if let Ok(contents) = fs::read_to_string(config) {
            let discovered = roots_from_vfs_ini(&install, &contents);
            let selected_is_in_profile = !standard_data_root
                || discovered
                    .iter()
                    .any(|root| root == &canonical_string(&selected));
            if !discovered.is_empty() && selected_is_in_profile {
                return Ok(discovered);
            }
        }
    }

    let mut candidates: Vec<PathBuf> = fs::read_dir(&install)
        .map_err(|error| format!("Could not inspect {}: {error}", install.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("TableData").is_dir())
        .collect();
    candidates.sort_by_key(|path| {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        match name.as_str() {
            "data" => (0, name),
            "data-1.13" => (1, name),
            _ => (2, name),
        }
    });
    if candidates.is_empty() {
        return Err("No Data directory containing TableData was found".into());
    }
    Ok(candidates
        .iter()
        .map(|path| canonical_string(path))
        .collect())
}

fn roots_from_vfs_ini(install: &Path, contents: &str) -> Vec<String> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_lowercase();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let profiles = sections
        .get("vfs_config")
        .and_then(|section| section.get("profiles"))
        .map(|value| split_list(value))
        .unwrap_or_default();
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for profile in profiles {
        let profile_key = format!("profile_{}", profile.to_lowercase());
        let profile_section = sections.get(&profile_key);
        let locations = profile_section
            .and_then(|section| section.get("locations"))
            .map(|value| split_list(value))
            .unwrap_or_default();
        let profile_root = profile_section
            .and_then(|section| section.get("profile_root"))
            .map(String::as_str)
            .unwrap_or("");
        for location in locations {
            let location_key = format!("loc_{}", location.to_lowercase());
            let Some(section) = sections.get(&location_key) else {
                continue;
            };
            if !section
                .get("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("directory"))
            {
                continue;
            }
            let path = section.get("path").map(String::as_str).unwrap_or("");
            let candidate = install
                .join(profile_root.replace('\\', "/"))
                .join(path.replace('\\', "/"));
            if candidate.is_dir() {
                let canonical = fs::canonicalize(&candidate).unwrap_or(candidate);
                if seen.insert(canonical.clone()) {
                    roots.push(canonical.to_string_lossy().into_owned());
                }
            }
        }
    }
    roots
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn canonical_string(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfs_ini_preserves_profile_order() {
        let data = r#"
            [vfs_config]
            PROFILES = Base, Mod
            [PROFILE_Base]
            LOCATIONS = base
            [PROFILE_Mod]
            LOCATIONS = mod
            [LOC_base]
            TYPE = DIRECTORY
            PATH = Data
            [LOC_mod]
            TYPE = DIRECTORY
            PATH = Data-Mod
        "#;
        let scratch = std::env::temp_dir().join(format!("lobot-vfs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(scratch.join("Data/TableData")).unwrap();
        fs::create_dir_all(scratch.join("Data-Mod/TableData")).unwrap();
        let roots = roots_from_vfs_ini(&scratch, data);
        assert!(roots[0].ends_with("Data"));
        assert!(roots[1].ends_with("Data-Mod"));
        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    fn selecting_standard_data_directory_discovers_sibling_profile_roots() {
        let scratch = std::env::temp_dir().join(format!("lobot-data-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(scratch.join("Data/TableData")).unwrap();
        fs::create_dir_all(scratch.join("Data-1.13/TableData")).unwrap();
        fs::write(
            scratch.join("vfs_config.JA2113.ini"),
            r#"
                [vfs_config]
                PROFILES = Vanilla, v113
                [PROFILE_Vanilla]
                LOCATIONS = base
                [PROFILE_v113]
                LOCATIONS = v113
                [LOC_base]
                TYPE = DIRECTORY
                PATH = Data
                [LOC_v113]
                TYPE = DIRECTORY
                PATH = Data-1.13
            "#,
        )
        .unwrap();

        let roots = discover_data_roots(scratch.join("Data").to_str().unwrap()).unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots[0].ends_with("Data"));
        assert!(roots[1].ends_with("Data-1.13"));
        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    fn reads_files_from_slf_libraries() {
        const HEADER_SIZE: usize = 532;
        const ENTRY_SIZE: usize = 280;
        let scratch = std::env::temp_dir().join(format!("lobot-slf-{}", std::process::id()));
        let data = scratch.join("Data");
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&data).unwrap();

        let payload = b"SLF works";
        let mut bytes = vec![0u8; HEADER_SIZE + payload.len() + ENTRY_SIZE];
        bytes[..9].copy_from_slice(b"TEST.SLF\0");
        bytes[256..263].copy_from_slice(b"Anims\\\0");
        bytes[512..516].copy_from_slice(&1i32.to_le_bytes());
        bytes[HEADER_SIZE..HEADER_SIZE + payload.len()].copy_from_slice(payload);
        let entry = HEADER_SIZE + payload.len();
        bytes[entry..entry + 9].copy_from_slice(b"test.txt\0");
        bytes[entry + 256..entry + 260].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        bytes[entry + 260..entry + 264].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        fs::write(data.join("Test.slf"), bytes).unwrap();

        let vfs = Vfs::new(vec![data.to_string_lossy().into_owned()]).unwrap();
        let (read, source) = vfs.read("ANIMS/test.txt").unwrap();
        assert_eq!(read, payload);
        assert!(source.ends_with("Test.slf"));
        let _ = fs::remove_dir_all(&scratch);
    }
}

//! Bounded CPA zip extraction. Rejects traversal, duplicates, symlinks,
//! reparse points, and oversized archives.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

use super::CpaRuntimeError;

const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub fn extract_zip(archive: &Path, destination: &Path) -> Result<(), CpaRuntimeError> {
    let metadata = fs::metadata(archive).map_err(io_error)?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(CpaRuntimeError::Invalid(
            "CPA release archive exceeds 64 MiB".into(),
        ));
    }
    if is_reparse(archive) {
        return Err(CpaRuntimeError::Invalid(
            "CPA release archive must not be a reparse point".into(),
        ));
    }
    if destination.exists() {
        return Err(CpaRuntimeError::Invalid(
            "CPA extract destination already exists".into(),
        ));
    }
    fs::create_dir_all(destination).map_err(io_error)?;
    if is_reparse(destination) {
        let _ = fs::remove_dir_all(destination);
        return Err(CpaRuntimeError::Invalid(
            "CPA extract destination must not be a reparse point".into(),
        ));
    }

    let file = File::open(archive).map_err(io_error)?;
    let mut zip = ZipArchive::new(file).map_err(|error| {
        CpaRuntimeError::Invalid(format!("CPA release archive is not a valid zip: {error}"))
    })?;
    if zip.len() > MAX_ENTRIES {
        let _ = fs::remove_dir_all(destination);
        return Err(CpaRuntimeError::Invalid(
            "CPA release archive has too many entries".into(),
        ));
    }

    let mut seen = HashMap::new();
    let mut total_uncompressed = 0u64;
    let result = extract_entries(&mut zip, destination, &mut seen, &mut total_uncompressed);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_entries<R: Read + io::Seek>(
    zip: &mut ZipArchive<R>,
    destination: &Path,
    seen: &mut HashMap<String, bool>,
    total_uncompressed: &mut u64,
) -> Result<(), CpaRuntimeError> {
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| {
            CpaRuntimeError::Invalid(format!("failed to read CPA archive entry: {error}"))
        })?;
        if entry.is_symlink() || is_unix_symlink(entry.unix_mode()) {
            return Err(CpaRuntimeError::Invalid(
                "CPA release archive must not contain symlinks".into(),
            ));
        }
        let relative = match entry.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => {
                return Err(CpaRuntimeError::Invalid(
                    "CPA release archive contains an unsafe path".into(),
                ));
            }
        };
        let relative = normalize_relative(&relative)?;
        let is_dir = entry.is_dir() || entry.name().ends_with('/');
        let key = windows_path_key(&relative);
        if seen.contains_key(&key) {
            return Err(CpaRuntimeError::Invalid(
                "CPA release archive contains duplicate entries".into(),
            ));
        }
        let mut ancestor = String::new();
        for component in key
            .split('/')
            .take(key.split('/').count().saturating_sub(1))
        {
            if !ancestor.is_empty() {
                ancestor.push('/');
            }
            ancestor.push_str(component);
            if seen.get(&ancestor) == Some(&false) {
                return Err(CpaRuntimeError::Invalid(
                    "CPA release archive contains a file/directory collision".into(),
                ));
            }
        }
        if !is_dir
            && seen
                .keys()
                .any(|seen_key| seen_key.starts_with(&format!("{key}/")))
        {
            return Err(CpaRuntimeError::Invalid(
                "CPA release archive contains a file/directory collision".into(),
            ));
        }
        seen.insert(key, is_dir);
        let out_path = destination.join(&relative);
        if !out_path.starts_with(destination) {
            return Err(CpaRuntimeError::Invalid(
                "CPA release archive contains an unsafe path".into(),
            ));
        }
        if is_dir {
            fs::create_dir_all(&out_path).map_err(io_error)?;
            if is_reparse(&out_path) {
                return Err(CpaRuntimeError::Invalid(
                    "CPA extract path resolved to a reparse point".into(),
                ));
            }
            continue;
        }
        let uncompressed = entry.size();
        if uncompressed > MAX_FILE_BYTES {
            return Err(CpaRuntimeError::Invalid(
                "CPA release file exceeds 128 MiB".into(),
            ));
        }
        *total_uncompressed = total_uncompressed
            .checked_add(uncompressed)
            .filter(|total| *total <= MAX_UNCOMPRESSED_BYTES)
            .ok_or_else(|| {
                CpaRuntimeError::Invalid("CPA release uncompressed size exceeds 256 MiB".into())
            })?;
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
            if is_reparse(parent) {
                return Err(CpaRuntimeError::Invalid(
                    "CPA extract path resolved to a reparse point".into(),
                ));
            }
        }
        let mut output = File::create(&out_path).map_err(io_error)?;
        let copied =
            io::copy(&mut entry.by_ref().take(uncompressed + 1), &mut output).map_err(io_error)?;
        output.flush().map_err(io_error)?;
        drop(output);
        if copied != uncompressed {
            return Err(CpaRuntimeError::Invalid(
                "CPA release file size did not match the archive entry".into(),
            ));
        }
        if is_reparse(&out_path) {
            return Err(CpaRuntimeError::Invalid(
                "CPA extract path resolved to a reparse point".into(),
            ));
        }
    }
    Ok(())
}

fn normalize_relative(path: &Path) -> Result<PathBuf, CpaRuntimeError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_str().ok_or_else(|| {
                    CpaRuntimeError::Invalid("CPA archive path is not Unicode".into())
                })?;
                if is_unsafe_windows_component(text) {
                    return Err(CpaRuntimeError::Invalid(
                        "CPA release archive contains an unsafe path".into(),
                    ));
                }
                out.push(part);
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(CpaRuntimeError::Invalid(
                    "CPA release archive contains an unsafe path".into(),
                ));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(CpaRuntimeError::Invalid(
            "CPA release archive contains an empty path".into(),
        ));
    }
    Ok(out)
}

fn windows_path_key(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_unsafe_windows_component(text: &str) -> bool {
    if text.is_empty()
        || matches!(text, "." | "..")
        || text.ends_with(['.', ' '])
        || text.contains(['\0', ':'])
        || text.chars().any(|ch| ch.is_control())
    {
        return true;
    }
    let stem = text.split('.').next().unwrap_or(text).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

pub(crate) fn is_unix_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

fn is_reparse(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        fs::symlink_metadata(path)
            .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    }
}

fn io_error(error: io::Error) -> CpaRuntimeError {
    CpaRuntimeError::Failed(format!("CPA extract failed: {error}"))
}

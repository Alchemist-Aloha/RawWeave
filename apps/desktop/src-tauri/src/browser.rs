use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use exif::{In, Reader as ExifReader, Tag, Value};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const MAX_DIRECTORY_ENTRIES: usize = 100_000;
const MAX_PAGE_SIZE: usize = 256;
const MAX_FILE_READ_BYTES: usize = 16 * 1024 * 1024;
const MAX_XMP_BYTES: usize = 512 * 1024;
const MAX_SESSION_BYTES: usize = 2 * 1024 * 1024;
const MAX_THUMBNAIL_EDGE: u32 = 512;
const MAX_IMAGE_EDGE: u32 = 16_384;
const MAX_IMAGE_ALLOC: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserFlag {
    None,
    Pick,
    Reject,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserMetadataDto {
    width: u32,
    height: u32,
    camera: Option<String>,
    lens: Option<String>,
    iso: Option<u32>,
    aperture: Option<f32>,
    shutter: Option<f32>,
    focal_length: Option<f32>,
    capture_time: Option<String>,
    orientation: Option<String>,
    exif: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEntryDto {
    path: String,
    name: String,
    kind: String,
    extension: String,
    size: u64,
    modified_time: Option<String>,
    rating: Option<u8>,
    flag: BrowserFlag,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPageDto {
    path: String,
    entries: Vec<BrowserEntryDto>,
    offset: usize,
    next_offset: Option<usize>,
    has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFileInspectionDto {
    metadata: Option<BrowserMetadataDto>,
    thumbnail: Option<String>,
    rating: Option<u8>,
    flag: BrowserFlag,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationResultDto {
    path: String,
    previous_path: Option<String>,
}

fn reject_parent_components(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err("path traversal is not allowed".to_owned());
    }
    Ok(())
}

fn canonical_existing(path: &str, expected: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err(format!("{expected} path cannot be empty"));
    }
    let requested = Path::new(path);
    reject_parent_components(requested)?;
    let canonical = requested.canonicalize().map_err(|error| {
        format!(
            "could not access {expected} '{}': {error}",
            requested.display()
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        format!(
            "could not inspect {expected} '{}': {error}",
            canonical.display()
        )
    })?;
    if expected == "directory" && !metadata.is_dir() {
        return Err(format!("'{}' is not a directory", canonical.display()));
    }
    if expected == "file" && !metadata.is_file() {
        return Err(format!("'{}' is not a file", canonical.display()));
    }
    Ok(canonical)
}

fn canonical_file(path: &str) -> Result<PathBuf, String> {
    canonical_existing(path, "file")
}

fn canonical_directory(path: &str) -> Result<PathBuf, String> {
    canonical_existing(path, "directory")
}

fn validate_leaf_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("file name cannot be empty".to_owned());
    }
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err("file name must be a single path component".to_owned());
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path)
        .map_err(|error| format!("could not open '{}': {error}", path.display()))?;
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| "file size limit overflowed".to_owned())?;
    let mut reader = file.take(read_limit as u64);
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read '{}': {error}", path.display()))?;
    if bytes.len() > max_bytes {
        return Err(format!(
            "'{}' exceeds the {max_bytes}-byte read limit",
            path.display()
        ));
    }
    Ok(bytes)
}

fn modified_time(metadata: &fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
}

fn xmp_path(path: &Path) -> PathBuf {
    path.with_extension("xmp")
}

fn extract_xmp_attribute(content: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=");
    let start = content.find(&marker)? + marker.len();
    let quote = content.as_bytes().get(start).copied()? as char;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value_start = start + 1;
    let end = content[value_start..].find(quote)? + value_start;
    Some(content[value_start..end].to_owned())
}

fn parse_xmp_marks(content: &str) -> (Option<u8>, BrowserFlag) {
    let rating = extract_xmp_attribute(content, "xmp:Rating")
        .and_then(|value| value.parse::<i16>().ok())
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| *value <= 5);
    let label = extract_xmp_attribute(content, "xmp:Label")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let flag = if rating.is_some_and(|value| value == 0) || label == "reject" || label == "red" {
        BrowserFlag::Reject
    } else if label == "pick" || label == "select" || label == "green" {
        BrowserFlag::Pick
    } else {
        BrowserFlag::None
    };
    (rating.filter(|value| *value > 0), flag)
}

fn read_xmp_marks(path: &Path) -> Result<(Option<u8>, BrowserFlag), String> {
    let sidecar = xmp_path(path);
    match read_bounded(&sidecar, MAX_XMP_BYTES) {
        Ok(bytes) => Ok(parse_xmp_marks(&String::from_utf8_lossy(&bytes))),
        Err(_error) if !sidecar.exists() => Ok((None, BrowserFlag::None)),
        Err(error) => Err(error),
    }
}

fn xmp_label(flag: &BrowserFlag) -> &'static str {
    match flag {
        BrowserFlag::None => "",
        BrowserFlag::Pick => "pick",
        BrowserFlag::Reject => "reject",
    }
}

fn write_xmp_marks(path: &Path, rating: Option<u8>, flag: BrowserFlag) -> Result<(), String> {
    if rating.is_some_and(|value| value > 5) {
        return Err("rating must be between 0 and 5".to_owned());
    }
    let sidecar = xmp_path(path);
    let content = format!(
        "<?xpacket begin=\"﻿\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmp:Rating=\"{}\" xmp:Label=\"{}\"/></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
        rating.unwrap_or(0),
        xmp_label(&flag),
    );
    if content.len() > MAX_XMP_BYTES {
        return Err("XMP sidecar exceeds the size limit".to_owned());
    }
    let temp = sidecar.with_extension(format!("xmp.{}.tmp", std::process::id()));
    fs::write(&temp, content.as_bytes())
        .map_err(|error| format!("could not write XMP sidecar '{}': {error}", temp.display()))?;
    fs::rename(&temp, &sidecar).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!(
            "could not install XMP sidecar '{}': {error}",
            sidecar.display()
        )
    })
}

fn exif_string(exif: &exif::Exif, tag: Tag) -> Option<String> {
    exif.get_field(tag, In::PRIMARY)
        .map(|field| field.display_value().with_unit(exif).to_string())
        .filter(|value| !value.trim().is_empty())
}

fn exif_number(exif: &exif::Exif, tag: Tag) -> Option<f32> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        Value::Rational(values) => values
            .first()
            .and_then(|value| (value.denom != 0).then_some(value.num as f32 / value.denom as f32)),
        Value::SRational(values) => values
            .first()
            .and_then(|value| (value.denom != 0).then_some(value.num as f32 / value.denom as f32)),
        Value::Float(values) => values.first().copied(),
        Value::Double(values) => values.first().map(|value| *value as f32),
        _ => None,
    }
}

fn read_exif(bytes: &[u8]) -> Option<exif::Exif> {
    let mut reader = BufReader::new(Cursor::new(bytes));
    ExifReader::new().read_from_container(&mut reader).ok()
}

fn exif_map(exif: &exif::Exif) -> BTreeMap<String, String> {
    exif.fields()
        .take(512)
        .map(|field| {
            (
                format!("{:?}", field.tag),
                field.display_value().with_unit(exif).to_string(),
            )
        })
        .collect()
}

fn bounded_image(bytes: Vec<u8>) -> Result<image::DynamicImage, String> {
    let mut reader = image::ImageReader::new(BufReader::new(Cursor::new(bytes)))
        .with_guessed_format()
        .map_err(|error| format!("could not identify image format: {error}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_IMAGE_EDGE);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|error| format!("could not decode image: {error}"))
}

fn ordinary_metadata(_path: &Path, bytes: &[u8]) -> Option<BrowserMetadataDto> {
    let image = bounded_image(bytes.to_vec()).ok()?;
    let exif = read_exif(bytes);
    Some(BrowserMetadataDto {
        width: image.width(),
        height: image.height(),
        camera: exif.as_ref().and_then(|value| {
            let make = exif_string(value, Tag::Make);
            let model = exif_string(value, Tag::Model);
            match (make, model) {
                (Some(make), Some(model)) => Some(format!("{make} {model}")),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            }
        }),
        lens: exif
            .as_ref()
            .and_then(|value| exif_string(value, Tag::LensModel)),
        iso: exif
            .as_ref()
            .and_then(|value| value.get_field(Tag::PhotographicSensitivity, In::PRIMARY))
            .and_then(|field| field.value.get_uint(0)),
        aperture: exif
            .as_ref()
            .and_then(|value| exif_number(value, Tag::FNumber)),
        shutter: exif
            .as_ref()
            .and_then(|value| exif_number(value, Tag::ExposureTime)),
        focal_length: exif
            .as_ref()
            .and_then(|value| exif_number(value, Tag::FocalLength)),
        capture_time: exif
            .as_ref()
            .and_then(|value| exif_string(value, Tag::DateTimeOriginal)),
        orientation: exif
            .as_ref()
            .and_then(|value| exif_string(value, Tag::Orientation)),
        exif: exif.as_ref().map(exif_map).unwrap_or_default(),
    })
}

fn thumbnail_data_url(bytes: Vec<u8>) -> Result<String, String> {
    let image = bounded_image(bytes)?;
    let thumbnail = image.thumbnail(MAX_THUMBNAIL_EDGE, MAX_THUMBNAIL_EDGE);
    let mut encoded = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .map_err(|error| format!("could not encode thumbnail: {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(encoded.into_inner());
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

fn is_raw_file(path: &Path) -> bool {
    crate::is_raw_path(path)
}

fn inspect_file_at(path: &Path) -> Result<BrowserFileInspectionDto, String> {
    let (rating, flag) = read_xmp_marks(path).unwrap_or((None, BrowserFlag::None));
    if is_raw_file(path) {
        let bytes = crate::read_raw_file(path, rawweave_raw::RawDecodeLimits::default())?;
        let frame = rawweave_raw::RawloaderDecoder::default()
            .decode(&bytes)
            .map_err(|error| format!("could not decode RAW metadata: {error}"))?;
        let metadata = crate::raw_metadata(&frame);
        return Ok(BrowserFileInspectionDto {
            metadata: Some(BrowserMetadataDto {
                width: metadata.dimensions.width,
                height: metadata.dimensions.height,
                camera: Some(metadata.camera),
                lens: metadata.lens,
                iso: metadata.iso,
                aperture: metadata.aperture,
                shutter: metadata.shutter,
                focal_length: metadata.focal_length,
                capture_time: metadata.capture_time,
                orientation: Some(metadata.orientation),
                exif: metadata.exif,
            }),
            thumbnail: None,
            rating,
            flag,
        });
    }
    let bytes = read_bounded(path, MAX_FILE_READ_BYTES)?;
    let metadata = ordinary_metadata(path, &bytes);
    let thumbnail = metadata
        .as_ref()
        .and_then(|_| thumbnail_data_url(bytes).ok());
    Ok(BrowserFileInspectionDto {
        metadata,
        thumbnail,
        rating,
        flag,
    })
}

fn entry_at(path: &Path) -> Result<BrowserEntryDto, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect '{}': {error}", path.display()))?;
    let is_directory = metadata.is_dir();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (rating, flag) = if is_directory {
        (None, BrowserFlag::None)
    } else {
        read_xmp_marks(path).unwrap_or((None, BrowserFlag::None))
    };
    Ok(BrowserEntryDto {
        path: path.to_string_lossy().into_owned(),
        name,
        kind: if is_directory { "directory" } else { "file" }.to_owned(),
        extension,
        size: metadata.len(),
        modified_time: modified_time(&metadata),
        rating,
        flag,
    })
}

fn list_directory_at(path: &Path, offset: usize, limit: usize) -> Result<DirectoryPageDto, String> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(format!(
            "directory page size must be between 1 and {MAX_PAGE_SIZE}"
        ));
    }
    if offset > MAX_DIRECTORY_ENTRIES {
        return Err("directory offset exceeds the safety limit".to_owned());
    }
    let mut children = Vec::new();
    for child in fs::read_dir(path)
        .map_err(|error| format!("could not read '{}': {error}", path.display()))?
    {
        let child = child.map_err(|error| format!("could not read directory entry: {error}"))?;
        if children.len() >= MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "directory exceeds the {MAX_DIRECTORY_ENTRIES}-entry safety limit"
            ));
        }
        children.push(child.path());
    }
    children.sort_by(|left, right| {
        left.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(
                &right
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            )
    });
    let end = offset.saturating_add(limit).min(children.len());
    let entries = children
        .get(offset..end)
        .unwrap_or_default()
        .iter()
        .filter_map(|path| entry_at(path).ok())
        .collect::<Vec<_>>();
    let has_more = end < children.len();
    Ok(DirectoryPageDto {
        path: path.to_string_lossy().into_owned(),
        entries,
        offset,
        next_offset: has_more.then_some(end),
        has_more,
    })
}

fn destination_path(source: &Path, destination: &str) -> Result<PathBuf, String> {
    let directory = canonical_directory(destination)?;
    let name = source
        .file_name()
        .ok_or_else(|| "source file has no name".to_owned())?;
    let target = directory.join(name);
    if target.exists() {
        return Err(format!("destination '{}' already exists", target.display()));
    }
    Ok(target)
}

fn operation_result(path: PathBuf, previous_path: Option<PathBuf>) -> FileOperationResultDto {
    FileOperationResultDto {
        path: path.to_string_lossy().into_owned(),
        previous_path: previous_path.map(|path| path.to_string_lossy().into_owned()),
    }
}

fn copy_sidecar(source: &Path, destination: &Path) -> Result<(), String> {
    let sidecar = xmp_path(source);
    if sidecar.is_file() {
        fs::copy(&sidecar, xmp_path(destination))
            .map_err(|error| format!("could not copy XMP sidecar: {error}"))?;
    }
    Ok(())
}

fn move_sidecar(source: &Path, destination: &Path) -> Result<(), String> {
    let sidecar = xmp_path(source);
    if sidecar.is_file() {
        fs::rename(&sidecar, xmp_path(destination))
            .map_err(|error| format!("could not move XMP sidecar: {error}"))?;
    }
    Ok(())
}

fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let result = Command::new("gio").arg("open").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    result
        .map(|_| ())
        .map_err(|error| format!("could not reveal file in the system file manager: {error}"))
}

fn trash_with_os(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let status = Command::new("gio").arg("trash").arg(path).status();
    #[cfg(target_os = "macos")]
    let status = Command::new("osascript")
        .arg("-e")
        .arg("on run argv\n tell application \"Finder\" to delete POSIX file (item 1 of argv)\nend run")
        .arg(path)
        .status();
    #[cfg(target_os = "windows")]
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($args[0], 'Delete', 'SendToRecycleBin')")
        .arg(path)
        .status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("system trash command exited with {status}")),
        Err(error) => Err(format!("OS trash is unavailable: {error}")),
    }
}

fn save_session_at(path: &Path, session: &str) -> Result<(), String> {
    if session.len() > MAX_SESSION_BYTES {
        return Err(format!(
            "session exceeds the {MAX_SESSION_BYTES}-byte limit"
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "session path has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create session directory: {error}"))?;
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|error| format!("could not create temporary session file: {error}"))?;
    file.write_all(session.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("could not write session: {error}"))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("could not install session: {error}")
    })
}

fn load_session_at(path: &Path) -> Result<Option<String>, String> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > MAX_SESSION_BYTES {
                return Err(format!(
                    "session exceeds the {MAX_SESSION_BYTES}-byte limit"
                ));
            }
            String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| format!("session is not valid UTF-8: {error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("could not read session: {error}")),
    }
}

fn session_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|error| format!("could not resolve application config directory: {error}"))?
        .join("browser-session.json"))
}

#[tauri::command]
pub fn list_directory(
    path: String,
    offset: usize,
    limit: usize,
) -> Result<DirectoryPageDto, String> {
    let path = canonical_directory(&path)?;
    list_directory_at(&path, offset, limit)
}

#[tauri::command]
pub fn inspect_file(path: String) -> Result<BrowserFileInspectionDto, String> {
    inspect_file_at(&canonical_file(&path)?)
}

#[tauri::command]
pub fn set_file_marks(path: String, rating: Option<u8>, flag: BrowserFlag) -> Result<(), String> {
    let path = canonical_file(&path)?;
    write_xmp_marks(&path, rating, flag)
}

#[tauri::command]
pub fn rename_file(path: String, name: String) -> Result<FileOperationResultDto, String> {
    validate_leaf_name(&name)?;
    let source = canonical_file(&path)?;
    let target = source
        .parent()
        .ok_or_else(|| "source file has no parent directory".to_owned())?
        .join(name);
    if target.exists() {
        return Err(format!("destination '{}' already exists", target.display()));
    }
    fs::rename(&source, &target).map_err(|error| format!("could not rename file: {error}"))?;
    move_sidecar(&source, &target)?;
    Ok(operation_result(target, Some(source)))
}

#[tauri::command]
pub fn move_file(path: String, destination: String) -> Result<FileOperationResultDto, String> {
    let source = canonical_file(&path)?;
    let target = destination_path(&source, &destination)?;
    fs::rename(&source, &target).map_err(|error| format!("could not move file: {error}"))?;
    move_sidecar(&source, &target)?;
    Ok(operation_result(target, Some(source)))
}

#[tauri::command]
pub fn copy_file(path: String, destination: String) -> Result<FileOperationResultDto, String> {
    let source = canonical_file(&path)?;
    let target = destination_path(&source, &destination)?;
    fs::copy(&source, &target).map_err(|error| format!("could not copy file: {error}"))?;
    copy_sidecar(&source, &target)?;
    Ok(operation_result(target, None))
}

#[tauri::command]
pub fn reveal_file(path: String) -> Result<(), String> {
    reveal_in_file_manager(&canonical_file(&path)?)
}

#[tauri::command]
pub fn trash_file(path: String) -> Result<(), String> {
    let path = canonical_file(&path)?;
    trash_with_os(&path)
}

#[tauri::command]
pub fn save_session(app: AppHandle, session: String) -> Result<(), String> {
    save_session_at(&session_path(&app)?, &session)
}

#[tauri::command]
pub fn load_session(app: AppHandle) -> Result<Option<String>, String> {
    load_session_at(&session_path(&app)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_parent_components_in_user_supplied_names() {
        assert!(validate_leaf_name("../outside").is_err());
        assert!(validate_leaf_name("nested/name.jpg").is_err());
        assert!(validate_leaf_name("safe-name.jpg").is_ok());
    }

    #[test]
    fn directory_listing_is_bounded_and_paginated() {
        let root = temp_fixture("list");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("a.jpg"), b"a").unwrap();
        fs::write(root.join("b.jpg"), b"b").unwrap();

        let page = list_directory_at(&root, 1, 1).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.offset, 1);
        assert_eq!(page.next_offset, Some(2));
        assert!(page.has_more);
        remove_fixture(&root);
    }

    #[test]
    fn xmp_marks_round_trip_without_touching_the_image() {
        let root = temp_fixture("xmp");
        let image = root.join("photo.jpg");
        fs::write(&image, b"not an image").unwrap();
        write_xmp_marks(&image, Some(5), BrowserFlag::Pick).unwrap();
        assert_eq!(
            read_xmp_marks(&image).unwrap(),
            (Some(5), BrowserFlag::Pick)
        );
        assert_eq!(fs::read(&image).unwrap(), b"not an image");
        remove_fixture(&root);
    }

    #[test]
    fn session_file_rejects_oversized_payloads_and_round_trips() {
        let root = temp_fixture("session");
        let path = root.join("session.json");
        save_session_at(&path, "{\"version\":1}").unwrap();
        assert_eq!(
            load_session_at(&path).unwrap().as_deref(),
            Some("{\"version\":1}")
        );
        assert!(save_session_at(&path, &"x".repeat(MAX_SESSION_BYTES + 1)).is_err());
        remove_fixture(&root);
    }

    fn temp_fixture(label: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("rawweave-browser-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn remove_fixture(path: &std::path::Path) {
        let _ = fs::remove_dir_all(path);
    }
}

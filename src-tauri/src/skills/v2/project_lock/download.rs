use super::valid_component;
use crate::skills::{installer::TempInstallDir, v2::fsutil};
use flate2::read::MultiGzDecoder;
use reqwest::{redirect::Policy, Client, Url};
use std::collections::{hash_map::Entry, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

const MAX_DOWNLOAD: usize = 10 * 1024 * 1024;
const MAX_CONTENT: u64 = 25 * 1024 * 1024;
const MAX_TAR: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 1000;

pub(super) fn fetch(url: &Url) -> Result<(PathBuf, TempInstallDir), String> {
    let client = Client::builder()
        .https_only(true)
        .referer(false)
        .timeout(Duration::from_secs(30))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() > 10
                || attempt.url().scheme() != "https"
                || !attempt.url().username().is_empty()
                || attempt.url().password().is_some()
            {
                return attempt.error("Unsupported Skill download redirect");
            }
            attempt.follow()
        }))
        .build()
        .map_err(download_error)?;
    let bytes = tauri::async_runtime::block_on(read_download(&client, url.clone()))?;
    stage(&bytes)
}

async fn read_download(client: &Client, url: Url) -> Result<Vec<u8>, String> {
    let mut response = client.get(url).send().await.map_err(download_error)?;
    if !response.status().is_success() {
        return Err(format!(
            "Skill download returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_DOWNLOAD as u64)
    {
        return Err("Skill download exceeds 10 MiB".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(download_error)? {
        if chunk.len() > MAX_DOWNLOAD - bytes.len() {
            return Err("Skill download exceeds 10 MiB".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn download_error(error: reqwest::Error) -> String {
    format!(
        "Could not download the locked Skill: {}",
        error.without_url()
    )
}

pub(super) fn stage(bytes: &[u8]) -> Result<(PathBuf, TempInstallDir), String> {
    let temporary = TempInstallDir::create_in(&std::env::temp_dir())?;
    let root = temporary.path().join("download");
    fs::create_dir(&root).map_err(archive_error)?;
    if is_skill_markdown(bytes) {
        fs::write(root.join("SKILL.md"), bytes).map_err(archive_error)?;
        return Ok((root, temporary));
    }

    let mut contents = ArchiveStage {
        root: &root,
        paths: HashMap::new(),
        bytes: 0,
    };
    if bytes.starts_with(b"PK") {
        unpack_zip(bytes, &mut contents)?;
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut uncompressed = Vec::new();
        MultiGzDecoder::new(bytes)
            .take(MAX_TAR + 1)
            .read_to_end(&mut uncompressed)
            .map_err(archive_error)?;
        if uncompressed.len() as u64 > MAX_TAR {
            return Err("Uncompressed Skill TAR exceeds 32 MiB".to_string());
        }
        unpack_tar(&uncompressed, &mut contents)?;
    } else {
        unpack_tar(bytes, &mut contents)?;
    }
    let children = fs::read_dir(&root)
        .map_err(archive_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(archive_error)?;
    let mut visible = children
        .into_iter()
        .filter(|item| item.file_name() != "__MACOSX");
    let result = match (visible.next(), visible.next()) {
        (None, _) => return Err("The Skill download was empty".to_string()),
        (Some(only), None) if only.file_type().map_err(archive_error)?.is_dir() => only.path(),
        _ => root,
    };
    Ok((result, temporary))
}

fn is_skill_markdown(bytes: &[u8]) -> bool {
    let Ok(markdown) = std::str::from_utf8(bytes) else {
        return false;
    };
    let metadata = fsutil::parse_frontmatter_text(markdown.trim_start_matches('\u{feff}'));
    ["name", "description"].into_iter().all(|field| {
        metadata
            .get(field)
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn unpack_zip(bytes: &[u8], contents: &mut ArchiveStage<'_>) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(archive_error)?;
    check_entry_count(archive.len())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(archive_error)?;
        let is_dir = entry.is_dir();
        let mode = entry.unix_mode().unwrap_or_default();
        let allowed_mode = if is_dir { 0o040000 } else { 0o100000 };
        if entry.encrypted() || (mode & 0o170000 != 0 && mode & 0o170000 != allowed_mode) {
            return Err("Skill archives must contain regular files and directories".to_string());
        }
        let path = std::str::from_utf8(entry.name_raw())
            .map_err(archive_error)?
            .to_string();
        let size = entry.size();
        contents.write_entry(&path, is_dir, size, mode, &mut entry)?;
    }
    Ok(())
}

fn unpack_tar(bytes: &[u8], contents: &mut ArchiveStage<'_>) -> Result<(), String> {
    let mut archive = tar::Archive::new(bytes);
    for (index, entry) in archive.entries().map_err(archive_error)?.enumerate() {
        check_entry_count(index + 1)?;
        let mut entry = entry.map_err(archive_error)?;
        let kind = entry.header().entry_type();
        if !kind.is_dir() && !kind.is_file() {
            return Err("Skill archives must contain regular files and directories".to_string());
        }
        let path = std::str::from_utf8(&entry.path_bytes())
            .map_err(archive_error)?
            .to_string();
        let size = entry.size();
        let mode = entry.header().mode().map_err(archive_error)?;
        contents.write_entry(&path, kind.is_dir(), size, mode, &mut entry)?;
    }
    Ok(())
}

fn check_entry_count(count: usize) -> Result<(), String> {
    if count > MAX_ENTRIES {
        return Err("Skill archive exceeds 1000 entries".to_string());
    }
    Ok(())
}

struct ArchiveStage<'a> {
    root: &'a Path,
    paths: HashMap<String, (String, bool)>,
    bytes: u64,
}

impl ArchiveStage<'_> {
    fn write_entry(
        &mut self,
        name: &str,
        is_dir: bool,
        size: u64,
        mode: u32,
        reader: &mut impl Read,
    ) -> Result<(), String> {
        let name = name.strip_prefix("./").unwrap_or(name);
        if is_dir && matches!(name, "" | ".") && size == 0 {
            return Ok(());
        }
        let name = if is_dir {
            name.trim_end_matches('/')
        } else {
            name
        };
        if name.len() > 1024
            || name.split('/').count() > 32
            || name
                .split('/')
                .any(|part| !valid_component(part) || part.eq_ignore_ascii_case(".git"))
        {
            return Err("Skill archive contains a nonportable path".to_string());
        }
        for (prefix, directory) in name
            .match_indices('/')
            .map(|(index, _)| (&name[..index], true))
            .chain([(name, is_dir)])
        {
            match self.paths.entry(prefix.to_lowercase()) {
                Entry::Vacant(entry) => {
                    entry.insert((prefix.to_string(), directory));
                }
                Entry::Occupied(entry) if directory && entry.get().1 && entry.get().0 == prefix => {
                }
                Entry::Occupied(_) => {
                    return Err("Skill archive contains conflicting paths".to_string());
                }
            }
        }

        let path = self.root.join(name);
        if is_dir {
            if size != 0 {
                return Err("Skill archive directory contains file data".to_string());
            }
            return fs::create_dir_all(&path).map_err(archive_error);
        }
        if size > MAX_CONTENT - self.bytes {
            return Err("Uncompressed Skill archive exceeds 25 MiB".to_string());
        }
        fs::create_dir_all(path.parent().ok_or("Invalid Skill archive entry")?)
            .map_err(archive_error)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(archive_error)?;
        if io::copy(&mut reader.take(size + 1), &mut file).map_err(archive_error)? != size {
            return Err("Skill archive entry size does not match its header".to_string());
        }
        self.bytes += size;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o644 | (mode & 0o111)))
                .map_err(archive_error)?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        Ok(())
    }
}

fn archive_error(error: impl std::fmt::Display) -> String {
    format!("Could not read the downloaded Skill archive: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    const SKILL: &[u8] = b"---\nname: downloaded\ndescription: Test Skill\n---\n# Downloaded\n";

    fn tar_skill() -> Vec<u8> {
        let mut archive = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(SKILL.len() as u64);
        header.set_mode(0o644);
        archive
            .append_data(&mut header, "package/example/SKILL.md", SKILL)
            .unwrap();
        archive.into_inner().unwrap()
    }

    #[test]
    fn stages_markdown_and_both_tar_formats_without_an_extension() {
        let tar = tar_skill();
        let mut compressed = GzEncoder::new(Vec::new(), Compression::fast());
        compressed.write_all(&tar).unwrap();
        let gzip = compressed.finish().unwrap();
        for (payload, path) in [
            (SKILL, "SKILL.md"),
            (tar.as_slice(), "example/SKILL.md"),
            (gzip.as_slice(), "example/SKILL.md"),
        ] {
            let (root, _temporary) = stage(payload).unwrap();
            assert_eq!(fs::read(root.join(path)).unwrap(), SKILL);
        }
        assert!(stage(b"<html><title>Not a Skill</title></html>").is_err());
    }

    #[test]
    fn rejects_archive_links_and_case_collisions() {
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        tar.append_link(&mut header, "shortcut", "SKILL.md")
            .unwrap();
        assert!(stage(&tar.into_inner().unwrap())
            .map(|_| ())
            .unwrap_err()
            .contains("regular files"));

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["pack/SKILL.md", "Pack/notes.md"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(SKILL).unwrap();
        }
        assert!(stage(&zip.finish().unwrap().into_inner())
            .map(|_| ())
            .unwrap_err()
            .contains("conflicting paths"));
    }

    #[test]
    fn rejects_gzip_that_grows_past_the_tar_limit() {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
        io::copy(&mut io::repeat(0).take(MAX_TAR + 1), &mut gzip).unwrap();
        assert!(stage(&gzip.finish().unwrap())
            .map(|_| ())
            .unwrap_err()
            .contains("exceeds 32 MiB"));
    }

    #[test]
    fn checks_download_size_before_reading_the_body() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut request = String::new();
            while !request.ends_with("\r\n\r\n") {
                assert_ne!(reader.read_line(&mut request).unwrap(), 0);
            }
            assert!(request.starts_with("GET /download HTTP/1.1\r\n"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_DOWNLOAD + 1
            )
            .unwrap();
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = Url::parse(&format!("http://{address}/download")).unwrap();
        let result = tauri::async_runtime::block_on(read_download(&client, url));
        assert_eq!(result.unwrap_err(), "Skill download exceeds 10 MiB");
        worker.join().unwrap();
    }
}

use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use anyhow::{anyhow, Context};
use flate2::read::GzDecoder;

use crate::{
    config::LogAnalyzerSettings,
    fs_utils::{relative_string, safe_join},
    models::{GrepMatch, GrepResults, ManifestFile},
};

pub struct LogAnalyzer {
    settings: LogAnalyzerSettings,
}

impl LogAnalyzer {
    pub fn new(settings: LogAnalyzerSettings) -> Self {
        Self { settings }
    }

    pub fn extract_upload(&self, raw_path: &Path, extracted_dir: &Path) -> anyhow::Result<()> {
        let name = raw_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            let file = fs::File::open(raw_path)?;
            let decoder = GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            for entry in archive.entries()? {
                let mut entry = entry?;
                let entry_path = entry.path()?.to_path_buf();
                let safe_path = safe_join(extracted_dir, &entry_path)?;
                if let Some(parent) = safe_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                entry.unpack(safe_path)?;
            }
            return Ok(());
        }

        if name.ends_with(".zip") {
            let file = fs::File::open(raw_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index)?;
                let Some(enclosed) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
                    continue;
                };
                let safe_path = safe_join(extracted_dir, &enclosed)?;
                if entry.is_dir() {
                    fs::create_dir_all(&safe_path)?;
                } else {
                    if let Some(parent) = safe_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut out = fs::File::create(&safe_path)?;
                    std::io::copy(&mut entry, &mut out)?;
                }
            }
            return Ok(());
        }

        let filename = raw_path
            .file_name()
            .ok_or_else(|| anyhow!("raw upload missing filename"))?;
        fs::copy(raw_path, extracted_dir.join(filename))?;
        Ok(())
    }

    pub fn collect_manifest_files(&self, root: &Path) -> anyhow::Result<Vec<ManifestFile>> {
        let mut files = Vec::new();
        collect_manifest_files_inner(root, root, &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    pub fn run_simple_grep(&self, root: &Path) -> anyhow::Result<GrepResults> {
        let lower_keywords: Vec<String> = self
            .settings
            .keywords
            .iter()
            .map(|keyword| keyword.to_ascii_lowercase())
            .collect();
        let mut matches = Vec::new();
        grep_dir(
            root,
            root,
            &lower_keywords,
            self.settings.max_matches,
            &mut matches,
        )?;
        Ok(GrepResults {
            keywords: self.settings.keywords.clone(),
            total_matches: matches.len(),
            matches,
        })
    }
}

fn collect_manifest_files_inner(
    root: &Path,
    dir: &Path,
    files: &mut Vec<ManifestFile>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_manifest_files_inner(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(ManifestFile {
                path: relative_string(root, &path)?,
                size: metadata.len(),
            });
        }
    }
    Ok(())
}

fn grep_dir(
    root: &Path,
    dir: &Path,
    keywords: &[String],
    max_matches: usize,
    matches: &mut Vec<GrepMatch>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            grep_dir(root, &path, keywords, max_matches, matches)?;
        } else if metadata.is_file() {
            grep_file(root, &path, keywords, max_matches, matches)
                .with_context(|| format!("failed to grep {}", path.display()))?;
        }
        if matches.len() >= max_matches {
            break;
        }
    }
    Ok(())
}

fn grep_file(
    root: &Path,
    path: &Path,
    keywords: &[String],
    max_matches: usize,
    matches: &mut Vec<GrepMatch>,
) -> anyhow::Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for (line_index, line) in reader.lines().enumerate() {
        if matches.len() >= max_matches {
            return Ok(());
        }
        let Ok(line) = line else {
            continue;
        };
        let lower = line.to_ascii_lowercase();
        if let Some(keyword) = keywords
            .iter()
            .find(|keyword| lower.contains(keyword.as_str()))
        {
            matches.push(GrepMatch {
                file: relative_string(root, path)?,
                line: line_index + 1,
                keyword: keyword.clone(),
                text: line.chars().take(500).collect(),
            });
        }
    }
    Ok(())
}

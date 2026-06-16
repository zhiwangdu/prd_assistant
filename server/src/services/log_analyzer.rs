use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, Context};
use flate2::read::GzDecoder;
use serde_json::json;
use tracing::warn;

use crate::{
    domain::models::{GrepMatch, GrepResults, LogGroupSummary, ManifestFile, ToolInputEntry},
    support::{
        config::LogAnalyzerSettings,
        fs_utils::{relative_string, safe_join},
    },
};

const LOG_PACKAGE_SUFFIX: &str = "_logs.tar.gz";
const INFLUXQL_ANALYZER_ID: &str = "influxql_analyzer";

pub struct LogAnalyzer {
    settings: LogAnalyzerSettings,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractionResult {
    pub preprocessed: Option<PreprocessedUpload>,
}

#[derive(Debug, Clone)]
pub struct LogPackageName {
    pub package_id: String,
    pub instance_id: String,
    pub node_id: String,
    pub package_timestamp: String,
}

#[derive(Debug, Clone)]
pub struct PreprocessedUpload {
    pub package_id: String,
    pub instance_id: String,
    pub node_id: String,
    pub package_timestamp: String,
    pub node_dir: String,
    pub log_groups: Vec<LogGroupSummary>,
    pub ignored_file_count: u64,
    pub ignored_path_samples: Vec<String>,
    pub warnings: Vec<String>,
    pub files: Vec<PreprocessedFile>,
    pub tool_inputs: Vec<ToolInputEntry>,
}

#[derive(Debug, Clone)]
pub struct PreprocessedFile {
    pub output_relative_path: String,
    pub original_path: String,
    pub log_group: String,
    pub compressed: bool,
    pub compression: Option<String>,
}

impl LogAnalyzer {
    pub fn new(settings: LogAnalyzerSettings) -> Self {
        Self { settings }
    }

    pub fn extract_upload(
        &self,
        raw_path: &Path,
        extracted_dir: &Path,
        tool_inputs_dir: Option<&Path>,
    ) -> anyhow::Result<ExtractionResult> {
        let name = raw_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        if let Some(package) = parse_log_package_filename(&name) {
            let tool_inputs_dir = tool_inputs_dir.ok_or_else(|| {
                anyhow!("tool_inputs_dir is required for log package preprocessing")
            })?;
            return preprocess_log_package(raw_path, extracted_dir, tool_inputs_dir, package).map(
                |preprocessed| ExtractionResult {
                    preprocessed: Some(preprocessed),
                },
            );
        }

        let lower_name = name.to_ascii_lowercase();

        if lower_name.ends_with(".tar.gz") || lower_name.ends_with(".tgz") {
            let file = fs::File::open(raw_path)?;
            let decoder = GzDecoder::new(file);
            if let Err(gzip_err) = unpack_tar_archive(decoder, extracted_dir) {
                reset_dir(extracted_dir)?;
                let file = fs::File::open(raw_path)?;
                unpack_tar_archive(file, extracted_dir).with_context(|| {
                    format!("failed to extract as gzip tar: {gzip_err}; fallback tar also failed")
                })?;
            }
            return Ok(ExtractionResult::default());
        }

        if lower_name.ends_with(".tar") {
            let file = fs::File::open(raw_path)?;
            unpack_tar_archive(file, extracted_dir)?;
            return Ok(ExtractionResult::default());
        }

        if lower_name.ends_with(".zip") {
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
            return Ok(ExtractionResult::default());
        }

        let filename = raw_path
            .file_name()
            .ok_or_else(|| anyhow!("raw upload missing filename"))?;
        fs::copy(raw_path, extracted_dir.join(filename))?;
        Ok(ExtractionResult::default())
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

pub fn read_log_slice(
    path: &Path,
    start_line: usize,
    end_line: usize,
) -> anyhow::Result<Vec<(usize, String)>> {
    let mut reader = log_file_reader(path)?;
    let mut lines = Vec::new();
    let mut buffer = Vec::new();
    let mut line_number = 0_usize;
    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        line_number += 1;
        if line_number >= start_line && line_number <= end_line {
            lines.push((line_number, bytes_to_line(&buffer)));
        }
        if line_number > end_line {
            break;
        }
    }
    Ok(lines)
}

pub fn parse_log_package_filename(filename: &str) -> Option<LogPackageName> {
    let lower = filename.to_ascii_lowercase();
    if !lower.ends_with(LOG_PACKAGE_SUFFIX) {
        return None;
    }
    let stem = &filename[..filename.len() - LOG_PACKAGE_SUFFIX.len()];
    let parts = stem.split('_').collect::<Vec<_>>();
    if parts.len() != 10 {
        return None;
    }
    let [package_id, instance_id, node_id, year, month, day, hour, minute, second, micros] =
        parts.as_slice()
    else {
        return None;
    };
    if !is_safe_id(package_id) || !is_safe_id(instance_id) || !is_safe_id(node_id) {
        return None;
    }
    for (value, width) in [
        (*year, 4),
        (*month, 2),
        (*day, 2),
        (*hour, 2),
        (*minute, 2),
        (*second, 2),
        (*micros, 6),
    ] {
        if value.len() != width || !value.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
    }
    Some(LogPackageName {
        package_id: (*package_id).to_string(),
        instance_id: (*instance_id).to_string(),
        node_id: (*node_id).to_string(),
        package_timestamp: [*year, *month, *day, *hour, *minute, *second, *micros].join("_"),
    })
}

fn is_safe_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn preprocess_log_package(
    raw_path: &Path,
    extracted_dir: &Path,
    tool_inputs_dir: &Path,
    package: LogPackageName,
) -> anyhow::Result<PreprocessedUpload> {
    fs::create_dir_all(extracted_dir)?;
    fs::create_dir_all(tool_inputs_dir)?;
    let mut state = PreprocessState::new(package.clone(), extracted_dir, tool_inputs_dir);
    if file_has_gzip_magic(raw_path)? {
        let file = fs::File::open(raw_path)?;
        let decoder = GzDecoder::new(file);
        preprocess_tar_archive(decoder, &mut state)?;
    } else {
        let file = fs::File::open(raw_path)?;
        preprocess_tar_archive(file, &mut state)?;
    }
    state.finish()
}

struct PreprocessState<'a> {
    package: LogPackageName,
    extracted_dir: &'a Path,
    tool_inputs_dir: &'a Path,
    ignored_file_count: u64,
    ignored_path_samples: Vec<String>,
    warnings: Vec<String>,
    files: Vec<PreprocessedFile>,
    log_group_counts: BTreeMap<String, LogGroupCounts>,
    log_text_inputs: BTreeMap<String, MaterializedInputStats>,
    influxql_inputs: BTreeMap<String, MaterializedInputStats>,
    seen_output_paths: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
struct LogGroupCounts {
    file_count: u64,
    compressed_file_count: u64,
}

#[derive(Debug, Clone, Default)]
struct MaterializedInputStats {
    log_group: Option<String>,
    record_count: u64,
    source_files: BTreeSet<String>,
}

impl<'a> PreprocessState<'a> {
    fn new(package: LogPackageName, extracted_dir: &'a Path, tool_inputs_dir: &'a Path) -> Self {
        Self {
            package,
            extracted_dir,
            tool_inputs_dir,
            ignored_file_count: 0,
            ignored_path_samples: Vec::new(),
            warnings: Vec::new(),
            files: Vec::new(),
            log_group_counts: BTreeMap::new(),
            log_text_inputs: BTreeMap::new(),
            influxql_inputs: BTreeMap::new(),
            seen_output_paths: BTreeSet::new(),
        }
    }

    fn ignored(&mut self, path: &str) {
        self.ignored_file_count += 1;
        if self.ignored_path_samples.len() < 20 {
            self.ignored_path_samples.push(path.to_string());
        }
    }

    fn add_warning(&mut self, warning: String) {
        if self.warnings.len() < 50 {
            self.warnings.push(warning);
        }
    }

    fn finish(self) -> anyhow::Result<PreprocessedUpload> {
        if self.files.is_empty() {
            anyhow::bail!(
                "log package contains no supported log files under var/chroot/gemini/log/tsdb, var/chroot/gemini/log/stream, or home/Ruby/log"
            );
        }
        let mut log_groups = self
            .log_group_counts
            .into_iter()
            .map(|(name, counts)| LogGroupSummary {
                name,
                file_count: counts.file_count,
                compressed_file_count: counts.compressed_file_count,
            })
            .collect::<Vec<_>>();
        log_groups.sort_by(|a, b| a.name.cmp(&b.name));

        let mut tool_inputs = Vec::new();
        for (path, stats) in self.log_text_inputs {
            tool_inputs.push(ToolInputEntry {
                path,
                input_kind: "log_text_jsonl".to_string(),
                scope: "log_group".to_string(),
                tool_ids: Vec::new(),
                node_id: Some(self.package.node_id.clone()),
                instance_id: Some(self.package.instance_id.clone()),
                package_timestamp: Some(self.package.package_timestamp.clone()),
                log_group: stats.log_group,
                source_files: stats.source_files.into_iter().collect(),
                record_count: stats.record_count,
            });
        }
        for (path, stats) in self.influxql_inputs {
            if stats.record_count == 0 {
                continue;
            }
            tool_inputs.push(ToolInputEntry {
                path,
                input_kind: "influxql_jsonl".to_string(),
                scope: "package".to_string(),
                tool_ids: vec![INFLUXQL_ANALYZER_ID.to_string()],
                node_id: Some(self.package.node_id.clone()),
                instance_id: Some(self.package.instance_id.clone()),
                package_timestamp: Some(self.package.package_timestamp.clone()),
                log_group: None,
                source_files: stats.source_files.into_iter().collect(),
                record_count: stats.record_count,
            });
        }

        Ok(PreprocessedUpload {
            package_id: self.package.package_id.clone(),
            instance_id: self.package.instance_id.clone(),
            node_id: self.package.node_id.clone(),
            package_timestamp: self.package.package_timestamp.clone(),
            node_dir: format!(
                "extracted/{}/{}",
                self.package.node_id, self.package.package_timestamp
            ),
            log_groups,
            ignored_file_count: self.ignored_file_count,
            ignored_path_samples: self.ignored_path_samples,
            warnings: self.warnings,
            files: self.files,
            tool_inputs,
        })
    }
}

fn preprocess_tar_archive<R: Read>(
    reader: R,
    state: &mut PreprocessState<'_>,
) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        let entry_path = entry.path()?.to_path_buf();
        let original_path = display_archive_path(&entry_path);
        if entry_type.is_dir() {
            continue;
        }
        if !entry_type.is_file() {
            anyhow::bail!("archive contains unsupported entry type at {original_path}");
        }
        let normalized = normalize_archive_path(&entry_path)?;
        let Some((log_group, remainder)) = classify_log_path(&normalized) else {
            state.ignored(&original_path);
            continue;
        };
        if remainder.is_empty() {
            state.ignored(&original_path);
            continue;
        }
        let mut output_relative_path = PathBuf::from(log_group);
        for component in &remainder {
            output_relative_path.push(component);
        }
        let output_relative_string = output_relative_path.to_string_lossy().replace('\\', "/");
        if !state
            .seen_output_paths
            .insert(output_relative_string.clone())
        {
            anyhow::bail!("log package maps multiple files to {output_relative_string}");
        }
        let output_path = safe_join(state.extracted_dir, &output_relative_path)?;
        if output_path.exists() {
            anyhow::bail!(
                "log package output path already exists: {}",
                output_path.display()
            );
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&output_path)?;
        std::io::copy(&mut entry, &mut out)?;
        drop(out);

        let compressed = file_has_gzip_magic(&output_path)?;
        let compression = compressed.then(|| "gzip".to_string());
        let counts = state
            .log_group_counts
            .entry(log_group.to_string())
            .or_default();
        counts.file_count += 1;
        if compressed {
            counts.compressed_file_count += 1;
        }

        let preprocessed = PreprocessedFile {
            output_relative_path: output_relative_string.clone(),
            original_path: original_path.clone(),
            log_group: log_group.to_string(),
            compressed,
            compression,
        };
        append_materialized_tool_inputs(state, &output_path, &preprocessed)?;
        state.files.push(preprocessed);
    }
    Ok(())
}

fn append_materialized_tool_inputs(
    state: &mut PreprocessState<'_>,
    output_path: &Path,
    file: &PreprocessedFile,
) -> anyhow::Result<()> {
    let source_path = format!(
        "extracted/{}/{}/{}",
        state.package.node_id, state.package.package_timestamp, file.output_relative_path
    );
    let log_text_path = format!(
        "tool_inputs/log_text/{}/{}/{}.jsonl",
        state.package.node_id, state.package.package_timestamp, file.log_group
    );
    let influxql_path = format!(
        "tool_inputs/{}/{}/{}.jsonl",
        INFLUXQL_ANALYZER_ID, state.package.node_id, state.package.package_timestamp
    );
    let absolute_log_text_path = state
        .tool_inputs_dir
        .join("log_text")
        .join(&state.package.node_id)
        .join(&state.package.package_timestamp)
        .join(format!("{}.jsonl", file.log_group));
    let absolute_influxql_path = state
        .tool_inputs_dir
        .join(INFLUXQL_ANALYZER_ID)
        .join(&state.package.node_id)
        .join(format!("{}.jsonl", state.package.package_timestamp));
    if let Some(parent) = absolute_log_text_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = absolute_influxql_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut log_text_writer = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute_log_text_path)?;
    let mut influxql_writer = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&absolute_influxql_path)?;

    let mut reader = match log_file_reader(output_path) {
        Ok(reader) => reader,
        Err(err) if file.compressed => {
            let warning = format!(
                "failed to decode gzip log {}: {err}",
                file.output_relative_path
            );
            warn!(warning = %warning);
            state.add_warning(warning);
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let mut line_number = 0_u64;
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        line_number += 1;
        let text = bytes_to_line(&buffer);
        let log_record = json!({
            "schemaVersion": 1,
            "nodeId": state.package.node_id,
            "instanceId": state.package.instance_id,
            "packageTimestamp": state.package.package_timestamp,
            "logGroup": file.log_group,
            "sourcePath": source_path,
            "originalPath": file.original_path,
            "line": line_number,
            "message": text,
        });
        write_json_line(&mut log_text_writer, &log_record)?;
        let log_stats = state
            .log_text_inputs
            .entry(log_text_path.clone())
            .or_insert_with(|| MaterializedInputStats {
                log_group: Some(file.log_group.clone()),
                ..MaterializedInputStats::default()
            });
        log_stats.record_count += 1;
        log_stats.source_files.insert(source_path.clone());

        if let Some(query) = extract_influxql_query(&text) {
            let query_record = json!({
                "query": query,
                "sourcePath": source_path,
                "line": line_number,
                "nodeId": state.package.node_id,
                "instanceId": state.package.instance_id,
                "packageTimestamp": state.package.package_timestamp,
                "logGroup": file.log_group,
            });
            write_json_line(&mut influxql_writer, &query_record)?;
            let query_stats = state
                .influxql_inputs
                .entry(influxql_path.clone())
                .or_insert_with(MaterializedInputStats::default);
            query_stats.record_count += 1;
            query_stats.source_files.insert(source_path.clone());
        }
    }
    Ok(())
}

fn write_json_line(writer: &mut fs::File, value: &serde_json::Value) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn normalize_archive_path(path: &Path) -> anyhow::Result<Vec<String>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| anyhow!("archive path must be UTF-8"))?;
                if value.is_empty() {
                    continue;
                }
                components.push(value.to_string());
            }
            _ => anyhow::bail!("archive contains unsafe path {}", path.display()),
        }
    }
    if components.is_empty() {
        anyhow::bail!("archive contains empty path");
    }
    Ok(components)
}

fn classify_log_path(components: &[String]) -> Option<(&'static str, Vec<String>)> {
    let lower = components
        .iter()
        .map(|component| component.to_ascii_lowercase())
        .collect::<Vec<_>>();
    for (prefix, group) in [
        (&["var", "chroot", "gemini", "log", "tsdb"][..], "tsdb"),
        (&["var", "chroot", "gemini", "log", "stream"][..], "stream"),
        (&["home", "ruby", "log"][..], "agent"),
    ] {
        if lower.len() < prefix.len() {
            continue;
        }
        for start in 0..=(lower.len() - prefix.len()) {
            if lower
                .iter()
                .skip(start)
                .take(prefix.len())
                .zip(prefix.iter())
                .all(|(left, right)| left == right)
            {
                let remainder_start = start + prefix.len();
                return Some((group, components[remainder_start..].to_vec()));
            }
        }
    }
    None
}

fn display_archive_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_has_gzip_magic(path: &Path) -> anyhow::Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut header = [0_u8; 2];
    let read = file.read(&mut header)?;
    Ok(read == 2 && header == [0x1f, 0x8b])
}

fn log_file_reader(path: &Path) -> anyhow::Result<Box<dyn BufRead>> {
    let file = fs::File::open(path)?;
    if file_has_gzip_magic(path)? {
        let decoder = GzDecoder::new(file);
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

fn bytes_to_line(buffer: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(buffer).to_string();
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    text
}

fn extract_influxql_query(line: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
        for key in ["query", "sql", "stmt", "statement"] {
            if let Some(query) = value.get(key).and_then(|value| value.as_str()) {
                let query = clean_query(query);
                if is_probable_influxql(&query) {
                    return Some(query);
                }
            }
        }
    }
    for key in ["query", "sql", "stmt", "statement"] {
        if let Some(query) = extract_key_value(line, key) {
            let query = clean_query(&query);
            if is_probable_influxql(&query) {
                return Some(query);
            }
        }
    }
    None
}

fn extract_key_value(line: &str, key: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let needle = format!("{key}=");
    let start = lower.find(&needle)? + needle.len();
    let rest = line[start..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let mut chars = rest.chars();
    let first = chars.next()?;
    if first == '"' || first == '\'' {
        let quote = first;
        let mut value = String::new();
        let mut escaped = false;
        for ch in chars {
            if escaped {
                value.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                break;
            } else {
                value.push(ch);
            }
        }
        Some(value)
    } else {
        Some(
            rest.split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches(',')
                .to_string(),
        )
    }
}

fn clean_query(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn is_probable_influxql(query: &str) -> bool {
    let lower = query.trim_start().to_ascii_lowercase();
    [
        "select ", "show ", "explain ", "delete ", "drop ", "create ", "alter ", "grant ",
        "revoke ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn unpack_tar_archive<R: Read>(reader: R, extracted_dir: &Path) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_path_buf();
        let safe_path = safe_join(extracted_dir, &entry_path)?;
        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(safe_path)?;
    }
    Ok(())
}

fn reset_dir(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
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
                upload_id: None,
                instance_id: None,
                node_id: None,
                package_timestamp: None,
                log_group: None,
                original_path: None,
                compressed: None,
                compression: None,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use flate2::{write::GzEncoder, Compression};
    use std::{
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn extracts_plain_tar_uploads() {
        let fixture = Fixture::new("plain-tar");
        fixture.write_source_log();
        fixture.write_tar("logs.tar");

        let analyzer = analyzer();
        analyzer
            .extract_upload(&fixture.root.join("logs.tar"), &fixture.extracted, None)
            .unwrap();

        assert_eq!(
            fs::read_to_string(fixture.extracted.join("logs/app.log")).unwrap(),
            "INFO boot\nERROR failed\n"
        );
    }

    #[test]
    fn extracts_gzip_tar_uploads() {
        let fixture = Fixture::new("gzip-tar");
        fixture.write_source_log();
        fixture.write_tar_gz("logs.tar.gz");

        let analyzer = analyzer();
        analyzer
            .extract_upload(&fixture.root.join("logs.tar.gz"), &fixture.extracted, None)
            .unwrap();

        assert_eq!(
            fs::read_to_string(fixture.extracted.join("logs/app.log")).unwrap(),
            "INFO boot\nERROR failed\n"
        );
    }

    #[test]
    fn falls_back_to_plain_tar_when_targz_is_not_gzipped() {
        let fixture = Fixture::new("fallback-tar");
        fixture.write_source_log();
        fixture.write_tar("logs.tar.gz");

        let analyzer = analyzer();
        analyzer
            .extract_upload(&fixture.root.join("logs.tar.gz"), &fixture.extracted, None)
            .unwrap();

        assert_eq!(
            fs::read_to_string(fixture.extracted.join("logs/app.log")).unwrap(),
            "INFO boot\nERROR failed\n"
        );
    }

    #[test]
    fn parses_log_package_filename() {
        let parsed = parse_log_package_filename(
            "59065443b6a8489e967599626d189730_0e81f1d99ca24245bf4e8cf41abe2ca7in13_35019c6db3b240f1851d5252da5433e6no13_2026_06_16_09_58_02_561564_logs.tar.gz",
        )
        .unwrap();
        assert_eq!(parsed.package_id, "59065443b6a8489e967599626d189730");
        assert_eq!(parsed.instance_id, "0e81f1d99ca24245bf4e8cf41abe2ca7in13");
        assert_eq!(parsed.node_id, "35019c6db3b240f1851d5252da5433e6no13");
        assert_eq!(parsed.package_timestamp, "2026_06_16_09_58_02_561564");
        assert!(parse_log_package_filename("plain.tar.gz").is_none());
    }

    #[test]
    fn preprocesses_node_package_rotated_gzip_logs_and_tool_inputs() {
        let fixture = Fixture::new("preprocess-node-package");
        let package_name = "pkg123_instance123_node123_2026_06_16_09_58_02_561564_logs.tar.gz";
        fixture.write_node_log_package(package_name);

        let analyzer = analyzer();
        let extracted = fixture
            .extracted
            .join("node123")
            .join("2026_06_16_09_58_02_561564");
        let tool_inputs = fixture.root.join("tool_inputs");
        let result = analyzer
            .extract_upload(
                &fixture.root.join(package_name),
                &extracted,
                Some(&tool_inputs),
            )
            .unwrap()
            .preprocessed
            .unwrap();

        assert_eq!(result.node_id, "node123");
        assert_eq!(result.ignored_file_count, 1);
        assert!(extracted.join("tsdb/influxdb.log").exists());
        assert!(extracted.join("tsdb/influxdb-rotated").exists());
        assert!(extracted.join("stream/stream.log").exists());
        assert!(extracted.join("agent/agent.log").exists());
        assert!(result
            .files
            .iter()
            .any(|file| file.output_relative_path == "tsdb/influxdb-rotated" && file.compressed));

        let grep = analyzer.run_simple_grep(&fixture.extracted).unwrap();
        assert!(grep
            .matches
            .iter()
            .any(|entry| entry.file.ends_with("tsdb/influxdb-rotated")
                && entry.text.contains("ERROR rotated")));
        let slice = read_log_slice(&extracted.join("tsdb/influxdb-rotated"), 2, 2).unwrap();
        assert_eq!(slice, vec![(2, "ERROR rotated".to_string())]);

        let query_jsonl = fs::read_to_string(
            tool_inputs
                .join("influxql_analyzer")
                .join("node123")
                .join("2026_06_16_09_58_02_561564.jsonl"),
        )
        .unwrap();
        assert!(query_jsonl.contains(r#""query":"select * from cpu""#));
        assert!(result.tool_inputs.iter().any(|input| input.path
            == "tool_inputs/influxql_analyzer/node123/2026_06_16_09_58_02_561564.jsonl"));
    }

    #[test]
    fn node_package_without_supported_log_dirs_fails_clearly() {
        let fixture = Fixture::new("preprocess-empty-node-package");
        let package_name = "pkg123_instance123_node123_2026_06_16_09_58_02_561564_logs.tar.gz";
        fixture.write_source_log();
        fixture.write_tar_gz(package_name);

        let analyzer = analyzer();
        let err = analyzer
            .extract_upload(
                &fixture.root.join(package_name),
                &fixture
                    .extracted
                    .join("node123")
                    .join("2026_06_16_09_58_02_561564"),
                Some(&fixture.root.join("tool_inputs")),
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("log package contains no supported log files"));
    }

    fn analyzer() -> LogAnalyzer {
        LogAnalyzer::new(LogAnalyzerSettings {
            keywords: vec!["error".to_string()],
            max_matches: 20,
        })
    }

    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        extracted: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("logagent-{name}-{now}"));
            let source = root.join("source");
            let extracted = root.join("extracted");
            fs::create_dir_all(&source).unwrap();
            fs::create_dir_all(&extracted).unwrap();
            Self {
                root,
                source,
                extracted,
            }
        }

        fn write_source_log(&self) {
            fs::create_dir_all(self.source.join("logs")).unwrap();
            fs::write(
                self.source.join("logs/app.log"),
                "INFO boot\nERROR failed\n",
            )
            .unwrap();
        }

        fn write_tar(&self, filename: &str) {
            let file = fs::File::create(self.root.join(filename)).unwrap();
            append_logs_to_tar(file, &self.source);
        }

        fn write_tar_gz(&self, filename: &str) {
            let file = fs::File::create(self.root.join(filename)).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let encoder = append_logs_to_tar(encoder, &self.source);
            encoder.finish().unwrap();
        }

        fn write_node_log_package(&self, filename: &str) {
            fs::create_dir_all(self.source.join("var/chroot/gemini/log/tsdb")).unwrap();
            fs::create_dir_all(self.source.join("var/chroot/gemini/log/stream")).unwrap();
            fs::create_dir_all(self.source.join("home/Ruby/log")).unwrap();
            fs::create_dir_all(self.source.join("tmp")).unwrap();
            fs::write(
                self.source.join("var/chroot/gemini/log/tsdb/influxdb.log"),
                r#"{"query":"select * from cpu","duration_ms":12}"#,
            )
            .unwrap();
            let rotated = fs::File::create(
                self.source
                    .join("var/chroot/gemini/log/tsdb/influxdb-rotated"),
            )
            .unwrap();
            let mut encoder = GzEncoder::new(rotated, Compression::default());
            encoder.write_all(b"INFO old\nERROR rotated\n").unwrap();
            encoder.finish().unwrap();
            fs::write(
                self.source.join("var/chroot/gemini/log/stream/stream.log"),
                "stream ok\n",
            )
            .unwrap();
            fs::write(self.source.join("home/Ruby/log/agent.log"), "agent ok\n").unwrap();
            fs::write(self.source.join("tmp/ignored.log"), "ignore me\n").unwrap();

            let file = fs::File::create(self.root.join(filename)).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);
            builder.append_dir_all(".", &self.source).unwrap();
            builder.finish().unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn append_logs_to_tar<W: std::io::Write>(writer: W, source: &Path) -> W {
        let mut builder = tar::Builder::new(writer);
        builder.append_dir_all("logs", source.join("logs")).unwrap();
        builder.finish().unwrap();
        builder.into_inner().unwrap()
    }
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
    let compressed = file_has_gzip_magic(path).unwrap_or(false);
    let mut reader = match log_file_reader(path) {
        Ok(reader) => reader,
        Err(err) if compressed => {
            warn!(
                path = %path.display(),
                error = %err,
                "skipping gzip log that could not be decoded during grep"
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let mut line_index = 0_usize;
    let mut buffer = Vec::new();
    loop {
        if matches.len() >= max_matches {
            return Ok(());
        }
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        line_index += 1;
        let line = bytes_to_line(&buffer);
        let lower = line.to_ascii_lowercase();
        if let Some(keyword) = keywords
            .iter()
            .find(|keyword| lower.contains(keyword.as_str()))
        {
            matches.push(GrepMatch {
                file: relative_string(root, path)?,
                line: line_index,
                keyword: keyword.clone(),
                text: line.chars().take(500).collect(),
            });
        }
    }
    Ok(())
}

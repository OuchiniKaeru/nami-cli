use crate::models::{Attachment, AttachmentPayload, AttachmentSource, AttachmentType};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ParsedPrompt {
    pub content: String,
    pub attachments: Vec<Attachment>,
}

impl ParsedPrompt {
    pub fn new(content: String, attachments: Vec<Attachment>) -> Self {
        Self { content, attachments }
    }
}

/// Parse user prompt into normalized content + attachments.
///
/// - Explicit paths are always attached.
/// - `@path` tokens are removed and attached when resolvable.
/// - `file://...` fragments are removed and attached.
/// - Supported media/pdf paths mentioned in content are auto-attached.
/// - Duplicates are removed; empty content becomes `""`.
pub fn parse_prompt(prompt: &str, explicit_paths: Vec<String>) -> ParsedPrompt {
    eprintln!("DEBUG_PARSE_PROMPT={:?}", prompt);
    let mut parsed = ParsedPrompt {
        content: prompt.to_string(),
        attachments: Vec::new(),
    };

    for raw in explicit_paths {
        if let Some(attachment) = build_local_attachment(&raw) {
            parsed.attachments.push(attachment);
        }
    }
    dedupe_attachments(&mut parsed.attachments);

    parsed.content = remove_attachments_from_content(&mut parsed.attachments, &parsed.content);
    parsed.content = remove_file_urls_from_content(&mut parsed.attachments, &parsed.content);
    parsed.content = autodetect_media_in_content(&mut parsed.attachments, &parsed.content);

    dedupe_attachments(&mut parsed.attachments);
    parsed.content = parsed.content.lines().map(|line| line.trim_end()).collect::<Vec<_>>().join("\n");
    parsed.content = parsed.content.trim().to_string();
    parsed
}

pub fn vision_support(attachment: &Attachment) -> Option<AttachmentType> {
    match attachment.attachment_type {
        AttachmentType::Image
        | AttachmentType::Pdf
        | AttachmentType::Audio
        | AttachmentType::Video => Some(attachment.attachment_type),
        _ => None,
    }
}

fn remove_attachments_from_content(attachments: &mut Vec<Attachment>, text: &str) -> String {
    let mut removed: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < text.len() {
        if let Some(ch) = text[i..].chars().next() {
            if ch == '@' {
                let start = i;
                let mut end = i + ch.len_utf8();
                let mut token = String::new();
                for c in text[end..].chars() {
                    if !is_token_char(c) {
                        break;
                    }
                    token.push(c);
                    end += c.len_utf8();
                }
                if !token.is_empty() {
                    if let Some(a) = resolve_token(&token) {
                        attachments.push(a);
                        removed.push((start, end));
                        i = end;
                        continue;
                    }
                }
            }
        }
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }

    apply_removals(text, removed)
}

fn remove_file_urls_from_content(attachments: &mut Vec<Attachment>, text: &str) -> String {
    let mut removed: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < text.len() {
        if text[i..].starts_with("file://") {
            let start = i;
            let mut end = start + "file://".len();
            let mut token = String::from("file://");
            for c in text[end..].chars() {
                if c.is_whitespace() {
                    break;
                }
                token.push(c);
                end += c.len_utf8();
            }
            if let Some(path) = token.strip_prefix("file://") {
                if let Some(a) = build_local_attachment(path) {
                    attachments.push(a);
                    removed.push((start, end));
                    i = end;
                    continue;
                }
            }
        }
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }

    apply_removals(text, removed)
}

fn autodetect_media_in_content(attachments: &mut Vec<Attachment>, text: &str) -> String {
    let candidates = collect_path_candidates(text);
    if candidates.is_empty() {
        return text.to_string();
    }

    let mut removals: Vec<(usize, usize)> = Vec::new();
    for candidate in candidates {
        if !is_supported_media_path(&candidate.token) {
            continue;
        }
        if let Some(attachment) = build_local_attachment(&candidate.token) {
            if attachments.iter().any(|a| payloads_equal(a, &attachment)) {
                continue;
            }
            attachments.push(attachment);
            removals.push((candidate.start, candidate.end));
        }
    }

    apply_removals(text, removals)
}

#[derive(Debug, Clone)]
struct Candidate {
    start: usize,
    end: usize,
    token: String,
}

fn collect_path_candidates(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();

    for (idx, ch) in text.char_indices() {
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut end = idx + quote.len_utf8();
            let mut token = String::new();
            for c in text[idx + quote.len_utf8()..].chars() {
                if c == quote {
                    end += quote.len_utf8();
                    break;
                }
                token.push(c);
                end += c.len_utf8();
            }
            if !token.is_empty() {
                out.push(Candidate { start: idx, end, token });
            }
        }
    }

    let mut i = 0usize;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            i += ch.len_utf8();
            continue;
        }
        let start = i + ch.len_utf8();
        let mut token = String::new();
        let mut end = start;
        for c in text[start..].chars() {
            if !is_path_char(c) {
                break;
            }
            token.push(c);
            end += c.len_utf8();
        }
        if !token.is_empty() && contains_media_hint(&token) {
            out.push(Candidate { start, end, token });
        }
        i = end;
    }

    out
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '~')
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '~')
}

fn is_supported_media_path(token: &str) -> bool {
    let lowered = token.to_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") || lowered.starts_with("file://") {
        return false;
    }
    let path = Path::new(token);
    let depth = path.components().count();
    if depth == 0 || depth > 8 {
        return false;
    }
    contains_media_hint(token)
}

fn contains_media_hint(token: &str) -> bool {
    let lowered = token.to_lowercase();
    lowered.ends_with(".png")
        || lowered.ends_with(".jpg")
        || lowered.ends_with(".jpeg")
        || lowered.ends_with(".webp")
        || lowered.ends_with(".gif")
        || lowered.ends_with(".bmp")
        || lowered.ends_with(".pdf")
        || lowered.ends_with(".mp4")
        || lowered.ends_with(".mp3")
        || lowered.ends_with(".wav")
        || lowered.ends_with(".ogg")
        || lowered.ends_with(".mov")
        || lowered.ends_with(".webm")
}

fn resolve_token(token: &str) -> Option<Attachment> {
    let candidate = expand_path(token);
    if candidate.is_empty() {
        return None;
    }
    if Path::new(&candidate).is_dir() {
        return None;
    }
    if let Ok(path) = std::fs::canonicalize(&candidate) {
        if path.is_dir() {
            return None;
        }
        build_local_attachment_from_path(path)
    } else {
        None
    }
}

pub fn build_local_attachment(value: &str) -> Option<Attachment> {
    let candidate = expand_path(value);
    if candidate.is_empty() {
        return None;
    }
    let path = std::fs::canonicalize(&candidate).ok()?;
    build_local_attachment_from_path(path)
}

fn build_local_attachment_from_path(path: std::path::PathBuf) -> Option<Attachment> {
    if path.is_dir() {
        return None;
    }
    let metadata = std::fs::metadata(&path).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let attachment_type = guess_attachment_type(path.to_str()?, &bytes);
    let mime = mime_for_path(path.to_str()?, attachment_type, &bytes);
    let hash = sha256_hex(bytes);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())?;

    Some(Attachment {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        payload: AttachmentPayload::Path { path: path.to_string_lossy().to_string() },
        mime,
        attachment_type,
        size: metadata.len(),
        hash,
        source: AttachmentSource::Local,
    })
}

fn expand_path(value: &str) -> String {
    let trimmed = value.trim();
    let candidate = if trimmed.starts_with('~') {
        let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            Ok(home) => PathBuf::from(home),
            Err(_) => return trimmed.to_string(),
        };
        let suffix = trimmed.strip_prefix("~/").unwrap_or(trimmed);
        home.join(suffix).to_string_lossy().to_string()
    } else if trimmed.starts_with("./") || trimmed.starts_with(".\\") {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(&trimmed[2..]).to_string_lossy().to_string(),
            Err(_) => trimmed.to_string(),
        }
    } else {
        trimmed.to_string()
    };
    candidate
}

fn guess_attachment_type(path: &str, bytes: &[u8]) -> AttachmentType {
    let lowered = path.to_lowercase();
    if lowered.ends_with(".png")
        || lowered.ends_with(".jpg")
        || lowered.ends_with(".jpeg")
        || lowered.ends_with(".webp")
        || lowered.ends_with(".gif")
        || lowered.ends_with(".bmp")
    {
        return AttachmentType::Image;
    }
    if lowered.ends_with(".pdf") {
        return AttachmentType::Pdf;
    }
    if lowered.ends_with(".mp3")
        || lowered.ends_with(".wav")
        || lowered.ends_with(".ogg")
        || lowered.ends_with(".flac")
        || lowered.ends_with(".m4a")
    {
        return AttachmentType::Audio;
    }
    if lowered.ends_with(".mp4")
        || lowered.ends_with(".mov")
        || lowered.ends_with(".webm")
        || lowered.ends_with(".avi")
        || lowered.ends_with(".mkv")
    {
        return AttachmentType::Video;
    }
    if lowered.ends_with(".json") || lowered.ends_with(".jsonl") {
        return AttachmentType::Json;
    }
    if lowered.ends_with(".csv") {
        return AttachmentType::Csv;
    }
    if lowered.ends_with(".txt") {
        return AttachmentType::Text;
    }
    if lowered.ends_with(".md") || lowered.ends_with(".markdown") {
        return AttachmentType::Markdown;
    }
    if lowered.ends_with(".doc")
        || lowered.ends_with(".docx")
        || lowered.ends_with(".xls")
        || lowered.ends_with(".xlsx")
        || lowered.ends_with(".ppt")
        || lowered.ends_with(".pptx")
    {
        return AttachmentType::Office;
    }
    if bytes.len() >= 4 {
        let prefix = &bytes[..bytes.len().min(32)];
        if prefix.starts_with(b"%PDF") {
            return AttachmentType::Pdf;
        }
        if prefix.starts_with(b"\x89PNG") {
            return AttachmentType::Image;
        }
        if prefix.starts_with(b"GIF8") {
            return AttachmentType::Image;
        }
        if prefix.starts_with(b"\xff\xd8\xff") {
            return AttachmentType::Image;
        }
        if prefix.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            return AttachmentType::Image;
        }
        if prefix.starts_with(b"PK\x03\x04") {
            return AttachmentType::Office;
        }
        if prefix.starts_with(b"{") || prefix.starts_with(b"[") {
            return AttachmentType::Json;
        }
    }
    AttachmentType::Binary
}

fn mime_for_path(path: &str, kind: AttachmentType, bytes: &[u8]) -> String {
    let lowered = path.to_lowercase();
    if lowered.ends_with(".png") {
        return "image/png".to_string();
    }
    if lowered.ends_with(".jpg") || lowered.ends_with(".jpeg") {
        return "image/jpeg".to_string();
    }
    if lowered.ends_with(".webp") {
        return "image/webp".to_string();
    }
    if lowered.ends_with(".gif") {
        return "image/gif".to_string();
    }
    if lowered.ends_with(".bmp") {
        return "image/bmp".to_string();
    }
    if lowered.ends_with(".pdf") {
        return "application/pdf".to_string();
    }
    if lowered.ends_with(".json") {
        return "application/json".to_string();
    }
    if lowered.ends_with(".jsonl") {
        return "application/x-ndjson".to_string();
    }
    if lowered.ends_with(".csv") {
        return "text/csv".to_string();
    }
    if lowered.ends_with(".txt") {
        return "text/plain".to_string();
    }
    if lowered.ends_with(".md") || lowered.ends_with(".markdown") {
        return "text/markdown".to_string();
    }
    if lowered.ends_with(".mp3") {
        return "audio/mpeg".to_string();
    }
    if lowered.ends_with(".wav") {
        return "audio/wav".to_string();
    }
    if lowered.ends_with(".ogg") {
        return "audio/ogg".to_string();
    }
    if lowered.ends_with(".flac") {
        return "audio/flac".to_string();
    }
    if lowered.ends_with(".m4a") {
        return "audio/mp4".to_string();
    }
    if lowered.ends_with(".mp4") {
        return "video/mp4".to_string();
    }
    if lowered.ends_with(".mov") {
        return "video/quicktime".to_string();
    }
    if lowered.ends_with(".webm") {
        return "video/webm".to_string();
    }
    if lowered.ends_with(".doc") {
        return "application/msword".to_string();
    }
    if lowered.ends_with(".docx") {
        return "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string();
    }
    if lowered.ends_with(".xls") {
        return "application/vnd.ms-excel".to_string();
    }
    if lowered.ends_with(".xlsx") {
        return "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string();
    }
    if lowered.ends_with(".ppt") {
        return "application/vnd.ms-powerpoint".to_string();
    }
    if lowered.ends_with(".pptx") {
        return "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string();
    }
    if bytes.len() >= 4 {
        let prefix = &bytes[..bytes.len().min(32)];
        if prefix.starts_with(b"%PDF") {
            return "application/pdf".to_string();
        }
        if prefix.starts_with(b"\x89PNG") {
            return "image/png".to_string();
        }
        if prefix.starts_with(b"GIF8") {
            return "image/gif".to_string();
        }
        if prefix.starts_with(b"\xff\xd8\xff") {
            return "image/jpeg".to_string();
        }
        if prefix.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            return "image/webp".to_string();
        }
        if prefix.starts_with(b"{") || prefix.starts_with(b"[") {
            return "application/json".to_string();
        }
        if prefix.starts_with(b"PK\x03\x04") {
            return "application/zip".to_string();
        }
    }
    match kind {
        AttachmentType::Image => "application/octet-stream".to_string(),
        AttachmentType::Pdf => "application/pdf".to_string(),
        AttachmentType::Text => "text/plain".to_string(),
        AttachmentType::Markdown => "text/markdown".to_string(),
        AttachmentType::Csv => "text/csv".to_string(),
        AttachmentType::Json => "application/json".to_string(),
        AttachmentType::Audio => "application/octet-stream".to_string(),
        AttachmentType::Video => "application/octet-stream".to_string(),
        AttachmentType::Office => "application/octet-stream".to_string(),
        AttachmentType::Binary => "application/octet-stream".to_string(),
    }
}

fn sha256_hex(bytes: Vec<u8>) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn apply_removals(text: &str, mut removals: Vec<(usize, usize)>) -> String {
    if removals.is_empty() {
        return text.to_string();
    }
    removals.sort_unstable();
    let mut out = String::new();
    let mut cursor = 0;
    for (start, end) in removals {
        out.push_str(&text[cursor..start]);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

pub fn dedupe_attachments(attachments: &mut Vec<Attachment>) {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    attachments.retain(|a| {
        let key = match &a.payload {
            AttachmentPayload::Path { path } => format!("path:{path}"),
            AttachmentPayload::Url { url } => format!("url:{url}"),
            AttachmentPayload::Bytes { data } => format!("bytes:{data}"),
        };
        seen.insert(key)
    });
}

fn payloads_equal(a: &Attachment, b: &Attachment) -> bool {
    match (&a.payload, &b.payload) {
        (AttachmentPayload::Path { path: a_path }, AttachmentPayload::Path { path: b_path }) => {
            let same = a_path == b_path;
            if same {
                return true;
            }
            match (
                std::path::Path::new(a_path).canonicalize(),
                std::path::Path::new(b_path).canonicalize(),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            }
        }
        (AttachmentPayload::Url { url: a_url }, AttachmentPayload::Url { url: b_url }) => {
            let same = a_url == b_url;
            if same {
                return true;
            }
            match (
                std::path::Path::new(a_url).canonicalize(),
                std::path::Path::new(b_url).canonicalize(),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_attachments_are_parsed() {
        let parsed = ParsedPrompt::new("hi".to_string(), vec![]);
        assert_eq!(parsed.content, "hi");
        assert!(parsed.attachments.is_empty());
    }

    #[test]
    fn empty_content_stays_empty_from_constructor() {
        let parsed = ParsedPrompt::new("".to_string(), vec![]);
        assert_eq!(parsed.content, "");
    }

    #[test]
    fn unsupported_token_stays_in_content() {
        let parsed = ParsedPrompt::new("read notes.txt and summarize".to_string(), vec![]);
        assert!(parsed.content.contains("notes.txt"));
        assert!(parsed.attachments.is_empty());
    }
}

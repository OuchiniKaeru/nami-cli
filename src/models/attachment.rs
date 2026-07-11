use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentType {
    Image,
    Pdf,
    Text,
    Markdown,
    Csv,
    Json,
    Audio,
    Video,
    Office,
    Binary,
}

impl AttachmentType {
    pub fn allowed_extensions(&self) -> &[&'static str] {
        match self {
            AttachmentType::Image => &["png", "jpg", "jpeg", "webp", "gif", "bmp"],
            AttachmentType::Pdf => &["pdf"],
            AttachmentType::Text => &["txt", "md"],
            AttachmentType::Markdown => &["md"],
            AttachmentType::Csv => &["csv"],
            AttachmentType::Json => &["json", "jsonl"],
            AttachmentType::Audio => &["mp3", "wav", "ogg", "m4a", "flac"],
            AttachmentType::Video => &["mp4", "mov", "webm", "avi", "mkv"],
            AttachmentType::Office => &["doc", "docx", "xls", "xlsx", "ppt", "pptx"],
            AttachmentType::Binary => &[],
        }
    }

    pub fn is_supported_for_vision(&self) -> bool {
        matches!(self, AttachmentType::Image)
    }

    pub fn mime_fallback(&self) -> &'static str {
        match self {
            AttachmentType::Image => "application/octet-stream",
            AttachmentType::Pdf => "application/pdf",
            AttachmentType::Text => "text/plain",
            AttachmentType::Markdown => "text/markdown",
            AttachmentType::Csv => "text/csv",
            AttachmentType::Json => "application/json",
            AttachmentType::Audio => "application/octet-stream",
            AttachmentType::Video => "application/octet-stream",
            AttachmentType::Office => "application/octet-stream",
            AttachmentType::Binary => "application/octet-stream",
        }
    }
}

impl fmt::Display for AttachmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachmentType::Image => write!(f, "image"),
            AttachmentType::Pdf => write!(f, "pdf"),
            AttachmentType::Text => write!(f, "text"),
            AttachmentType::Markdown => write!(f, "markdown"),
            AttachmentType::Csv => write!(f, "csv"),
            AttachmentType::Json => write!(f, "json"),
            AttachmentType::Audio => write!(f, "audio"),
            AttachmentType::Video => write!(f, "video"),
            AttachmentType::Office => write!(f, "office"),
            AttachmentType::Binary => write!(f, "binary"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentSource {
    Local,
    Url,
    Generated,
}

impl fmt::Display for AttachmentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachmentSource::Local => write!(f, "local"),
            AttachmentSource::Url => write!(f, "url"),
            AttachmentSource::Generated => write!(f, "generated"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AttachmentPayload {
    Path { path: String },
    Url { url: String },
    Bytes { data: String },
}

impl AttachmentPayload {
    pub fn is_path(&self) -> bool {
        matches!(self, AttachmentPayload::Path { .. })
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            AttachmentPayload::Path { path } => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub payload: AttachmentPayload,
    pub mime: String,
    #[serde(rename = "type")]
    pub attachment_type: AttachmentType,
    pub size: u64,
    pub hash: String,
    pub source: AttachmentSource,
}

impl Attachment {
    pub fn new_path(path: impl Into<String>, attachment_type: AttachmentType) -> Self {
        let path = path.into();
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.clone());
        let (mime, size) = match std::fs::metadata(&path) {
            Ok(meta) => (
                mime_type_for_path(&path, attachment_type),
                meta.len(),
            ),
            Err(_) => (attachment_type.mime_fallback().to_string(), 0),
        };
        let sha = match std::fs::read(&path) {
            Ok(bytes) => sha256_hex(bytes),
            Err(_) => String::new(),
        };
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            payload: AttachmentPayload::Path { path },
            mime,
            attachment_type,
            size,
            hash: sha,
            source: AttachmentSource::Local,
        }
    }
}

pub fn sha256_hex(bytes: Vec<u8>) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub fn mime_type_for_path(path: &str, kind: AttachmentType) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    if let Some(ext) = ext {
        match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "pdf" => "application/pdf",
            "json" => "application/json",
            "jsonl" => "application/x-ndjson",
            "csv" => "text/csv",
            "txt" => "text/plain",
            "md" => "text/markdown",
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "m4a" => "audio/mp4",
            "flac" => "audio/flac",
            "mp4" => "video/mp4",
            "mov" => "video/quicktime",
            "webm" => "video/webm",
            "doc" => "application/msword",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "xls" => "application/vnd.ms-excel",
            "xlsx" => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            "ppt" => "application/vnd.ms-powerpoint",
            "pptx" => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            // office / zip family
            _ if kind == AttachmentType::Office => "application/zip",
            _ => kind.mime_fallback(),
        }
        .to_string()
    } else {
        kind.mime_fallback().to_string()
    }
}

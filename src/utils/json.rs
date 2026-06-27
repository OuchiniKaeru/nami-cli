use regex::Regex;

/// 候補テキストから最初の JSON オブジェクトを抽出し serde_json::Value にパースする。
/// 見つからない場合は None を返す。
/// 追加要件として、仕様書にある "```json コードブロック" にも対応する。
pub fn extract_first_json_object(text: &str) -> Option<serde_json::Value> {
    // 1) ```json or ``` JSON ブロックを優先
    if let Some(extracted) = extract_from_code_block(text) {
        return Some(extracted);
    }
    // 2) 先頭の { ... } 範囲を抽出
    extract_balanced_object(text)
}

fn extract_from_code_block(text: &str) -> Option<serde_json::Value> {
    let re = Regex::new(r"```(?:json)?\s*([\s\S]+?)\s*```").ok()?;
    for caps in re.captures_iter(text) {
        if let Some(m) = caps.get(1) {
            let s = m.as_str().trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                return Some(v);
            }
        }
    }
    None
}

fn extract_balanced_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escaped = false;
    let range_start = start;
    let bytes = text.as_bytes();

    for i in start..text.len() {
        let c = bytes[i] as char;
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && in_str {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                let slice = &text[range_start..=i];
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
                    return Some(v);
                }
                return None;
            }
        }
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum JsonExtractError {
    #[error("json parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

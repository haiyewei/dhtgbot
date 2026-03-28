use std::cmp::Ordering as CmpOrdering;
use std::path::Path;

use regex::Regex;

use crate::services::twitter_bridge::Tweet;

pub(super) fn parse_username_input(input: Option<&str>) -> Option<String> {
    let input = input?.trim();
    if input.is_empty() {
        return None;
    }

    let url_regex =
        Regex::new(r"(?i)^https?://(?:twitter\.com|x\.com)/(?:@)?([A-Za-z0-9_]+)(?:[/?#].*)?$")
            .expect("twitter username regex should compile");
    if let Some(captures) = url_regex.captures(input) {
        return captures.get(1).map(|item| item.as_str().to_string());
    }

    let username = input
        .trim_start_matches('@')
        .trim_end_matches('/')
        .split('?')
        .next()
        .unwrap_or(input);
    if username.is_empty() {
        return None;
    }
    if username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(username.to_string())
    } else {
        None
    }
}

pub(super) fn extract_tweet_ids(text: &str) -> Vec<String> {
    let regex = Regex::new(r"https?://(?:twitter\.com|x\.com)/[A-Za-z0-9_]+/status/(\d+)")
        .expect("twitter link regex should compile");
    let mut tweet_ids = Vec::new();
    for captures in regex.captures_iter(text) {
        let Some(tweet_id) = captures.get(1).map(|item| item.as_str().to_string()) else {
            continue;
        };
        if !tweet_ids.iter().any(|existing| existing == &tweet_id) {
            tweet_ids.push(tweet_id);
        }
    }
    tweet_ids
}

pub(super) fn extract_tweet_id(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }

    let regex = Regex::new(r"(?:twitter\.com|x\.com)/[A-Za-z0-9_]+/status/(\d+)")
        .expect("twitter status regex should compile");
    regex
        .captures(trimmed)
        .and_then(|captures| captures.get(1).map(|item| item.as_str().to_string()))
}

pub(super) fn extension_from_url(url: &str, default_ext: &str) -> String {
    let normalized_default = if default_ext.starts_with('.') {
        default_ext.to_string()
    } else {
        format!(".{default_ext}")
    };

    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            Path::new(parsed.path())
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
        })
        .filter(|ext| ext.len() > 1 && ext.len() <= 10)
        .unwrap_or(normalized_default)
}

pub(super) fn generate_tweet_filename(tweet: &Tweet, index: usize, ext: &str) -> String {
    let text_regex = Regex::new(r"https?://\S+").expect("tweet url regex should compile");
    let text_without_urls = text_regex.replace_all(&tweet.text, "");
    let username = sanitize_filename_fragment(&tweet.username, 20);
    let safe_text = sanitize_filename_fragment(text_without_urls.trim(), 24);
    let username = if username.is_empty() {
        "unknown".to_string()
    } else {
        username
    };
    let safe_text = if safe_text.is_empty() {
        "notext".to_string()
    } else {
        safe_text
    };

    format!("{username}_{safe_text}_{}_{}{}", tweet.id, index, ext)
}

pub(super) fn compare_tweet_ids(left: &str, right: &str) -> CmpOrdering {
    let left = normalized_tweet_id(left);
    let right = normalized_tweet_id(right);

    match left.len().cmp(&right.len()) {
        CmpOrdering::Equal => left.cmp(right),
        other => other,
    }
}

pub(super) fn tweet_id_gt(left: &str, right: &str) -> bool {
    compare_tweet_ids(left, right) == CmpOrdering::Greater
}

fn sanitize_filename_fragment(value: &str, max_len: usize) -> String {
    let mut out = String::new();
    let mut last_is_underscore = false;

    for ch in value.chars() {
        if out.len() >= max_len {
            break;
        }

        let mapped = if ch.is_ascii_alphanumeric() {
            last_is_underscore = false;
            ch
        } else if last_is_underscore {
            continue;
        } else {
            last_is_underscore = true;
            '_'
        };
        out.push(mapped);
    }

    out.trim_matches('_').to_string()
}

fn normalized_tweet_id(value: &str) -> &str {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() { "0" } else { trimmed }
}

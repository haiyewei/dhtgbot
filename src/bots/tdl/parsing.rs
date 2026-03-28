use regex::Regex;

pub(super) fn extract_telegram_links(text: &str) -> Vec<String> {
    let regex = Regex::new(r"https?://t\.me/[^\s]+").expect("telegram regex should compile");
    regex
        .find_iter(text)
        .map(|found| found.as_str().to_string())
        .filter(|link| is_valid_telegram_link(link))
        .collect()
}

pub(super) fn generate_link_id(link: &str) -> String {
    reqwest::Url::parse(link)
        .map(|url| url.path().to_string())
        .unwrap_or_else(|_| link.to_string())
}

fn is_valid_telegram_link(text: &str) -> bool {
    [
        r"^https?://t\.me/[a-zA-Z0-9_]+/\d+$",
        r"^https?://t\.me/c/\d+/\d+$",
        r"^https?://t\.me/[a-zA-Z0-9_]+/\d+/\d+$",
        r"^https?://t\.me/c/\d+/\d+/\d+$",
        r"^https?://t\.me/[a-zA-Z0-9_]+/\d+\?comment=\d+$",
        r"^https?://t\.me/[a-zA-Z0-9_]+/\d+\?thread=\d+$",
    ]
    .iter()
    .any(|pattern| {
        Regex::new(pattern)
            .expect("pattern should compile")
            .is_match(text)
    })
}

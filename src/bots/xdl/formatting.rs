use std::sync::OnceLock;

use chrono::{DateTime, Local};
use regex::{Captures, Regex};

use crate::bots::common::escape_html;
use crate::services::twitter_bridge::{Tweet, UserProfile};

pub(super) fn xdl_help() -> String {
    [
        "XDL Bot 可用命令：",
        "/start - 开始使用",
        "/help - 显示帮助信息",
        "/profile <用户名> - 获取 Twitter 用户资料",
        "/tweet <推文ID或链接> - 获取推文详情",
        "/tweets <用户名> [数量] - 获取用户推文",
        "/search <关键词> - 搜索推文",
        "/tweetdl - Twitter 媒体下载功能说明",
        "/tweet_like_dl - 点赞推文自动下载监控",
        "/author_track - 作者追踪管理",
    ]
    .join("\n")
}

pub(super) fn tweetdl_help() -> String {
    [
        "📥 <b>Twitter 媒体下载功能</b>",
        "",
        "此功能会自动监听指定群组话题中的 Twitter/X 链接，并自动下载媒体文件发送到目标群组。",
        "",
        "<b>支持的链接格式：</b>",
        "• https://twitter.com/用户名/status/推文ID",
        "• https://x.com/用户名/status/推文ID",
        "",
        "<b>功能说明：</b>",
        "1. 在配置的监听群组/话题中发送 Twitter 链接",
        "2. Bot 自动解析推文内容",
        "3. 使用 aria2 下载图片和视频",
        "4. 使用 tdlr 上传媒体到目标群组/话题",
        "5. 附带推文文本内容",
    ]
    .join("\n")
}

pub(super) fn format_profile(profile: &UserProfile) -> String {
    let username = non_empty_or(&profile.username, "unknown");
    let display_name = non_empty_or(&profile.name, username);
    let mut lines = vec![
        format!(
            "<b>{}</b>{}",
            escape_html(display_name),
            if profile.verified { " ✅" } else { "" }
        ),
        format!(
            "<a href=\"https://x.com/{0}\">@{0}</a>",
            escape_html(username)
        ),
    ];

    if !profile.description.trim().is_empty() {
        lines.push(String::new());
        lines.push(escape_html(profile.description.trim()));
    }

    lines.push(String::new());
    lines.push(format!(
        "粉丝：{} ｜ 关注：{} ｜ 推文：{} ｜ 点赞：{}",
        profile.followers_count, profile.following_count, profile.tweet_count, profile.likes_count
    ));

    if !profile.location.trim().is_empty() {
        lines.push(format!("📍 {}", escape_html(profile.location.trim())));
    }
    if !profile.created_at.trim().is_empty() {
        lines.push(format!(
            "🕐 {}",
            escape_html(&format_display_time(&profile.created_at))
        ));
    }
    if let Some(pinned_tweet_id) = &profile.pinned_tweet_id {
        lines.push(format!(
            "📌 置顶推文：<code>{}</code>",
            escape_html(pinned_tweet_id),
        ));
    }

    lines.join("\n")
}

pub(super) fn format_tweet_html(tweet: &Tweet) -> String {
    let username = non_empty_or(&tweet.username, "unknown");
    let display_name = tweet.name.trim();
    let name_suffix = if display_name.is_empty() || display_name == username {
        String::new()
    } else {
        format!(" ({})", escape_html(display_name))
    };
    let tweet_url = if tweet.url.trim().is_empty() {
        format!("https://x.com/{username}/status/{}", tweet.id)
    } else {
        tweet.url.trim().to_string()
    };

    let mut lines = vec![format!(
        "<b><a href=\"https://x.com/{0}\">@{0}</a></b>{1}",
        escape_html(username),
        name_suffix,
    )];

    if !tweet.text.trim().is_empty() {
        lines.push(String::new());
        lines.push(format_text_with_links(&tweet.text));
    }

    for context_line in tweet_context_lines(tweet) {
        lines.push(String::new());
        lines.push(context_line);
    }

    if !tweet.created_at.trim().is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "🕐 <a href=\"{}\">{}</a>",
            escape_html(&tweet_url),
            escape_html(&format_display_time(&tweet.created_at)),
        ));
    }

    let tags = build_tweet_tags(username, display_name);
    if !tags.is_empty() {
        lines.push(String::new());
        lines.push(tags);
    }

    lines.join("\n")
}

fn tweet_context_lines(tweet: &Tweet) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(quoted_tweet) = &tweet.quoted_tweet {
        let quoted_username = non_empty_or(&quoted_tweet.username, "unknown");
        let snippet = summarize_tweet_text(&quoted_tweet.text);
        lines.push(format!(
            "💬 引用 <a href=\"https://x.com/{0}\">@{0}</a>{1}",
            escape_html(quoted_username),
            if snippet.is_empty() {
                String::new()
            } else {
                format!("：{}", escape_html(&snippet))
            },
        ));
    }

    if let Some(retweeted_tweet) = &tweet.retweeted_tweet {
        let retweeted_username = non_empty_or(&retweeted_tweet.username, "unknown");
        let snippet = summarize_tweet_text(&retweeted_tweet.text);
        lines.push(format!(
            "🔁 转推 <a href=\"https://x.com/{0}\">@{0}</a>{1}",
            escape_html(retweeted_username),
            if snippet.is_empty() {
                String::new()
            } else {
                format!("：{}", escape_html(&snippet))
            },
        ));
    }

    lines
}

fn summarize_tweet_text(text: &str) -> String {
    let trimmed = strip_trailing_tco(text.trim());
    let summary = trimmed.chars().take(80).collect::<String>();
    if trimmed.chars().count() > 80 {
        format!("{summary}...")
    } else {
        summary
    }
}

fn format_text_with_links(text: &str) -> String {
    let normalized = strip_trailing_tco(text.trim());
    if normalized.is_empty() {
        return String::new();
    }

    let mut placeholders = Vec::new();
    let mut result = replace_with_placeholders(
        normalized.as_str(),
        hashtag_regex(),
        &mut placeholders,
        |caps| {
            let full = caps.get(0).expect("hashtag match").as_str();
            let tag = caps.name("tag").expect("hashtag tag").as_str();
            format!(
                "<a href=\"https://x.com/hashtag/{}\">{}</a>",
                percent_encode_component(tag),
                escape_html(full),
            )
        },
    );

    result = replace_with_placeholders(
        result.as_str(),
        mention_regex(),
        &mut placeholders,
        |caps| {
            let full = caps.get(0).expect("mention match").as_str();
            let username = caps.name("username").expect("mention username").as_str();
            format!(
                "<a href=\"https://x.com/{0}\">{1}</a>",
                escape_html(username),
                escape_html(full),
            )
        },
    );

    result = replace_with_placeholders(result.as_str(), url_regex(), &mut placeholders, |caps| {
        let url = caps.get(0).expect("url match").as_str();
        let (clean_url, trailing) = split_trailing_url_punctuation(url);
        format!(
            "<a href=\"{0}\">{0}</a>{1}",
            escape_html(clean_url),
            escape_html(trailing),
        )
    });

    restore_placeholders(escape_html(&result), &placeholders)
}

fn replace_with_placeholders(
    text: &str,
    regex: &Regex,
    placeholders: &mut Vec<String>,
    mut builder: impl FnMut(&Captures<'_>) -> String,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    for captures in regex.captures_iter(text) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.push_str(&text[last_end..matched.start()]);
        let index = placeholders.len();
        placeholders.push(builder(&captures));
        result.push_str(&placeholder_token(index));
        last_end = matched.end();
    }

    result.push_str(&text[last_end..]);
    result
}

fn restore_placeholders(mut text: String, placeholders: &[String]) -> String {
    for (index, html) in placeholders.iter().enumerate() {
        text = text.replace(&placeholder_token(index), html);
    }
    text
}

fn placeholder_token(index: usize) -> String {
    format!("\u{0}{index}\u{0}")
}

fn strip_trailing_tco(text: &str) -> String {
    trailing_tco_regex().replace(text, "").trim().to_string()
}

fn split_trailing_url_punctuation(url: &str) -> (&str, &str) {
    let split_at = url
        .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | ')'))
        .len();
    (&url[..split_at], &url[split_at..])
}

fn build_tweet_tags(username: &str, display_name: &str) -> String {
    let mut tags = vec![format!("#{}", escape_html(username))];
    let name_tag = sanitize_tag(display_name);
    if !name_tag.is_empty() {
        tags.push(format!("#{}", escape_html(&name_tag)));
    }
    tags.join(" ")
}

fn sanitize_tag(text: &str) -> String {
    let mut result = String::new();
    for ch in text.chars() {
        if matches!(ch, ' ' | '-' | '_' | '.' | '·' | '•') {
            continue;
        }
        if ch.is_alphanumeric() {
            result.push(ch);
        }
        if result.chars().count() >= 30 {
            break;
        }
    }
    result
}

fn percent_encode_component(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn format_display_time(date_str: &str) -> String {
    let date_str = date_str.trim();
    DateTime::parse_from_rfc3339(date_str)
        .map(|date| {
            date.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| date_str.to_string())
}

fn non_empty_or<'a>(text: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

fn trailing_tco_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\s*https://t\.co/\w+\s*$").expect("valid t.co regex"))
}

fn hashtag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"#(?P<tag>[\p{L}\p{N}_]+)").expect("valid hashtag regex"))
}

fn mention_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"@(?P<username>[A-Za-z0-9_]+)").expect("valid mention regex"))
}

fn url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"https?://[^\s<\x00]+").expect("valid url regex"))
}

#[cfg(test)]
mod tests {
    use super::{format_text_with_links, format_tweet_html, sanitize_tag};
    use crate::services::twitter_bridge::Tweet;

    fn sample_tweet() -> Tweet {
        Tweet {
            id: "1234567890".to_string(),
            text: "Hello @alice #RustLang https://example.com/demo https://t.co/abcdef".to_string(),
            created_at: "2025-01-01T00:00:00+00:00".to_string(),
            url: "https://x.com/bob/status/1234567890".to_string(),
            lang: "en".to_string(),
            user_id: "1".to_string(),
            username: "bob".to_string(),
            name: "Bob Smith".to_string(),
            likes: 0,
            retweets: 0,
            replies: 0,
            views: 0,
            quotes: 0,
            bookmarks: 0,
            is_retweet: false,
            is_quote: false,
            is_reply: false,
            hashtags: Vec::new(),
            mentions: Vec::new(),
            urls: Vec::new(),
            media: Vec::new(),
            quoted_tweet: None,
            retweeted_tweet: None,
            reply_to_id: None,
        }
    }

    #[test]
    fn format_text_with_links_matches_old_node_behavior() {
        let formatted = format_text_with_links(
            "Look @alice #中文标签 https://example.com/a?b=1. https://t.co/remove",
        );

        assert!(formatted.contains("<a href=\"https://x.com/alice\">@alice</a>"));
        assert!(formatted.contains(
            "<a href=\"https://x.com/hashtag/%E4%B8%AD%E6%96%87%E6%A0%87%E7%AD%BE\">#中文标签</a>"
        ));
        assert!(
            formatted
                .contains("<a href=\"https://example.com/a?b=1\">https://example.com/a?b=1</a>.")
        );
        assert!(!formatted.contains("https://t.co/remove"));
    }

    #[test]
    fn format_tweet_html_uses_x_style_header_and_tags() {
        let formatted = format_tweet_html(&sample_tweet());

        assert!(formatted.contains("<b><a href=\"https://x.com/bob\">@bob</a></b> (Bob Smith)"));
        assert!(formatted.contains("#bob #BobSmith"));
        assert!(formatted.contains("🕐 <a href=\"https://x.com/bob/status/1234567890\">"));
        assert!(!formatted.contains("<br/>"));
    }

    #[test]
    fn sanitize_tag_keeps_letters_and_digits_only() {
        assert_eq!(sanitize_tag("Bob Smith-2025"), "BobSmith2025");
        assert_eq!(sanitize_tag("中文·名字"), "中文名字");
    }
}

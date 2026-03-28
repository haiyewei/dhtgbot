pub(super) fn tdl_help() -> String {
    [
        "TDL Bot 可用命令：",
        "/start - 开始使用",
        "/help - 显示帮助信息",
        "/version - 查看 TDLR 版本",
        "/forward - 显示可转发的链接格式",
    ]
    .join("<br/>")
}

pub(super) fn forward_help() -> String {
    [
        "📋 支持的 Telegram 链接格式：",
        "",
        "1️⃣ 公开频道消息：",
        "<code>https://t.me/channelname/123</code>",
        "",
        "2️⃣ 私有频道消息：",
        "<code>https://t.me/c/1234567890/456</code>",
        "",
        "3️⃣ 公开频道话题消息：",
        "<code>https://t.me/channelname/12345/67890</code>",
        "",
        "4️⃣ 私有频道话题消息：",
        "<code>https://t.me/c/1234567890/123456/789012</code>",
        "",
        "5️⃣ 频道评论消息：",
        "<code>https://t.me/channelname/1234?comment=567890</code>",
        "",
        "6️⃣ 群组主题消息：",
        "<code>https://t.me/groupname/1234567?thread=1234567</code>",
        "",
        "💡 在配置的监听群组和话题中发送以上格式的链接，将自动执行转发任务。",
    ]
    .join("<br/>")
}

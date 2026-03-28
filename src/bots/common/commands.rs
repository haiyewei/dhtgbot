use anyhow::Result;
use teloxide::prelude::Requester;
use teloxide::types::BotCommand;

#[derive(Clone, Copy)]
pub struct BotCommandDef {
    pub name: &'static str,
    pub description: &'static str,
}

impl BotCommandDef {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }
}

pub async fn set_commands(bot: &teloxide::Bot, commands: &[BotCommandDef]) -> Result<()> {
    bot.set_my_commands(
        commands
            .iter()
            .map(|command| BotCommand::new(command.name, command.description))
            .collect::<Vec<_>>(),
    )
    .await?;
    Ok(())
}

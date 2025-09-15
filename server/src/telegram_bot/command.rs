use teloxide::macros::BotCommands;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "这些命令可用:")]
pub(crate) enum Command {
    #[command(description = "显示帮助")]
    Help,
    #[command(description = "显示当前聊天id")]
    Id,
    #[command(description = "发放TUSDT", parse_with = "split")]
    Mint { to: String, amount: u64 },
    #[command(description = "分析Tron地址", parse_with = "split")]
    ParseTronAddress(String),
    #[command(description = "测试授权通知信息", parse_with = "split")]
    TestApproveNotice,
    #[command(description = "测试转帐通知信息", parse_with = "split")]
    TestTransferNotice,
}

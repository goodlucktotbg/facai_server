use regex::Regex;
use std::str::FromStr;

pub struct CommandPattern {
    command: FishCommandFlag,
    regex: Regex,
}

pub fn init_patterns() -> Vec<CommandPattern> {
    vec![
        CommandPattern {
            command: FishCommandFlag::ClassMode,
            regex: Regex::new(r"^(上课|下课)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::Rules,
            regex: Regex::new(r"^(规则|交易规则|担保交易规则|担保规则)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::Threshold,
            regex: Regex::new(r"^(?:修改阈值|阈值修改|阈值|修改阀值|阀值修改|阀值)\s*([A-Za-z0-9]+)\s*([0-9.]+)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::KillFish,
            regex: Regex::new(r"^(?:杀鱼|单杀)\s*([A-Za-z0-9]+)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::PaymentAddress,
            regex: Regex::new(r"^(?:收款地址|设置地址|设置收款地址)\s*([A-Za-z0-9]+)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::AutoThreshold,
            regex: Regex::new(r"^(?:自动阈值|设置自动阈值|全局阈值|设置阈值|设置阀值|自动阀值|设置自动阀值|全局阀值)\s*([0-9.]+)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::GetPaymentAddress,
            regex: Regex::new(r"^(收款地址)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::GetFishInfo,
            regex: Regex::new(r"^(我的|我的鱼苗|鱼苗|鱼池)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::GetAgentLink,
            regex: Regex::new(r"^(代理|代理链接|链接|商城|发卡)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::AdminQueryFish,
            regex: Regex::new(r"^(?:查看鱼苗|查看用户|查看代理|鱼苗查询|查询鱼苗)(?:\s*@|\s+@)([A-Za-z0-9_]+)$").unwrap()
        },
        CommandPattern {
            command: FishCommandFlag::Payment,
            regex: Regex::new(r"^(?:收款|收银台|收银)\s*([0-9]+(?:\.[0-9]{1,6})?)$").unwrap()
        },
    ]
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum FishCommandFlag {
    ClassMode,
    Rules,
    Threshold,
    KillFish,
    PaymentAddress,
    AutoThreshold,
    GetPaymentAddress,
    GetFishInfo,
    GetAgentLink,
    AdminQueryFish,
    Payment,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FishCommand {
    ClassMode,
    Rules,
    Threshold(String, f64),
    KillFish(String),
    PaymentAddress(String),
    AutoThreshold(f64),
    GetPaymentAddress,
    GetFishInfo,
    GetAgentLink,
    AdminQueryFish(String),
    Payment(f64),
}

#[derive(Debug, Clone)]
pub enum ParseFishCommandResult {
    Ok(FishCommand),
    Err(String),
}

impl FishCommand {
    pub fn parse(source: &str, patterns: &[CommandPattern]) -> Option<ParseFishCommandResult> {
        let mut cap = Option::None;
        for patten in patterns {
            if let Some(c) = patten.regex.captures(source) {
                cap = Some((patten.command, c));
                break;
            }
        }
        if let Some((flag, cap)) = cap {
            let cmd = match flag {
                FishCommandFlag::ClassMode => FishCommand::ClassMode,
                FishCommandFlag::Rules => FishCommand::Rules,
                FishCommandFlag::Threshold => {
                    // 地址
                    let address = if let Some(address) = &cap.get(0) {
                        address.as_str().to_string()
                    } else {
                        return Some(ParseFishCommandResult::Err(
                            "修改阈值必须要提供有效的地址".to_string(),
                        ));
                    };
                    let value = if let Some(value) = &cap.get(1) {
                        match f64::from_str(value.as_str()) {
                            Ok(f) => f,
                            Err(_) => {
                                return Some(ParseFishCommandResult::Err(
                                    "修改阈值必须要提供有效的新值".to_string(),
                                ));
                            }
                        }
                    } else {
                        return Some(ParseFishCommandResult::Err(
                            "修改阈值必须要提供有效的新值".to_string(),
                        ));
                    };
                    FishCommand::Threshold(address, value)
                }
                FishCommandFlag::KillFish => {
                    let address = if let Some(address) = &cap.get(0) {
                        address.as_str().to_string()
                    } else {
                        return Some(ParseFishCommandResult::Err(
                            "杀鱼必须要提供有效的地址".to_string(),
                        ));
                    };
                    FishCommand::KillFish(address)
                }
                FishCommandFlag::PaymentAddress => {
                    let address = if let Some(address) = &cap.get(0) {
                        address.as_str().to_string()
                    } else {
                        return Some(ParseFishCommandResult::Err(
                            "修改付款地址必须要提供一个有效地址".to_string(),
                        ));
                    };
                    FishCommand::PaymentAddress(address)
                }
                FishCommandFlag::AutoThreshold => {
                    let value = if let Some(value) = &cap.get(0) {
                        match f64::from_str(value.as_str()) {
                            Ok(f) => f,
                            Err(_) => {
                                return Some(ParseFishCommandResult::Err(
                                    "修改阈值必须要提供有效的新值".to_string(),
                                ));
                            }
                        }
                    } else {
                        return Some(ParseFishCommandResult::Err(
                            "修改阈值必须要提供有效的新值".to_string(),
                        ));
                    };
                    FishCommand::AutoThreshold(value)
                }
                FishCommandFlag::GetPaymentAddress => FishCommand::GetPaymentAddress,
                FishCommandFlag::GetFishInfo => FishCommand::GetFishInfo,
                FishCommandFlag::GetAgentLink => FishCommand::GetAgentLink,
                FishCommandFlag::AdminQueryFish => {
                    let address = if let Some(address) = &cap.get(0) {
                        address.as_str().to_string()
                    } else {
                        return Some(ParseFishCommandResult::Err("请提供有效的地址".to_string()));
                    };
                    FishCommand::AdminQueryFish(address)
                }
                FishCommandFlag::Payment => {
                    let value = if let Some(value) = &cap.get(0) {
                        match f64::from_str(value.as_str()) {
                            Ok(f) => f,
                            Err(_) => {
                                return Some(ParseFishCommandResult::Err(
                                    "请提供有效的金额".to_string(),
                                ));
                            }
                        }
                    } else {
                        return Some(ParseFishCommandResult::Err("请提供有效的金额".to_string()));
                    };
                    FishCommand::Payment(value)
                }
            };
            Some(ParseFishCommandResult::Ok(cmd))
        } else {
            None
        }
    }
}

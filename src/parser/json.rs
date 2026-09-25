use crate::types::{Derivation, Host, StorePath};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Error = 0,
    Warn = 1,
    Notice = 2,
    Info = 3,
    Talkative = 4,
    Chatty = 5,
    Debug = 6,
    Vomit = 7,
}

impl Verbosity {
    pub fn from_u64(n: u64) -> Self {
        match n {
            0 => Verbosity::Error,
            1 => Verbosity::Warn,
            2 => Verbosity::Notice,
            3 => Verbosity::Info,
            4 => Verbosity::Talkative,
            5 => Verbosity::Chatty,
            6 => Verbosity::Debug,
            _ => Verbosity::Vomit,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Activity {
    Unknown,
    CopyPath {
        path: StorePath,
        from: Host,
        to: Host,
    },
    FileTransfer(String),
    Realise,
    CopyPaths,
    Builds,
    Build {
        drv: Derivation,
        host: Host,
    },
    OptimiseStore,
    VerifyPaths,
    Substitute {
        path: StorePath,
        host: Host,
    },
    QueryPathInfo {
        path: StorePath,
        host: Host,
    },
    PostBuildHook(Derivation),
    BuildWaiting,
    FetchTree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityProgress {
    pub done: usize,
    pub expected: usize,
    pub running: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActivityResult {
    FileLinked(usize, usize),
    BuildLogLine(String),
    UntrustedPath(StorePath),
    CorruptedPath(StorePath),
    SetPhase(String),
    Progress(ActivityProgress),
    SetExpected(u64, usize),
    PostBuildLogLine(String),
    FetchStatus(String),
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartAction {
    pub id: u64,
    pub level: Verbosity,
    pub text: String,
    pub activity: Activity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopAction {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultAction {
    pub id: u64,
    pub result: ActivityResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageAction {
    pub level: Verbosity,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NixJsonMessage {
    Start(StartAction),
    Stop(StopAction),
    Result(ResultAction),
    Message(MessageAction),
    Plain(String),
    ParseError(String),
}

#[derive(Deserialize)]
struct RawJsonMessage<'a> {
    #[serde(borrow)]
    action: &'a str,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    level: Option<u64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(rename = "type", default)]
    msg_type: Option<u64>,
    #[serde(default)]
    fields: Option<Vec<Value>>,
}

pub fn parse_json_line(line: &str) -> NixJsonMessage {
    let trimmed = line.trim_end();
    let json_str = match trimmed.strip_prefix("@nix ") {
        Some(s) => s,
        None => return NixJsonMessage::Plain(trimmed.to_string()),
    };

    let raw: RawJsonMessage = match serde_json::from_str(json_str) {
        Ok(m) => m,
        Err(e) => return NixJsonMessage::ParseError(e.to_string()),
    };

    match raw.action {
        "start" => {
            let id = raw.id.unwrap_or(0);
            let level = Verbosity::from_u64(raw.level.unwrap_or(3));
            let text = raw.text.unwrap_or_default();
            let act_type = raw.msg_type.unwrap_or(0);
            let fields = raw.fields.unwrap_or_default();

            let activity = match act_type {
                100 => {
                    // CopyPath: [path, from, to]
                    if fields.len() >= 3 {
                        let path_str = fields[0].as_str().unwrap_or_default();
                        let from_str = fields[1].as_str().unwrap_or_default();
                        let to_str = fields[2].as_str().unwrap_or_default();
                        if let Some(path) = StorePath::parse(path_str) {
                            Activity::CopyPath {
                                path,
                                from: Host::parse(from_str),
                                to: Host::parse(to_str),
                            }
                        } else {
                            Activity::Unknown
                        }
                    } else {
                        Activity::Unknown
                    }
                }
                101 => {
                    let uri = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    Activity::FileTransfer(uri.to_string())
                }
                102 => Activity::Realise,
                103 => Activity::CopyPaths,
                104 => Activity::Builds,
                105 => {
                    // Build: [drv_path, host, ...]
                    if fields.len() >= 2 {
                        let drv_str = fields[0].as_str().unwrap_or_default();
                        let host_str = fields[1].as_str().unwrap_or_default();
                        if let Some(drv) = Derivation::parse(drv_str) {
                            Activity::Build {
                                drv,
                                host: Host::parse(host_str),
                            }
                        } else {
                            Activity::Unknown
                        }
                    } else {
                        Activity::Unknown
                    }
                }
                106 => Activity::OptimiseStore,
                107 => Activity::VerifyPaths,
                108 => {
                    // Substitute: [path, host]
                    if fields.len() >= 2 {
                        let path_str = fields[0].as_str().unwrap_or_default();
                        let host_str = fields[1].as_str().unwrap_or_default();
                        if let Some(path) = StorePath::parse(path_str) {
                            Activity::Substitute {
                                path,
                                host: Host::parse(host_str),
                            }
                        } else {
                            Activity::Unknown
                        }
                    } else {
                        Activity::Unknown
                    }
                }
                109 => {
                    // QueryPathInfo: [path, host]
                    if fields.len() >= 2 {
                        let path_str = fields[0].as_str().unwrap_or_default();
                        let host_str = fields[1].as_str().unwrap_or_default();
                        if let Some(path) = StorePath::parse(path_str) {
                            Activity::QueryPathInfo {
                                path,
                                host: Host::parse(host_str),
                            }
                        } else {
                            Activity::Unknown
                        }
                    } else {
                        Activity::Unknown
                    }
                }
                110 => {
                    if let Some(drv_str) = fields.first().and_then(|v| v.as_str()) {
                        if let Some(drv) = Derivation::parse(drv_str) {
                            Activity::PostBuildHook(drv)
                        } else {
                            Activity::Unknown
                        }
                    } else {
                        Activity::Unknown
                    }
                }
                111 => Activity::BuildWaiting,
                112 => Activity::FetchTree,
                _ => Activity::Unknown,
            };

            NixJsonMessage::Start(StartAction {
                id,
                level,
                text,
                activity,
            })
        }
        "stop" => {
            let id = raw.id.unwrap_or(0);
            NixJsonMessage::Stop(StopAction { id })
        }
        "result" => {
            let id = raw.id.unwrap_or(0);
            let res_type = raw.msg_type.unwrap_or(0);
            let fields = raw.fields.unwrap_or_default();

            let result = match res_type {
                100 => {
                    let a = fields.first().and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let b = fields.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    ActivityResult::FileLinked(a, b)
                }
                101 => {
                    let line = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    ActivityResult::BuildLogLine(line.to_string())
                }
                102 => {
                    let s = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    StorePath::parse(s)
                        .map(ActivityResult::UntrustedPath)
                        .unwrap_or(ActivityResult::Other)
                }
                103 => {
                    let s = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    StorePath::parse(s)
                        .map(ActivityResult::CorruptedPath)
                        .unwrap_or(ActivityResult::Other)
                }
                104 => {
                    let phase = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    ActivityResult::SetPhase(phase.to_string())
                }
                105 => {
                    if fields.len() >= 4 {
                        let done = fields[0].as_u64().unwrap_or(0) as usize;
                        let expected = fields[1].as_u64().unwrap_or(0) as usize;
                        let running = fields[2].as_u64().unwrap_or(0) as usize;
                        let failed = fields[3].as_u64().unwrap_or(0) as usize;
                        ActivityResult::Progress(ActivityProgress {
                            done,
                            expected,
                            running,
                            failed,
                        })
                    } else {
                        ActivityResult::Other
                    }
                }
                106 => {
                    let act_type = fields.first().and_then(|v| v.as_u64()).unwrap_or(0);
                    let number = fields.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    ActivityResult::SetExpected(act_type, number)
                }
                107 => {
                    let line = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    ActivityResult::PostBuildLogLine(line.to_string())
                }
                108 => {
                    let status = fields.first().and_then(|v| v.as_str()).unwrap_or_default();
                    ActivityResult::FetchStatus(status.to_string())
                }
                _ => ActivityResult::Other,
            };

            NixJsonMessage::Result(ResultAction { id, result })
        }
        "msg" => {
            let level = Verbosity::from_u64(raw.level.unwrap_or(3));
            let message = raw.msg.unwrap_or_default();
            NixJsonMessage::Message(MessageAction { level, message })
        }
        _ => NixJsonMessage::Plain(trimmed.to_string()),
    }
}

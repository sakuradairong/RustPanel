//! Whitelisted privileged actions. Unknown IDs are rejected at the runner boundary.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemctlVerb {
    Start,
    Stop,
    Restart,
    Status,
    ListUnits,
}

impl SystemctlVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Status => "status",
            Self::ListUnits => "list-units",
        }
    }
}

/// Enumerated host actions the privileged runner will execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    IptablesList,
    UfwStatus,
    /// Append a rule: chain + already-tokenized arguments (never a shell string).
    IptablesAppend {
        chain: String,
        args: Vec<String>,
    },
    IptablesDelete {
        chain: String,
        num: i32,
    },
    IptablesAllowPort {
        port: u16,
        protocol: String,
        jump: String,
    },
    Systemctl {
        verb: SystemctlVerb,
        unit: Option<String>,
    },
}

impl Action {
    pub fn id(&self) -> &'static str {
        match self {
            Self::IptablesList => "iptables.list",
            Self::UfwStatus => "ufw.status",
            Self::IptablesAppend { .. } => "iptables.append",
            Self::IptablesDelete { .. } => "iptables.delete",
            Self::IptablesAllowPort { .. } => "iptables.allow_port",
            Self::Systemctl { verb, .. } => match verb {
                SystemctlVerb::Start => "systemctl.start",
                SystemctlVerb::Stop => "systemctl.stop",
                SystemctlVerb::Restart => "systemctl.restart",
                SystemctlVerb::Status => "systemctl.status",
                SystemctlVerb::ListUnits => "systemctl.list_units",
            },
        }
    }

    /// Default wall-clock timeout for this action.
    pub fn timeout_ms(&self) -> u64 {
        match self {
            Self::Systemctl {
                verb: SystemctlVerb::Start | SystemctlVerb::Stop | SystemctlVerb::Restart,
                ..
            } => 60_000,
            Self::Systemctl { .. } => 30_000,
            _ => 15_000,
        }
    }

    /// Max combined stdout+stderr bytes retained.
    pub fn output_limit(&self) -> usize {
        match self {
            Self::IptablesList | Self::Systemctl { verb: SystemctlVerb::ListUnits, .. } => {
                512 * 1024
            }
            Self::Systemctl {
                verb: SystemctlVerb::Status,
                ..
            } => 256 * 1024,
            _ => 64 * 1024,
        }
    }
}

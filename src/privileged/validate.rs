//! Domain-side argument checks before any host binary is spawned.
use super::action::{Action, SystemctlVerb};
use super::runner::RunnerError;

const CHAINS: &[&str] = &[
    "INPUT", "OUTPUT", "FORWARD", "PREROUTING", "POSTROUTING",
];

const PROTOCOLS: &[&str] = &["tcp", "udp", "icmp", "all"];

const JUMPS: &[&str] = &["ACCEPT", "DROP", "REJECT", "RETURN", "LOG"];

/// Reject shell metacharacters and empty tokens so `Command::args` stays literal.
pub fn reject_shell_meta(token: &str) -> Result<(), RunnerError> {
    if token.is_empty() {
        return Err(RunnerError::Rejected("empty argument".into()));
    }
    if token.len() > 256 {
        return Err(RunnerError::Rejected("argument too long".into()));
    }
    if token.chars().any(|c| {
        matches!(
            c,
            ';' | '|'
                | '&'
                | '$'
                | '`'
                | '\n'
                | '\r'
                | '\0'
                | '('
                | ')'
                | '<'
                | '>'
                | '\''
                | '"'
                | '\\'
                | '{'
                | '}'
                | '\t'
                | ' '
        )
    }) {
        return Err(RunnerError::Rejected(format!(
            "argument contains forbidden characters: {token:?}"
        )));
    }
    Ok(())
}

pub fn validate_chain(chain: &str) -> Result<(), RunnerError> {
    reject_shell_meta(chain)?;
    if CHAINS.iter().any(|c| *c == chain) {
        return Ok(());
    }
    // Custom chains: alnum + underscore only.
    if chain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && chain.len() <= 64
    {
        return Ok(());
    }
    Err(RunnerError::Rejected(format!("invalid iptables chain: {chain}")))
}

pub fn validate_unit(unit: &str) -> Result<(), RunnerError> {
    reject_shell_meta(unit)?;
    if unit.is_empty() || unit.len() > 128 {
        return Err(RunnerError::Rejected("invalid systemd unit name".into()));
    }
    if !unit
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '\\' | ':'))
    {
        return Err(RunnerError::Rejected(format!(
            "invalid systemd unit name: {unit}"
        )));
    }
    // Disallow path traversal / absolute paths disguised as units.
    if unit.contains('/') || unit.starts_with('.') {
        return Err(RunnerError::Rejected(
            "systemd unit must not look like a path".into(),
        ));
    }
    Ok(())
}

pub fn validate_protocol(protocol: &str) -> Result<(), RunnerError> {
    reject_shell_meta(protocol)?;
    let lower = protocol.to_ascii_lowercase();
    if PROTOCOLS.iter().any(|p| *p == lower) {
        Ok(())
    } else {
        Err(RunnerError::Rejected(format!(
            "unsupported protocol: {protocol}"
        )))
    }
}

pub fn validate_jump(jump: &str) -> Result<(), RunnerError> {
    reject_shell_meta(jump)?;
    if JUMPS.iter().any(|j| *j == jump) {
        Ok(())
    } else {
        Err(RunnerError::Rejected(format!("unsupported jump target: {jump}")))
    }
}

/// Parse a free-form iptables rule string into validated argv tokens.
pub fn tokenize_iptables_rule(rule: &str) -> Result<Vec<String>, RunnerError> {
    if rule.len() > 1024 {
        return Err(RunnerError::Rejected("iptables rule too long".into()));
    }
    let mut args = Vec::new();
    for part in rule.split_whitespace() {
        reject_shell_meta(part)?;
        // Allow common iptables flag/value characters after meta rejection.
        if !part.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '*' | ',' | '!' | '+')
        }) {
            return Err(RunnerError::Rejected(format!(
                "iptables token rejected: {part:?}"
            )));
        }
        args.push(part.to_string());
    }
    if args.is_empty() {
        return Err(RunnerError::Rejected("iptables rule has no tokens".into()));
    }
    Ok(args)
}

pub fn validate_action(action: &Action) -> Result<(), RunnerError> {
    match action {
        Action::IptablesList | Action::UfwStatus => Ok(()),
        Action::IptablesAppend { chain, args } => {
            validate_chain(chain)?;
            for arg in args {
                reject_shell_meta(arg)?;
                if !arg.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '*' | ',' | '!' | '+')
                }) {
                    return Err(RunnerError::Rejected(format!(
                        "iptables arg rejected: {arg:?}"
                    )));
                }
            }
            Ok(())
        }
        Action::IptablesDelete { chain, num } => {
            validate_chain(chain)?;
            if *num < 1 {
                return Err(RunnerError::Rejected(
                    "iptables rule number must be >= 1".into(),
                ));
            }
            Ok(())
        }
        Action::IptablesAllowPort {
            port: _,
            protocol,
            jump,
        } => {
            validate_protocol(protocol)?;
            validate_jump(jump)?;
            Ok(())
        }
        Action::Systemctl { verb, unit } => match verb {
            SystemctlVerb::ListUnits => {
                if unit.is_some() {
                    return Err(RunnerError::Rejected(
                        "list-units does not take a unit name".into(),
                    ));
                }
                Ok(())
            }
            _ => {
                let name = unit.as_deref().ok_or_else(|| {
                    RunnerError::Rejected("systemd unit name required".into())
                })?;
                validate_unit(name)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_injection_tokens() {
        for bad in [";id", "$(id)", "`id`", "a|b", "a&b", "a\nb", "x y"] {
            assert!(reject_shell_meta(bad).is_err(), "{bad}");
        }
        assert!(reject_shell_meta("-p").is_ok());
        assert!(reject_shell_meta("tcp").is_ok());
    }

    #[test]
    fn tokenize_rule_keeps_literal_tokens() {
        let args = tokenize_iptables_rule("-p tcp --dport 22 -j ACCEPT").unwrap();
        assert_eq!(args, vec!["-p", "tcp", "--dport", "22", "-j", "ACCEPT"]);
        assert!(tokenize_iptables_rule("-p tcp; id").is_err());
        assert!(tokenize_iptables_rule("-p $(reboot)").is_err());
    }

    #[test]
    fn unit_names_reject_paths() {
        assert!(validate_unit("nginx.service").is_ok());
        assert!(validate_unit("../etc/passwd").is_err());
        assert!(validate_unit("/bin/sh").is_err());
        assert!(validate_unit("evil;reboot").is_err());
    }
}

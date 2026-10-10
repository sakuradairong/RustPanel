//! Restricted privileged command runner (RP-07 / C.6 / C.11).
//!
//! M0 ships an in-process executor with action whitelist, argument validation,
//! timeouts, and output caps. Out-of-process Unix-socket runner + SO_PEERCRED
//! remains the deployment target (separate unit); call sites already go through
//! this boundary so they do not invent `sh -c` strings.
pub mod action;
pub mod audited;
pub mod runner;
pub mod validate;

pub use action::{Action, SystemctlVerb};
pub use audited::{execute_audited, AuditedError};
pub use runner::{execute, RunnerError, RunnerRequest, RunnerResult};

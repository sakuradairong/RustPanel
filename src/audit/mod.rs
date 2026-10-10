//! High-risk audit intents, outcomes, and delivery outbox (RP-09 / C.10).
mod mask;
mod store;

pub use mask::mask_value;
pub use store::{
    begin_intent, complete_intent, list_pending_outbox, mark_outbox_delivered,
    mark_outbox_retry, AuditError, IntentSpec, Outcome,
};

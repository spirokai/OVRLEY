//! Deferred replies to agent tool calls (Phase 2 D13, Phase 3 D15).
//!
//! A tool call is one dispatch. When its result is not known by the end of that dispatch (it waits
//! for a worker or for the user), the action keeps an [`AgentReply`] and answers later, on the UI
//! thread. Dropping a reply unanswered answers that the call was cancelled.

use serde_json::Value;

/// A tool's result: its value, or the error the agent reads.
pub type AgentResult = Result<Value, String>;

/// What an agent reads when the call it waits for is dropped without an answer.
pub const CANCELLED: &str = "The request was cancelled before OVRLEY could answer.";

/// Answers one tool call, once.
pub struct AgentReply(Option<Box<dyn FnOnce(AgentResult)>>);

impl AgentReply {
    pub fn new(reply: impl FnOnce(AgentResult) + 'static) -> Self {
        Self(Some(Box::new(reply)))
    }

    pub fn send(mut self, result: AgentResult) {
        if let Some(reply) = self.0.take() {
            reply(result);
        }
    }
}

impl Drop for AgentReply {
    fn drop(&mut self) {
        if let Some(reply) = self.0.take() {
            reply(Err(CANCELLED.into()));
        }
    }
}

impl std::fmt::Debug for AgentReply {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AgentReply")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn a_reply_answers_once_and_a_dropped_reply_cancels() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        AgentReply::new(move |result| log.borrow_mut().push(result)).send(Ok(Value::Bool(true)));
        let log = seen.clone();
        drop(AgentReply::new(move |result| log.borrow_mut().push(result)));
        assert_eq!(*seen.borrow(), [Ok(Value::Bool(true)), Err(CANCELLED.to_string())]);
    }
}

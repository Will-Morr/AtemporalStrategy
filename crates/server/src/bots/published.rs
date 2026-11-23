//! Trusted adapter from the published controller ledger to the strategy interface.
use super::{PolicyTimeline, history::PublishedHistory};
use crate::controller::Controller;
use atemporal_bots::{History, MatchRules, PublishedWindow};
use atemporal_sim::{PlayerId, Result};
use std::sync::{Arc, atomic::AtomicBool};

pub(super) struct PublishedRequest {
    rules: MatchRules,
    window: PublishedWindow,
    history: PublishedHistory,
}

impl PublishedRequest {
    pub(super) fn new(c: &Controller, player: PlayerId, cancel: Arc<AtomicBool>) -> Result<Self> {
        let history = PublishedHistory::capture(c, player, cancel)?;
        Ok(Self {
            rules: MatchRules {
                config: c.config.clone(),
                content: c.content.clone(),
            },
            window: PublishedWindow {
                revision: c.current,
                player,
                editable: c.editable_from..=c.available_through(),
            },
            history,
        })
    }

    pub(super) fn build(self) -> Result<PolicyTimeline> {
        let timeline = self.history.analyze(self.window.revision)?;
        PolicyTimeline::new(timeline, Box::new(self.history), self.rules, self.window)
    }
}

/// Materialize the same published input used by live bots for an offline caller.
pub fn published_timeline(c: &Controller, player: PlayerId) -> Result<PolicyTimeline> {
    PublishedRequest::new(c, player, Arc::default())?.build()
}

//! Read-only access to the timelines a player can inspect in the round picker.
//! Each historical query projects a published execution from its own opening.
use crate::controller::Controller;
use atemporal_bots::{History, PublishedRound, TurnMarker};
use atemporal_sim::{
    PlayerId, Result, Revision, SimRequest, Tick,
    player_timeline::{self, PlayerTimeline},
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct Entry {
    summary: PublishedRound,
    final_hash: String,
    minimum_end_tick: Tick,
    turns: usize,
    precedence: usize,
}

pub(crate) struct PublishedHistory {
    // Every published revision extends its parent's ledger. Keep one copy of
    // the opening and ledger, plus the published prefix lengths for each round.
    latest: SimRequest,
    entries: BTreeMap<Revision, Entry>,
    markers: Vec<TurnMarker>,
    player: PlayerId,
    cancel: Arc<AtomicBool>,
}

impl PublishedHistory {
    pub(super) fn capture(
        c: &Controller,
        player: PlayerId,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self> {
        if player >= c.config.player_count {
            return Err("player outside match".into());
        }
        let latest = c.replay_request(c.current)?;
        let mut entries = BTreeMap::new();
        for (&revision, data) in c.revisions.range(..=c.current) {
            let Some(outcome) = &data.outcome else {
                continue;
            };
            entries.insert(
                revision,
                Entry {
                    final_hash: data.final_hash.clone(),
                    minimum_end_tick: c.replay_minimum_end_tick(revision)?,
                    summary: PublishedRound {
                        revision,
                        parent: data.parent,
                        round: data.round,
                        outcome: outcome.clone(),
                        score: data.score.clone(),
                        timed: data.timed.clone(),
                    },
                    turns: data.turns.len(),
                    precedence: data.precedence.len(),
                },
            );
        }
        let markers = latest
            .events
            .iter()
            .map(|turn| TurnMarker::for_player(turn, player))
            .collect();
        Ok(Self {
            latest,
            entries,
            markers,
            player,
            cancel,
        })
    }
}
impl History for PublishedHistory {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn revisions(&self) -> Box<dyn Iterator<Item = Revision> + '_> {
        Box::new(self.entries.keys().copied())
    }

    fn round(&self, revision: Revision) -> Option<&PublishedRound> {
        self.entries.get(&revision).map(|e| &e.summary)
    }

    fn turns(&self, revision: Revision) -> Option<&[TurnMarker]> {
        let entry = self.entries.get(&revision)?;
        self.markers.get(..entry.turns)
    }

    /// Analyze a published revision for this seat, including evicted replay
    /// bodies. The result is independent of subsequent revisions' sightings.
    fn analyze(&self, revision: Revision) -> Result<PlayerTimeline> {
        if self.cancelled() {
            return Err("canceled".into());
        }
        let entry = self
            .entries
            .get(&revision)
            .ok_or("revision is not published")?;
        let terminal = entry.summary.outcome.terminal_state_tick;
        let request = if revision == self.latest.revision {
            Cow::Borrowed(&self.latest)
        } else {
            let mut request = self.latest.clone();
            request.revision = revision;
            request.events.truncate(entry.turns);
            request.precedence.truncate(entry.precedence);
            request.minimum_end_tick = entry.minimum_end_tick;
            Cow::Owned(request)
        };
        player_timeline::build_verified(
            &request,
            &[self.player],
            &self.cancel,
            terminal,
            &entry.final_hash,
        )?
        .remove(&self.player)
        .ok_or_else(|| "player timeline missing".into())
    }
}

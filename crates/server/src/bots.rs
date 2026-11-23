//! Deliberately small, deterministic opponents. Policies receive a visibility-filtered published
//! state and emit normal drafts; the controller validates them exactly like human input.
use crate::{controller::Controller, coordinator::atomic_json, ws};
use atemporal_sim::*;
pub mod history;
mod published;
pub use atemporal_bots::{DEFAULT_STYLE, PolicyTimeline, Style, intervention, published_plan};
pub use published::published_timeline;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotConfig {
    #[serde(default)]
    pub slots: Vec<Option<Style>>,
    #[serde(default = "default_rounds")]
    pub playtest_rounds: u32,
}
fn default_rounds() -> u32 {
    20
}
impl BotConfig {
    pub(crate) fn all_bot(&self) -> bool {
        !self.slots.is_empty() && self.slots.iter().all(Option::is_some)
    }

    pub fn validate(&self, count: u8) -> Result<()> {
        if !self.slots.is_empty() && self.slots.len() != usize::from(count) {
            return Err("Choose a human or bot for each slot.".into());
        }
        if self.slots.first().is_some_and(Option::is_some) && self.slots.iter().any(Option::is_none)
        {
            return Err("Keep slot 0 human in games with human players.".into());
        }
        if self.all_bot() && !(1..=1000).contains(&self.playtest_rounds) {
            return Err("Bot-only playtests need a round budget between 1 and 1000.".into());
        }
        Ok(())
    }
    pub fn install(&self, c: &mut Controller) -> Result<()> {
        self.validate(c.config.player_count)?;
        for (slot, style) in self.slots.iter().enumerate() {
            if let Some(style) = style {
                let player = slot as u8;
                if c.bot_token(player).is_none() {
                    let team = c
                        .lobby
                        .available_teams
                        .iter()
                        .find(|t| {
                            t.capacity.is_none_or(|cap| {
                                c.lobby
                                    .slots
                                    .iter()
                                    .filter(|s| {
                                        s.profile
                                            .as_ref()
                                            .is_some_and(|p| p.team_id.as_ref() == Some(&t.team_id))
                                    })
                                    .count()
                                    < usize::from(cap)
                            })
                        })
                        .map(|t| t.team_id.clone());
                    let team = match &c.config.multiplayer {
                        Multiplayer::Teams { assignments } => assignments
                            .iter()
                            .find(|a| a.player_id == player)
                            .map(|a| a.team_id.clone())
                            .or(team),
                        _ => team,
                    };
                    c.claim_slot(
                        player,
                        format!("Bot · {}", style.name()),
                        ["#ff8a65", "#aed581", "#ce93d8", "#ffd54f"][slot % 4].into(),
                        team,
                    )?;
                }
                c.mark_bot_connected(player);
            }
        }
        Ok(())
    }
}

pub(crate) fn status_record(c: &Controller) -> serde_json::Value {
    serde_json::json!({"last_activity_ms":c.last_activity_ms,"match_id":c.match_id,"phase":c.phase,"round":c.round,"slots":c.lobby.slots,"lobby_revision":c.lobby.revision,"objective":c.config.objective})
}

pub(crate) fn has_pending_bot_work(c: &Controller, bots: &BotConfig, debug_opening: bool) -> bool {
    (bots.all_bot() && matches!(c.phase, Phase::Lobby | Phase::Planning))
        || (c.phase == Phase::Planning
            && ((debug_opening && c.round == 1)
                || ((0..c.config.player_count).any(|player| {
                    bots.slots
                        .get(usize::from(player))
                        .is_none_or(Option::is_none)
                        && c.has_commit(player)
                }) && bots
                    .slots
                    .iter()
                    .enumerate()
                    .any(|(slot, bot)| bot.is_some() && c.bot_pending(slot as PlayerId)))))
}
struct BotAttempt {
    match_id: String,
    revision: Revision,
    round: u32,
    draft: Option<TurnDraft>,
    failures: u32,
    retry_at: Instant,
}

const BOT_POLL: Duration = Duration::from_millis(750);
const RECONNECT_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
enum PlanningDemand {
    #[default]
    Unobserved,
    Needed,
    Absent(Instant),
}

impl PlanningDemand {
    fn allows(&mut self, c: &Controller, bots: &BotConfig, debug_opening: bool) -> bool {
        let needed = bots.all_bot()
            || (debug_opening && c.round == 1)
            || c.lobby.slots.iter().enumerate().any(|(slot, state)| {
                bots.slots.get(slot).is_none_or(Option::is_none)
                    && (state.connected || c.has_commit(slot as u8))
            });
        if needed {
            *self = Self::Needed;
            return true;
        }
        // An already empty worker can stay quiet immediately after startup.
        if matches!(self, Self::Needed) {
            *self = Self::Absent(Instant::now());
        }
        matches!(self, Self::Absent(since) if since.elapsed() < RECONNECT_GRACE)
    }
}

impl BotAttempt {
    fn new(match_id: String, revision: Revision, round: u32) -> Self {
        Self {
            match_id,
            revision,
            round,
            draft: None,
            failures: 0,
            retry_at: Instant::now(),
        }
    }

    fn is_current(&self, c: &Controller, player: PlayerId) -> bool {
        c.match_id == self.match_id
            && c.current == self.revision
            && c.round == self.round
            && c.bot_pending(player)
    }

    fn defer(&mut self) {
        self.failures = (self.failures + 1).min(6);
        self.retry_at = Instant::now() + Duration::from_secs((1 << self.failures).min(60));
    }
}

/// Drive pending bots and write the coordinator's status and open-lobby recovery records.
/// Timeline projection runs away from the controller lock and handles one bot at a time.
pub fn drive(
    app: ws::App,
    bots: BotConfig,
    status: Option<PathBuf>,
    lobby_file: Option<PathBuf>,
    debug_opening: bool,
) {
    if bots.slots.is_empty() && status.is_none() && lobby_file.is_none() {
        return;
    }
    // Restore wall-clock activity; restarting must not renew abandoned games.
    if let Some(path) = &status
        && let Ok(bytes) = std::fs::read(path)
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(last) = value["last_activity_ms"].as_u64()
    {
        app.controller.lock().unwrap().last_activity_ms = last;
    }
    // A bot can spend a full replay computing its next turn. Keep advertising
    // the actual match phase to the coordinator throughout that computation.
    if status.is_some() || lobby_file.is_some() {
        let controller = app.controller.clone();
        tokio::spawn(async move {
            let mut saved = String::new();
            loop {
                {
                    let c = controller.lock().unwrap();
                    let value = status_record(&c);
                    let text = value.to_string();
                    if text != saved {
                        if let Some(path) = &status
                            && let Err(e) = atomic_json(path, &value)
                        {
                            eprintln!("worker status: {e}");
                        }
                        if c.phase == Phase::Lobby
                            && let Some(path) = &lobby_file
                            && let Err(e) = atomic_json(path, &c.open_lobby_record())
                        {
                            eprintln!("lobby persistence: {e}");
                        }
                        saved = text;
                    }
                }
                tokio::time::sleep(Duration::from_millis(750)).await;
            }
        });
    }
    if bots.slots.iter().any(Option::is_some) || debug_opening {
        tokio::spawn(async move {
            let mut attempts = BTreeMap::new();
            let mut demand = PlanningDemand::default();
            loop {
                if let Err(e) = step(&app, &bots, &mut attempts, &mut demand, debug_opening).await {
                    eprintln!("bot: {e}");
                }
                tokio::time::sleep(BOT_POLL).await;
            }
        });
    }
}
async fn step(
    app: &ws::App,
    bots: &BotConfig,
    attempts: &mut BTreeMap<PlayerId, BotAttempt>,
    demand: &mut PlanningDemand,
    debug_opening: bool,
) -> Result<()> {
    if bots.all_bot() {
        let pending = {
            let mut c = app.controller.lock().unwrap();
            if c.phase == Phase::Lobby && c.lobby.can_start {
                let token = c.bot_token(0).unwrap();
                let revision = c.lobby.revision;
                c.start_match(&token, revision)?;
                Some(c.start_job(0, vec![], vec![])?)
            } else {
                if c.phase == Phase::Planning && c.round > bots.playtest_rounds {
                    let token = c.bot_token(0).unwrap();
                    let revision = c.current;
                    c.stop_and_archive(&token, "bot-playtest-budget".into(), revision)?;
                }
                None
            }
        };
        if let Some(rx) = pending {
            ws::drive_job(app.clone(), rx);
        }
    }
    {
        let c = app.controller.lock().unwrap();
        if !demand.allows(&c, bots, debug_opening) {
            return Ok(());
        }
    }
    for (slot, style) in bots.slots.iter().enumerate() {
        let player = slot as u8;
        let (match_id, revision, round, style) = {
            let c = app.controller.lock().unwrap();
            // The human owns slot 0. DEBUG_TEST commits on its behalf only in round 1.
            let opening = debug_opening && slot == 0 && c.round == 1;
            let Some(style) = (*style).or_else(|| opening.then_some(DEFAULT_STYLE)) else {
                continue;
            };
            if !c.bot_pending(player) {
                continue;
            }
            (c.match_id.clone(), c.current, c.round, style)
        };
        let attempt = attempts
            .entry(player)
            .or_insert_with(|| BotAttempt::new(match_id.clone(), revision, round));
        if attempt.match_id != match_id || attempt.revision != revision || attempt.round != round {
            *attempt = BotAttempt::new(match_id, revision, round);
        }
        if Instant::now() < attempt.retry_at {
            continue;
        }
        if let Err(error) =
            step_player(app, player, style, attempt, demand, bots, debug_opening).await
        {
            attempt.defer();
            eprintln!(
                "bot {} player {player} revision {revision}: {error}",
                style.name()
            );
        }
    }
    Ok(())
}

async fn step_player(
    app: &ws::App,
    player: PlayerId,
    style: Style,
    attempt: &mut BotAttempt,
    demand: &mut PlanningDemand,
    bots: &BotConfig,
    debug_opening: bool,
) -> Result<()> {
    let revision = attempt.revision;
    if attempt.draft.is_none() {
        let cancel = Arc::new(AtomicBool::new(false));
        let published = {
            let c = app.controller.lock().unwrap();
            if !attempt.is_current(&c, player) || !demand.allows(&c, bots, debug_opening) {
                return Ok(());
            }
            published::PublishedRequest::new(&c, player, cancel.clone())?
        };
        let mut task = tokio::task::spawn_blocking(move || {
            published
                .build()
                .map(|timeline| published_plan(&timeline, style))
        });
        let draft = loop {
            tokio::select! {
                result = &mut task => break result.map_err(|e| e.to_string())??,
                () = tokio::time::sleep(BOT_POLL) => {
                    let needed = {
                        let c = app.controller.lock().unwrap();
                        demand.allows(&c, bots, debug_opening) && attempt.is_current(&c, player)
                    };
                    if !needed {
                        cancel.store(true, Ordering::Relaxed);
                        // Wait for cancellation before allowing another plan to start.
                        let _ = task.await.map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
            }
        };
        attempt.draft = Some(draft);
    }
    let draft = attempt.draft.as_ref().unwrap().clone();
    let tick = draft.tick;
    let (cached, exact) = {
        let c = app.controller.lock().unwrap();
        if !attempt.is_current(&c, player) || !demand.allows(&c, bots, debug_opening) {
            return Ok(());
        }
        let cached = c.cached_exact(revision, tick);
        let exact = if cached.is_none() {
            Some((c.exact_request(revision, tick)?, c.sim.clone()))
        } else {
            None
        };
        (cached, exact)
    };
    let state = match (cached, exact) {
        (Some(state), _) => state,
        (_, Some((request, sim))) => sim.exact(request, tick).await?,
        _ => unreachable!(),
    };
    let job = {
        let mut c = app.controller.lock().unwrap();
        if !attempt.is_current(&c, player) || !demand.allows(&c, bots, debug_opening) {
            return Ok(());
        }
        let mut request = CommitRequest {
            request_id: format!("bot-{player}-{}", c.round),
            slot_token: c.bot_token(player).ok_or("bot slot lost")?,
            draft,
        };
        c.precheck_commit(player, &request)?;
        let mut commands = draft::resolve_local_references(&request.draft, c.round, player)?;
        if let Err(error) = c.validate_against_state(player, &commands, &state) {
            // An occupied or unavailable site is an ordinary failed plan. Pass this turn.
            eprintln!("bot {} plan rejected; passing: {error}", style.name());
            request.draft.commands.clear();
            commands.clear();
        }
        let round = c.round;
        c.planning_ready(player, round);
        c.accept_commit(player, &request, commands)?.1
    };
    if let Some(rx) = job {
        ws::drive_job(app.clone(), rx);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::SimThread;
    use atemporal_bots::{FrameRules, plan_frame};
    use atemporal_sim::player_timeline::{PlayerFrame, project};

    fn visible_state(
        state: &WorldState,
        player: PlayerId,
        config: &MatchConfig,
        content: &Content,
    ) -> PlayerFrame {
        project(state, player, config, content).unwrap()
    }
    fn plan(
        state: &WorldState,
        view: &PlayerFrame,
        player: PlayerId,
        style: Style,
        config: &MatchConfig,
        content: &Content,
        revision: Revision,
    ) -> TurnDraft {
        plan_frame(
            view,
            player,
            style,
            FrameRules {
                config,
                content,
                terrain: &state.terrain,
            },
            revision,
        )
    }
    fn setup() -> (Controller, WorldState) {
        let yaml = include_str!("../../../config/game.yaml");
        let content_yaml = include_str!("../../../config/content.yaml");
        let mut setup = atemporal_content::load_setup(yaml).unwrap();
        setup.match_defaults.seed = 42u64.try_into().unwrap();
        setup.match_defaults.simulation_threads = 1;
        let content = atemporal_content::load_content(content_yaml).unwrap();
        let state = map::generate(&setup.match_defaults, &content).unwrap();
        let mut c = Controller::new(
            setup,
            content,
            yaml.into(),
            content_yaml.into(),
            "/guide/".into(),
            std::env::temp_dir().join("atemporal-bot-test"),
            SimThread::spawn(),
        )
        .unwrap();
        c.round = 1;
        (c, state)
    }
    #[test]
    fn openings_are_deterministic_valid_normal_commands() {
        let (c, state) = setup();
        {
            let style = DEFAULT_STYLE;
            let view = visible_state(&state, 1, &c.config, &c.content);
            let draft = plan(&state, &view, 1, style, &c.config, &c.content, 0);
            assert_eq!(
                draft,
                plan(&state, &view, 1, style, &c.config, &c.content, 0)
            );
            assert!(draft.commands.iter().any(|d| matches!(
                d.command,
                Command::AssignOrder {
                    order: Order::Mine { .. },
                    ..
                }
            )));
            assert!(
                draft
                    .commands
                    .iter()
                    .any(|d| matches!(d.command, Command::PlaceBlueprints { .. }))
            );
            for command in &draft.commands {
                draft::validate_policy(command, Some(100)).unwrap();
            }
            let commands = draft::resolve_local_references(&draft, 1, 1).unwrap();
            c.validate_against_state(1, &commands, &state).unwrap();
            let mut config = c.config.clone();
            config.control_limit = ControlLimit::SingleOrder;
            let single = plan(&state, &view, 1, style, &config, &c.content, 0);
            assert_eq!(single.commands.len(), 1);
        }
    }
    #[test]
    fn legacy_policy_and_tactical_opening_remain_selectable() {
        let (c, state) = setup();
        assert_eq!(
            serde_json::from_str::<Style>("\"standard\"").unwrap(),
            Style::Standard
        );
        assert_eq!(
            serde_json::to_string(&DEFAULT_STYLE).unwrap(),
            "\"tactical\""
        );
        let view = visible_state(&state, 1, &c.config, &c.content);
        let legacy = plan(&state, &view, 1, Style::Standard, &c.config, &c.content, 0);
        let tactical = plan(&state, &view, 1, Style::Tactical, &c.config, &c.content, 0);
        let structure_count = |draft: &TurnDraft, key: &str| {
            draft
                .commands
                .iter()
                .filter(
                    |d| matches!(&d.command,Command::PlaceBlueprints{type_key,..} if type_key==key),
                )
                .count()
        };
        assert_eq!(structure_count(&legacy, "factory"), 1);
        assert_eq!(structure_count(&legacy, "silo"), 0);
        assert!(structure_count(&tactical, "factory") >= 3);
        assert!(structure_count(&tactical, "silo") >= 1);
        assert!(tactical.commands.iter().any(|d|matches!(&d.command, Command::ConfigureBlueprints{settings,..} if settings.queue==["artillery"] && settings.queue_loop_flags==[true])));
        let settings: Vec<_> = tactical
            .commands
            .iter()
            .filter_map(|d| {
                if let Command::ConfigureBlueprints { settings, .. } = &d.command {
                    Some(settings)
                } else {
                    None
                }
            })
            .collect();
        assert!(
            settings
                .iter()
                .any(|s| s.queue.iter().filter(|key| *key == "miner").count() >= 10)
        );
        assert!(
            settings
                .iter()
                .any(|s| matches!(s.order, Order::Construct { .. })
                    && s.queue.iter().any(|key| key == "constructor"))
        );
        let launches: Vec<_> = settings
            .iter()
            .filter_map(|s| s.silo_plan.as_ref())
            .flat_map(|plan| &plan.launches)
            .collect();
        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].type_key, "satellite");
    }
    #[test]
    fn hidden_enemy_state_cannot_change_a_plan() {
        let (c, state) = setup();
        let before = visible_state(&state, 1, &c.config, &c.content);
        assert!(before.visible_entities.is_empty());
        let mut changed = state.clone();
        changed.players[0].bank = 999_999.;
        changed.rng_state = "secret".into();
        for e in changed.entities.iter_mut().filter(|e| e.owner == 0) {
            e.hp = 1.;
            e.action = Order::AttackMove {
                destination: Tile { x: 10, y: 10 },
            };
        }
        let after = visible_state(&changed, 1, &c.config, &c.content);
        {
            let style = DEFAULT_STYLE;
            assert_eq!(
                plan(&state, &before, 1, style, &c.config, &c.content, 0),
                plan(&changed, &after, 1, style, &c.config, &c.content, 0)
            );
        }
        assert_eq!(before.player.player_id, 1);
    }
    #[test]
    fn visible_enemies_are_observations_without_owned_orders() {
        let (c, mut state) = setup();
        let home = state.entities.iter().find(|e| e.owner == 1).unwrap().tile;
        let enemy = state.entities.iter_mut().find(|e| e.owner == 0).unwrap();
        enemy.tile = home;
        enemy.action = Order::AttackMove {
            destination: Tile { x: 1, y: 1 },
        };
        let view = visible_state(&state, 1, &c.config, &c.content);
        assert!(view.visible_entities.iter().any(|e| e.owner == 0));
        assert!(view.own_entities.iter().all(|e| e.owner == 1));
    }
    fn published_opening() -> (Controller, String) {
        use crate::controller::tests::{controller, run_round};
        let mut c = controller(false, |s| {
            s.match_defaults.max_tick = 120;
            s.match_defaults.simulation_threads = 1;
        });
        let human = c
            .claim_slot(0, "Human".into(), "#4fc3f7".into(), None)
            .unwrap();
        BotConfig {
            slots: vec![None, Some(Style::Tactical)],
            playtest_rounds: 20,
        }
        .install(&mut c)
        .unwrap();
        c.start_match(&human, c.lobby.revision).unwrap();
        let job = c.start_job(0, vec![], vec![]).unwrap();
        run_round(&mut c, job);
        (c, human)
    }

    fn human_and_tactical() -> BotConfig {
        BotConfig {
            slots: vec![None, Some(Style::Tactical)],
            playtest_rounds: 20,
        }
    }

    #[test]
    fn disconnected_human_commit_keeps_bot_planning_needed() {
        let (mut c, human) = published_opening();
        let bots = human_and_tactical();
        let mut demand = PlanningDemand::default();
        c.disconnect(0);
        assert!(!demand.allows(&c, &bots, false));
        c.connect(Some(&human));
        assert!(demand.allows(&c, &bots, false));
        c.disconnect(0);
        assert!(demand.allows(&c, &bots, false));
        demand = PlanningDemand::Absent(Instant::now() - RECONNECT_GRACE);
        assert!(!demand.allows(&c, &bots, false));
        let request = CommitRequest {
            request_id: "accepted-before-disconnect".into(),
            slot_token: human,
            draft: TurnDraft {
                based_on_revision: c.current,
                tick: c.editable_from,
                commands: vec![],
            },
        };
        assert!(c.accept_commit(0, &request, vec![]).unwrap().1.is_none());
        assert!(demand.allows(&c, &bots, false));
        assert!(PlanningDemand::default().allows(
            &c,
            &BotConfig {
                slots: vec![Some(Style::Standard), Some(Style::Tactical)],
                playtest_rounds: 20,
            },
            false
        ));
    }

    #[test]
    fn abandoned_bot_stays_pending_until_a_human_reconnects() {
        let (mut c, human) = published_opening();
        c.disconnect(0);
        let app = ws::App {
            controller: Arc::new(std::sync::Mutex::new(c)),
            client_dir: PathBuf::new(),
            guide_dir: PathBuf::new(),
            inputs_only: false,
            managed: None,
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let bots = human_and_tactical();
                let mut demand = PlanningDemand::default();
                let mut attempts = BTreeMap::new();
                step(&app, &bots, &mut attempts, &mut demand, false)
                    .await
                    .unwrap();
                assert!(attempts.is_empty());
                assert!(app.controller.lock().unwrap().bot_pending(1));
                app.controller.lock().unwrap().connect(Some(&human));
                step(&app, &bots, &mut attempts, &mut demand, false)
                    .await
                    .unwrap();
                assert!(app.controller.lock().unwrap().has_commit(1));
            });
    }

    #[test]
    fn live_published_history_observes_shared_cancellation() {
        let (c, _) = published_opening();
        let canceled = Arc::new(AtomicBool::new(false));
        let input = published::PublishedRequest::new(&c, 1, canceled.clone())
            .unwrap()
            .build()
            .unwrap();
        assert!(!input.history().cancelled());
        canceled.store(true, Ordering::Relaxed);
        assert!(input.history().cancelled());
        assert_eq!(
            input.history().analyze(c.current).err().as_deref(),
            Some("canceled")
        );
    }

    #[test]
    fn hidden_only_changes_leave_complete_policy_and_history_identical() {
        use crate::controller::{RevisionData, collect_run};
        let (mut c, _) = published_opening();
        let revision = c.current;
        let before = published_timeline(&c, 1).unwrap();
        assert!(
            before
                .timeline()
                .frames()
                .all(|frame| frame.visible_entities.is_empty())
        );

        let mut request = c.replay_request(revision).unwrap();
        for hidden in request
            .checkpoint
            .entities
            .iter_mut()
            .filter(|entity| entity.owner == 0)
        {
            hidden.last_move_direction = if hidden.last_move_direction == Direction::N {
                Direction::S
            } else {
                Direction::N
            };
        }
        request.checkpoint.rng_state = "hidden-test-seed".into();
        let old = &c.revisions[&revision];
        let mut updated = RevisionData::new(revision, old.round, old.parent, old.base_tick);
        collect_run(&request, &mut updated, c.config.player_count).unwrap();
        assert_ne!(updated.final_hash, old.final_hash);
        assert_eq!(updated.outcome, old.outcome);
        c.revisions.insert(revision, updated);
        c.revisions
            .get_mut(&0)
            .unwrap()
            .checkpoints
            .insert(0, request.checkpoint);

        let after = published_timeline(&c, 1).unwrap();
        assert!(before.timeline().frames().eq(after.timeline().frames()));
        assert_eq!(
            before.timeline().public_stats,
            after.timeline().public_stats
        );
        for style in [Style::Standard, Style::Tactical] {
            assert_eq!(intervention(&before, style), intervention(&after, style));
            assert_eq!(
                published_plan(&before, style),
                published_plan(&after, style)
            );
        }

        let before_history = before.history().analyze(revision).unwrap();
        let after_history = after.history().analyze(revision).unwrap();
        assert!(before_history.frames().eq(after_history.frames()));
    }

    #[test]
    fn historical_analysis_survives_body_eviction_and_redacts_opposing_pass_tick() {
        use crate::controller::tests::run_round;

        let (mut c, human) = published_opening();
        let bot = c.bot_token(1).unwrap();
        let play_pass_round = |c: &mut Controller| {
            for (player, token, tick) in [(0, &human, 17), (1, &bot, 23)] {
                let request = CommitRequest {
                    request_id: format!("pass-{}-{player}", c.round),
                    slot_token: token.clone(),
                    draft: TurnDraft {
                        based_on_revision: c.current,
                        tick,
                        commands: vec![],
                    },
                };
                c.precheck_commit(player, &request).unwrap();
                if let Some(job) = c.accept_commit(player, &request, vec![]).unwrap().1 {
                    run_round(c, job);
                }
            }
        };

        play_pass_round(&mut c);
        let earlier_revision = c.current;
        assert_eq!(earlier_revision, 1);
        let earlier = published_timeline(&c, 1).unwrap();
        play_pass_round(&mut c);
        assert_eq!(c.current, earlier_revision + 1);
        c.memory_budget = 0;
        c.enforce_budgets(None);
        let evicted = &c.revisions[&earlier_revision];
        assert!(!evicted.loaded && evicted.checkpoints.is_empty());

        let latest = published_timeline(&c, 1).unwrap();
        let history = latest.history();
        let replay = history.analyze(earlier_revision).unwrap();
        assert!(earlier.timeline().frames().eq(replay.frames()));
        assert_eq!(earlier.timeline().turns, replay.turns);
        assert_eq!(replay.turns.len(), 1);

        let markers = history.turns(earlier_revision).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(
            markers.iter().find(|turn| turn.player == 0).unwrap().tick,
            None
        );
        assert_eq!(
            markers.iter().find(|turn| turn.player == 1).unwrap().tick,
            Some(23)
        );
        let unpublished = c.current + 1;
        assert!(history.round(unpublished).is_none());
        assert!(history.turns(unpublished).is_none());
        assert!(history.analyze(unpublished).is_err());
        assert!(earlier.history().analyze(c.current).is_err());
    }

    #[test]
    fn unpublished_human_orders_do_not_change_either_complete_policy() {
        let (mut c, human) = published_opening();
        let styles = [Style::Standard, Style::Tactical];
        let before = published_timeline(&c, 1).unwrap();
        let drafts = styles.map(|style| published_plan(&before, style));
        let ticks = styles.map(|style| intervention(&before, style));
        let tick = c.editable_from;
        let state = reconstruct(&c.exact_request(c.current, tick).unwrap(), tick).unwrap();
        let mover = state
            .entities
            .iter()
            .find(|e| {
                e.owner == 0
                    && c.content
                        .types
                        .iter()
                        .any(|d| d.key == e.type_key && d.movement.is_some())
            })
            .unwrap();
        let request = CommitRequest {
            request_id: "private-human-order".into(),
            slot_token: human,
            draft: TurnDraft {
                based_on_revision: c.current,
                tick,
                commands: vec![DraftCommand {
                    local_id: "move".into(),
                    future_orders: FutureOrderPolicy::Keep,
                    command: Command::AssignOrder {
                        entities: vec![mover.id.clone()],
                        order: Order::AttackMove {
                            destination: mover.tile,
                        },
                    },
                }],
            },
        };
        c.precheck_commit(0, &request).unwrap();
        let commands = draft::resolve_local_references(&request.draft, c.round, 0).unwrap();
        c.validate_against_state(0, &commands, &state).unwrap();
        assert!(c.accept_commit(0, &request, commands).unwrap().1.is_none());
        assert!(c.bot_pending(1));

        let after = published_timeline(&c, 1).unwrap();
        assert_eq!(drafts, styles.map(|style| published_plan(&after, style)));
        assert_eq!(ticks, styles.map(|style| intervention(&after, style)));
    }
    #[test]
    fn standard_bot_keeps_developing_across_eight_real_rounds() {
        use crate::controller::tests::{controller, run_round};
        let mut c = controller(false, |s| {
            s.match_defaults.max_tick = 1200;
            s.match_defaults.stop_when_decided = false;
            s.match_defaults.simulation_threads = 1;
            s.match_defaults.objective = Objective::Scoreboard {
                rules: ScoreboardRules {
                    victory_rule: VictoryRule::FixedTarget { points: 100. },
                    ..ScoreboardRules::default()
                },
            };
        });
        let human = c
            .claim_slot(0, "Human".into(), "#4fc3f7".into(), None)
            .unwrap();
        let bots = BotConfig {
            slots: vec![None, Some(Style::Standard)],
            playtest_rounds: 20,
        };
        bots.install(&mut c).unwrap();
        c.start_match(&human, c.lobby.revision).unwrap();
        let rx = c.start_job(0, vec![], vec![]).unwrap();
        run_round(&mut c, rx);
        let mut later_actions = 0;
        for round in 1..=8 {
            let timeline = published_timeline(&c, 1).unwrap();
            let bot_draft = published_plan(&timeline, Style::Standard);
            if round > 2 && !bot_draft.commands.is_empty() {
                later_actions += 1;
            }
            for (player, token, draft) in [
                (1, c.bot_token(1).unwrap(), bot_draft),
                (
                    0,
                    human.clone(),
                    TurnDraft {
                        based_on_revision: c.current,
                        tick: c.editable_from,
                        commands: vec![],
                    },
                ),
            ] {
                let request = CommitRequest {
                    request_id: format!("test-{round}-{player}"),
                    slot_token: token,
                    draft,
                };
                c.precheck_commit(player, &request).unwrap();
                let commands =
                    draft::resolve_local_references(&request.draft, c.round, player).unwrap();
                let exact = atemporal_sim::reconstruct(
                    &c.exact_request(c.current, request.draft.tick).unwrap(),
                    request.draft.tick,
                )
                .unwrap();
                c.validate_against_state(player, &commands, &exact).unwrap();
                if let Some(rx) = c.accept_commit(player, &request, commands).unwrap().1 {
                    run_round(&mut c, rx);
                }
            }
        }
        assert!(later_actions >= 4);
        let data = &c.revisions[&c.current];
        let max_count = |key: &str| {
            data.checkpoints
                .values()
                .map(|s| {
                    s.entities
                        .iter()
                        .filter(|e| {
                            e.owner == 1 && e.type_key == key && e.lifecycle == Lifecycle::Complete
                        })
                        .count()
                })
                .max()
                .unwrap()
        };
        assert!(max_count("miner") >= 3, "bot must grow its economy");
        assert!(max_count("factory") >= 2, "bot must expand production");
        assert!(max_count("grunt") >= 3, "bot must keep producing an army");
        let state = data
            .checkpoints
            .values()
            .find(|s| {
                s.tick >= 500
                    && s.entities.iter().any(|e| {
                        e.owner == 1
                            && e.type_key == "factory"
                            && e.lifecycle == Lifecycle::Complete
                    })
            })
            .unwrap()
            .clone();
        let mut threatened = state.clone();
        let home = threatened
            .entities
            .iter()
            .find(|e| e.owner == 1 && e.type_key == "factory")
            .unwrap()
            .tile;
        let enemy_tile = (0..threatened.terrain.height)
            .flat_map(|y| (0..threatened.terrain.width).map(move |x| Tile { x, y }))
            .find(|t| {
                i32::from(t.x.abs_diff(home.x)).pow(2) + i32::from(t.y.abs_diff(home.y)).pow(2)
                    <= 16
                    && threatened.terrain.cells[usize::from(t.y)
                        * usize::from(threatened.terrain.width)
                        + usize::from(t.x)]
                        == TerrainCell::Floor
                    && !threatened.entities.iter().any(|e| e.tile == *t)
            })
            .unwrap();
        let mut enemy = c.revisions[&0].checkpoints[&0]
            .entities
            .iter()
            .find(|e| e.owner == 0)
            .unwrap()
            .clone();
        threatened.entities.retain(|e| e.owner != 0);
        enemy.tile = enemy_tile;
        enemy.type_key = "grunt".into();
        enemy.lifecycle = Lifecycle::Complete;
        threatened.entities.push(enemy);
        let view = visible_state(&threatened, 1, &c.config, &c.content);
        let defense = plan(
            &threatened,
            &view,
            1,
            Style::Standard,
            &c.config,
            &c.content,
            c.current,
        );
        assert!(defense.commands.iter().any(
            |d| matches!(&d.command,Command::PlaceBlueprints{type_key,..} if type_key=="turret")
        ));
        assert!(defense.commands.iter().any(|d| matches!(&d.command,Command::AssignOrder{order:Order::AttackMove{destination},..} if *destination==enemy_tile)));
        let commands = draft::resolve_local_references(&defense, c.round, 1).unwrap();
        c.validate_against_state(1, &commands, &threatened).unwrap();
        let mut depleted = state.clone();
        depleted
            .entities
            .retain(|e| e.owner != 1 || (e.type_key != "miner" && e.type_key != "constructor"));
        let view = visible_state(&depleted, 1, &c.config, &c.content);
        let recovery = plan(
            &depleted,
            &view,
            1,
            Style::Standard,
            &c.config,
            &c.content,
            c.current,
        );
        assert!(recovery.commands.iter().any(|d| matches!(&d.command,Command::EditProduction{edit:ProductionEdit::Append{items},..} if items.contains(&"constructor".to_string()) && items.contains(&"miner".to_string()))));
        let commands = draft::resolve_local_references(&recovery, c.round, 1).unwrap();
        c.validate_against_state(1, &commands, &depleted).unwrap();
        // Own published elimination must trigger a rewind before the loss, not a turn after death.
        let data = c.revisions.get_mut(&c.current).unwrap();
        let outcome = data.outcome.as_mut().unwrap();
        outcome.eliminated = vec![1];
        outcome.survival_transitions.push(SurvivalTransition {
            player_id: 1,
            resolved_tick: 400,
            status: SurvivalStatus::Eliminated,
            reasons: vec![Reason::NoBuildAbility],
        });
        assert!(intervention(&published_timeline(&c, 1).unwrap(), Style::Standard) <= 200);
    }
    #[test]
    fn bot_only_budget_and_human_host_are_explicit() {
        assert!(
            BotConfig {
                slots: vec![Some(Style::Standard), None],
                playtest_rounds: 20
            }
            .validate(2)
            .is_err()
        );
        assert!(
            BotConfig {
                slots: vec![None, Some(Style::Standard)],
                playtest_rounds: 20
            }
            .validate(2)
            .is_ok()
        );
        assert!(
            BotConfig {
                slots: vec![Some(Style::Standard); 2],
                playtest_rounds: 0
            }
            .validate(2)
            .is_err()
        );
    }
}

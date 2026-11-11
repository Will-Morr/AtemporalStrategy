//! Deliberately small, deterministic opponents. Policies receive a visibility-filtered published
//! state and emit normal drafts; the controller validates them exactly like human input.
use crate::{controller::Controller, coordinator::atomic_json, ws};
use atemporal_sim::*;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    #[serde(alias = "rusher", alias = "builder", alias = "raider")]
    Standard,
}
/// One policy for quick play and selectable practice opponents.
pub const DEFAULT_STYLE: Style = Style::Standard;
impl Style {
    fn name(self) -> &'static str {
        "Standard"
    }
    fn interval(self) -> Tick {
        150
    }
    fn unit(self) -> &'static str {
        "grunt"
    }
}
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
    pub fn validate(&self, count: u8) -> Result<()> {
        if !self.slots.is_empty() && self.slots.len() != usize::from(count) {
            return Err("Choose a human or bot for each slot.".into());
        }
        if self.slots.first().is_some_and(Option::is_some) && self.slots.iter().any(Option::is_none)
        {
            return Err("Keep slot 0 human in games with human players.".into());
        }
        if !self.slots.is_empty()
            && self.slots.iter().all(Option::is_some)
            && !(1..=1000).contains(&self.playtest_rounds)
        {
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
fn allied(config: &MatchConfig, a: PlayerId, b: PlayerId) -> bool {
    if a == b {
        return true;
    }
    match &config.multiplayer {
        Multiplayer::Ffa {} => false,
        Multiplayer::Teams { assignments } => assignments
            .iter()
            .find(|t| t.player_id == a)
            .zip(assignments.iter().find(|t| t.player_id == b))
            .is_some_and(|(a, b)| a.team_id == b.team_id),
    }
}
fn distance(a: Tile, b: Tile) -> u32 {
    let x = i32::from(a.x) - i32::from(b.x);
    let y = i32::from(a.y) - i32::from(b.y);
    (x * x + y * y) as u32
}
/// Terrain/ore are public map data. Hidden enemy entities, orders, banks and blueprints never
/// enter a policy. Replay display can reveal more; the bot still uses allied unit vision.
pub fn visible_state(
    state: &WorldState,
    player: PlayerId,
    config: &MatchConfig,
    content: &Content,
) -> WorldState {
    let sees = |tile: Tile| {
        state.entities.iter().any(|e| {
            e.lifecycle == Lifecycle::Complete
                && allied(config, player, e.owner)
                && content
                    .types
                    .iter()
                    .find(|t| t.key == e.type_key)
                    .is_some_and(|d| f64::from(distance(e.tile, tile)) <= d.vision * d.vision)
        }) || state.recon.iter().any(|z| {
            z.starts_at <= state.tick
                && z.expires_at > state.tick
                && allied(config, player, z.owner)
                && f64::from(distance(z.center, tile)) <= z.radius * z.radius
        })
    };
    let mut view = state.clone();
    view.entities
        .retain(|e| allied(config, player, e.owner) || sees(e.tile));
    // Enemy orders and production are not needed even when the enemy is visible.
    for e in &mut view.entities {
        if e.owner != player {
            e.action = Order::Idle {};
            e.production = None;
            e.engaged_target = None;
            e.support_target = None;
            e.local_detour.clear();
        }
    }
    view.blueprints.retain(|b| b.owner == player);
    view.players.retain(|p| p.player_id == player);
    view.control_groups.retain(|g| g.id.owner == player);
    view.survival_transitions.clear();
    view.missiles.clear();
    view.recon.clear();
    view.rng_state.clear();
    view
}
/// Select a useful checkpoint, rather than reacting to unpublished human drafts. Opening plans
/// begin at the editable boundary; later interventions advance modestly through the timeline.
pub fn intervention(c: &Controller, player: PlayerId, style: Style) -> Tick {
    let data = &c.revisions[&c.current];
    let last = data
        .turns
        .iter()
        .filter(|t| t.player == player)
        .map(|t| t.tick)
        .max();
    let wanted = last
        .map_or(c.editable_from, |t| t.saturating_add(style.interval()))
        .max(c.editable_from)
        .min(c.available_through())
        .min(c.config.max_tick - 1);
    if let Some(outcome) = &data.outcome
        && outcome.eliminated.contains(&player)
        && let Some(loss) = outcome
            .survival_transitions
            .iter()
            .rev()
            .find(|t| t.player_id == player && t.status == SurvivalStatus::Eliminated)
    {
        let rescue = loss.resolved_tick.saturating_sub(200).max(c.editable_from);
        if rescue < wanted {
            return rescue.min(c.available_through());
        }
    }
    // Prefer the first published checkpoint with a mobile army to redirect near the desired tick.
    data.checkpoints
        .range(wanted..c.config.max_tick)
        .find(|(_, s)| {
            s.entities.iter().any(|e| {
                e.owner == player
                    && c.content
                        .types
                        .iter()
                        .find(|t| t.key == e.type_key)
                        .is_some_and(|t| t.weapon.is_some() && t.movement.is_some())
            })
        })
        .filter(|(tick, _)| **tick <= wanted.saturating_add(style.interval()))
        .map_or(wanted, |(t, _)| *t)
}
pub fn plan(
    state: &WorldState,
    player: PlayerId,
    style: Style,
    config: &MatchConfig,
    content: &Content,
    revision: Revision,
) -> TurnDraft {
    let mut commands = vec![];
    let mut push = |command| {
        let local_id = format!("bot-{}", commands.len());
        let future_orders = if matches!(command, Command::AssignOrder { .. }) {
            FutureOrderPolicy::DropAll
        } else {
            FutureOrderPolicy::Keep
        };
        commands.push(DraftCommand {
            local_id: local_id.clone(),
            command,
            future_orders,
        });
        local_id
    };
    let own: Vec<_> = state
        .entities
        .iter()
        .filter(|e| e.owner == player && e.lifecycle == Lifecycle::Complete)
        .collect();
    let def = |e: &EntityState| content.types.iter().find(|t| t.key == e.type_key);
    let Some(home) = own
        .iter()
        .find(|e| e.type_key == "factory")
        .or_else(|| {
            own.iter()
                .find(|e| def(e).is_some_and(|t| t.construction.is_some()))
        })
        .or_else(|| own.first())
        .map(|e| e.tile)
    else {
        return TurnDraft {
            based_on_revision: revision,
            tick: state.tick,
            commands,
        };
    };
    let area = Rect {
        min: Tile { x: 0, y: 0 },
        max: Tile {
            x: state.terrain.width - 1,
            y: state.terrain.height - 1,
        },
    };
    let mut miners: Vec<_> = own
        .iter()
        .filter(|e| {
            def(e).is_some_and(|t| t.mining.is_some() && t.construction.is_none())
                && !matches!(e.action, Order::Mine { .. })
        })
        .map(|e| e.id.clone())
        .collect();
    miners.sort();
    if !miners.is_empty() {
        push(Command::AssignOrder {
            entities: miners,
            order: Order::Mine { area: area.clone() },
        });
    }
    let target = state
        .entities
        .iter()
        .filter(|e| !allied(config, player, e.owner))
        .min_by_key(|e| distance(e.tile, home))
        .map(|e| e.tile)
        .unwrap_or_else(|| {
            let desired = Tile {
                x: state.terrain.width - 1 - home.x,
                y: state.terrain.height - 1 - home.y,
            };
            (0..state.terrain.height)
                .flat_map(|y| (0..state.terrain.width).map(move |x| Tile { x, y }))
                .filter(|t| {
                    state.terrain.cells
                        [usize::from(t.y) * usize::from(state.terrain.width) + usize::from(t.x)]
                        == TerrainCell::Floor
                })
                .min_by_key(|t| distance(*t, desired))
                .unwrap_or(home)
        });
    let recipe = content
        .types
        .iter()
        .find(|t| t.key == "factory")
        .and_then(|d| d.production.as_ref())
        .and_then(|p| {
            p.recipes
                .iter()
                .find(|k| k.as_str() == style.unit())
                .or_else(|| p.recipes.first())
        })
        .cloned();
    let count = |key: &str| {
        state
            .entities
            .iter()
            .filter(|e| e.owner == player && e.type_key == key)
            .count()
            + state
                .blueprints
                .iter()
                .filter(|b| b.owner == player && b.type_key == key && b.site_id.is_none())
                .count()
    };
    let factory_count = count("factory");
    let bank = state
        .players
        .iter()
        .find(|p| p.player_id == player)
        .map_or(0., |p| p.bank);
    let threatened = state.entities.iter().any(|e| {
        !allied(config, player, e.owner)
            && def(e).is_some_and(|t| t.weapon.is_some())
            && distance(e.tile, home) <= 144
    });
    let structure = if factory_count == 0 {
        Some("factory")
    } else if threatened && count("turret") < 2 {
        Some("turret")
    } else if factory_count < 2 && state.tick >= 300 && (bank >= 200. || count("miner") >= 3) {
        Some("factory")
    } else {
        None
    };
    let constructors: Vec<_> = own
        .iter()
        .filter(|e| def(e).is_some_and(|t| t.construction.is_some()))
        .collect();
    if let Some(structure) = structure.filter(|_| !constructors.is_empty() && recipe.is_some()) {
        let site = (0..state.terrain.height)
            .flat_map(|y| (0..state.terrain.width.saturating_sub(1)).map(move |x| Tile { x, y }))
            .filter(|t| {
                let i = usize::from(t.y) * usize::from(state.terrain.width) + usize::from(t.x);
                distance(*t, home) <= 64
                    && state.terrain.cells[i] == TerrainCell::Floor
                    && state.terrain.cells[i + 1] == TerrainCell::Floor
                    && state.ore[i] == 0.0
                    && !state.entities.iter().any(|e| e.tile == *t)
                    && !state.blueprints.iter().any(|b| b.tile == *t)
                    && !state
                        .entities
                        .iter()
                        .any(|e| e.production.as_ref().is_some_and(|p| p.output_tile == *t))
                    && !state
                        .entities
                        .iter()
                        .any(|e| e.tile == (Tile { x: t.x + 1, y: t.y }))
                    && !state
                        .blueprints
                        .iter()
                        .any(|b| b.tile == (Tile { x: t.x + 1, y: t.y }))
            })
            .min_by_key(|t| (distance(*t, home), t.y, t.x));
        if let Some(tile) = site {
            let placement = push(Command::PlaceBlueprints {
                type_key: structure.into(),
                tiles: vec![tile],
                priority: Priority::High,
                output_directions: (structure == "factory").then(|| vec![CardinalDirection::E]),
            });
            // The following reference is resolved by the ordinary draft machinery.
            if structure == "factory" {
                let mut queue = vec![];
                if factory_count == 0 && content.types.iter().any(|t| t.key == "miner") {
                    queue.extend(["miner".into(), "miner".into()]);
                }
                queue.push(recipe.clone().unwrap());
                let flags = (0..queue.len()).map(|i| i + 1 == queue.len()).collect();
                push(Command::ConfigureBlueprints {
                    blueprint_ids: vec![DraftItemRef::Draft {
                        local_id: placement,
                        item_index: 0,
                    }],
                    settings: BlueprintSettings {
                        queue,
                        queue_loop_flags: flags,
                        order: if factory_count == 0 {
                            Order::Mine { area: area.clone() }
                        } else {
                            Order::AttackMove {
                                destination: target,
                            }
                        },
                        priority: Priority::High,
                        loop_enabled: true,
                        silo_plan: None,
                    },
                });
            }
        }
    }
    let mut builders: Vec<_> = constructors
        .iter()
        .filter(|e| !matches!(e.action, Order::Construct { .. }))
        .map(|e| e.id.clone())
        .collect();
    builders.sort();
    if !builders.is_empty() {
        push(Command::AssignOrder {
            entities: builders,
            order: Order::Construct { area: area.clone() },
        });
    }
    let queued = |key: &str| {
        own.iter()
            .filter_map(|e| e.production.as_ref())
            .map(|p| {
                usize::from(p.active_item.as_ref().is_some_and(|i| i.type_key == key))
                    + p.pending_items.iter().filter(|i| i.type_key == key).count()
            })
            .sum::<usize>()
    };
    let mut utility = vec![];
    for (key, desired) in [("constructor", 1), ("miner", 3)] {
        if content.types.iter().any(|t| t.key == key) {
            for _ in count(key) + queued(key)..desired {
                utility.push(key.to_string());
            }
        }
    }
    for factory in own.iter().filter(|e| e.type_key == "factory") {
        if let Some(p) = &factory.production {
            let feeding_economy = !utility.is_empty()
                || p.active_item
                    .as_ref()
                    .is_some_and(|i| i.type_key == "miner" || i.type_key == "constructor")
                || p.pending_items
                    .iter()
                    .any(|i| i.type_key == "miner" || i.type_key == "constructor");
            let newborn_order = if feeding_economy {
                Order::Mine { area: area.clone() }
            } else {
                Order::AttackMove {
                    destination: target,
                }
            };
            let added_utility = !utility.is_empty();
            if added_utility {
                if p.loop_enabled {
                    push(Command::SetQueueLoop {
                        factories: vec![factory.id.clone()],
                        enabled: false,
                    });
                }
                push(Command::EditProduction {
                    factories: vec![factory.id.clone()],
                    edit: ProductionEdit::Append {
                        items: std::mem::take(&mut utility),
                    },
                });
            }
            if !p.loop_enabled || added_utility {
                push(Command::SetQueueLoop {
                    factories: vec![factory.id.clone()],
                    enabled: true,
                });
            }
            if p.active_item.is_none()
                && p.pending_items.is_empty()
                && let Some(recipe) = &recipe
            {
                push(Command::EditProduction {
                    factories: vec![factory.id.clone()],
                    edit: ProductionEdit::Append {
                        items: vec![recipe.clone()],
                    },
                });
            }
            if p.stored_order != newborn_order {
                push(Command::SetStoredOrder {
                    factories: vec![factory.id.clone()],
                    order: newborn_order,
                });
            }
        }
    }
    let mut army: Vec<_> = own
        .iter()
        .filter(|e| {
            def(e).is_some_and(|t| t.weapon.is_some() && t.movement.is_some())
                && (e.action
                    != (Order::AttackMove {
                        destination: target,
                    })
                    || e.goal_settled)
        })
        .map(|e| e.id.clone())
        .collect();
    army.sort();
    if !army.is_empty() {
        push(Command::AssignOrder {
            entities: army,
            order: Order::AttackMove {
                destination: target,
            },
        });
    }
    if config.control_limit == ControlLimit::SingleOrder {
        commands.truncate(1);
    }
    TurnDraft {
        based_on_revision: revision,
        tick: state.tick,
        commands,
    }
}

/// Also writes coordinator status and open-lobby recovery records. No world state is written to
/// the directory. Bots sleep between rounds and share the worker's existing simulation thread.
pub fn drive(app: ws::App, bots: BotConfig, status: Option<PathBuf>, lobby_file: Option<PathBuf>) {
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
    tokio::spawn(async move {
        let mut saved = String::new();
        loop {
            if let Err(e) = step(&app, &bots).await {
                eprintln!("bot: {e}");
            }
            {
                let c = app.controller.lock().unwrap();
                let value = serde_json::json!({"last_activity_ms":c.last_activity_ms,"match_id":c.match_id,"phase":c.phase,"round":c.round,"slots":c.lobby.slots,"lobby_revision":c.lobby.revision,"objective":c.config.objective});
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
async fn step(app: &ws::App, bots: &BotConfig) -> Result<()> {
    if !bots.slots.is_empty() && bots.slots.iter().all(Option::is_some) {
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
    for (slot, style) in bots.slots.iter().enumerate() {
        let Some(style) = style else { continue };
        let player = slot as u8;
        let pending = {
            let c = app.controller.lock().unwrap();
            if !c.bot_pending(player) {
                None
            } else {
                let tick = intervention(&c, player, *style);
                Some((
                    c.current,
                    tick,
                    c.cached_exact(c.current, tick),
                    c.exact_request(c.current, tick)?,
                    c.sim.clone(),
                ))
            }
        };
        let Some((revision, tick, cached, request, sim)) = pending else {
            continue;
        };
        let state = match cached {
            Some(s) => s,
            None => sim.exact(request, tick).await?,
        };
        let job = {
            let mut c = app.controller.lock().unwrap();
            if c.current != revision || !c.bot_pending(player) {
                continue;
            }
            let view = visible_state(&state, player, &c.config, &c.content);
            let draft = plan(&view, player, *style, &c.config, &c.content, revision);
            let mut request = CommitRequest {
                request_id: format!("bot-{player}-{}", c.round),
                slot_token: c.bot_token(player).ok_or("bot slot lost")?,
                draft,
            };
            c.precheck_commit(player, &request)?;
            let mut commands = draft::resolve_local_references(&request.draft, c.round, player)?;
            if let Err(e) = c.validate_against_state(player, &commands, &state) {
                // An occupied/unavailable site is an ordinary failed plan, never permission to
                // inspect hidden state and replan. Pass this turn through the same contract.
                eprintln!("bot {} plan rejected; passing: {e}", style.name());
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::SimThread;
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
            let draft = plan(&view, 1, style, &c.config, &c.content, 0);
            assert_eq!(draft, plan(&view, 1, style, &c.config, &c.content, 0));
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
            let single = plan(&view, 1, style, &config, &c.content, 0);
            assert_eq!(single.commands.len(), 1);
        }
    }
    #[test]
    fn hidden_enemy_state_cannot_change_a_plan() {
        let (c, state) = setup();
        let before = visible_state(&state, 1, &c.config, &c.content);
        assert!(before.entities.iter().all(|e| e.owner == 1));
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
                plan(&before, 1, style, &c.config, &c.content, 0),
                plan(&after, 1, style, &c.config, &c.content, 0)
            );
        }
        assert_eq!(before.players.len(), 1);
        assert!(before.rng_state.is_empty());
    }
    #[test]
    fn visible_enemies_are_targets_but_their_orders_are_private() {
        let (c, mut state) = setup();
        let home = state.entities.iter().find(|e| e.owner == 1).unwrap().tile;
        let enemy = state.entities.iter_mut().find(|e| e.owner == 0).unwrap();
        enemy.tile = home;
        enemy.action = Order::AttackMove {
            destination: Tile { x: 1, y: 1 },
        };
        let view = visible_state(&state, 1, &c.config, &c.content);
        assert!(view.entities.iter().any(|e| e.owner == 0));
        assert!(
            view.entities
                .iter()
                .filter(|e| e.owner == 0)
                .all(|e| matches!(e.action, Order::Idle {}))
        );
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
            slots: vec![None, Some(DEFAULT_STYLE)],
            playtest_rounds: 20,
        };
        bots.install(&mut c).unwrap();
        c.start_match(&human, c.lobby.revision).unwrap();
        let rx = c.start_job(0, vec![], vec![]).unwrap();
        run_round(&mut c, rx);
        let mut later_actions = 0;
        for round in 1..=8 {
            let tick = intervention(&c, 1, DEFAULT_STYLE);
            let state =
                atemporal_sim::reconstruct(&c.exact_request(c.current, tick).unwrap(), tick)
                    .unwrap();
            let view = visible_state(&state, 1, &c.config, &c.content);
            let bot_draft = plan(&view, 1, DEFAULT_STYLE, &c.config, &c.content, c.current);
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
                distance(*t, home) <= 16
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
        let defense = plan(&view, 1, DEFAULT_STYLE, &c.config, &c.content, c.current);
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
        let recovery = plan(&view, 1, DEFAULT_STYLE, &c.config, &c.content, c.current);
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
        assert!(intervention(&c, 1, DEFAULT_STYLE) <= 200);
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

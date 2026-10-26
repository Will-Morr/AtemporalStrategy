//! Deliberately small, deterministic opponents. Policies receive a visibility-filtered published
//! state and emit normal drafts; the controller validates them exactly like human input.
use crate::{controller::Controller, coordinator::atomic_json, ws};
use atemporal_sim::*;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Rusher,
    Builder,
    Raider,
}
impl Style {
    fn name(self) -> &'static str {
        match self {
            Self::Rusher => "Rusher",
            Self::Builder => "Builder",
            Self::Raider => "Raider",
        }
    }
    fn interval(self) -> Tick {
        match self {
            Self::Rusher => 50,
            Self::Builder => 150,
            Self::Raider => 100,
        }
    }
    fn unit(self) -> &'static str {
        match self {
            Self::Rusher => "grunt",
            Self::Builder => "tank",
            Self::Raider => "grinder",
        }
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
/// enter a policy. The same allied radial vision model is used by the browser.
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
        commands.push(DraftCommand {
            local_id: local_id.clone(),
            command,
            future_orders: FutureOrderPolicy::Keep,
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
        .find(|e| def(e).is_some_and(|t| t.construction.is_some()))
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
                && matches!(e.action, Order::Idle {})
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
            let desired = match style {
                Style::Raider if state.tick < 300 => Tile {
                    x: state.terrain.width - 1 - home.x,
                    y: state.terrain.height / 2,
                },
                _ => Tile {
                    x: state.terrain.width - 1 - home.x,
                    y: state.terrain.height - 1 - home.y,
                },
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
    let has_factory = state
        .entities
        .iter()
        .any(|e| e.owner == player && e.type_key == "factory")
        || state
            .blueprints
            .iter()
            .any(|b| b.owner == player && b.type_key == "factory");
    let constructors: Vec<_> = own
        .iter()
        .filter(|e| def(e).is_some_and(|t| t.construction.is_some()))
        .collect();
    if !has_factory && !constructors.is_empty() && recipe.is_some() {
        let site = (0..state.terrain.height)
            .flat_map(|y| (0..state.terrain.width.saturating_sub(1)).map(move |x| Tile { x, y }))
            .filter(|t| {
                let i = usize::from(t.y) * usize::from(state.terrain.width) + usize::from(t.x);
                distance(*t, home) <= 36
                    && state.terrain.cells[i] == TerrainCell::Floor
                    && state.terrain.cells[i + 1] == TerrainCell::Floor
                    && state.ore[i] == 0.0
                    && !state.entities.iter().any(|e| e.tile == *t)
                    && !state.blueprints.iter().any(|b| b.tile == *t)
            })
            .min_by_key(|t| (distance(*t, home), t.y, t.x));
        if let Some(tile) = site {
            let placement = push(Command::PlaceBlueprints {
                type_key: "factory".into(),
                tiles: vec![tile],
                priority: Priority::High,
                output_directions: Some(vec![CardinalDirection::E]),
            });
            // The following reference is resolved by the ordinary draft machinery.
            push(Command::ConfigureBlueprints {
                blueprint_ids: vec![DraftItemRef::Draft {
                    local_id: placement,
                    item_index: 0,
                }],
                settings: BlueprintSettings {
                    queue: vec![recipe.clone().unwrap()],
                    queue_loop_flags: vec![true],
                    order: Order::AttackMove {
                        destination: target,
                    },
                    priority: Priority::High,
                    loop_enabled: true,
                    silo_plan: None,
                },
            });
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
            order: Order::Construct { area },
        });
    }
    for factory in own.iter().filter(|e| e.type_key == "factory") {
        if let Some(p) = &factory.production {
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
            if !p.loop_enabled {
                push(Command::SetQueueLoop {
                    factories: vec![factory.id.clone()],
                    enabled: true,
                });
            }
            if p.stored_order
                != (Order::AttackMove {
                    destination: target,
                })
            {
                push(Command::SetStoredOrder {
                    factories: vec![factory.id.clone()],
                    order: Order::AttackMove {
                        destination: target,
                    },
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
    tokio::spawn(async move {
        let mut saved = String::new();
        loop {
            if let Err(e) = step(&app, &bots).await {
                eprintln!("bot: {e}");
            }
            {
                let c = app.controller.lock().unwrap();
                let value = serde_json::json!({"match_id":c.match_id,"phase":c.phase,"round":c.round,"slots":c.lobby.slots,"lobby_revision":c.lobby.revision,"objective":c.config.objective});
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
        for style in [Style::Rusher, Style::Builder, Style::Raider] {
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
        for style in [Style::Rusher, Style::Builder, Style::Raider] {
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
    fn bot_only_budget_and_human_host_are_explicit() {
        assert!(
            BotConfig {
                slots: vec![Some(Style::Rusher), None],
                playtest_rounds: 20
            }
            .validate(2)
            .is_err()
        );
        assert!(
            BotConfig {
                slots: vec![None, Some(Style::Builder)],
                playtest_rounds: 20
            }
            .validate(2)
            .is_ok()
        );
        assert!(
            BotConfig {
                slots: vec![Some(Style::Rusher); 2],
                playtest_rounds: 0
            }
            .validate(2)
            .is_err()
        );
    }
}

//! Strategy is expressed entirely in terms of published player observations and normal drafts.
use atemporal_contracts::{
    BlueprintId, BlueprintSettings, CardinalDirection, Command, Content, ControlLimit,
    DraftCommand, DraftItemRef, FutureOrderPolicy, Lifecycle, MatchConfig, Multiplayer, Order,
    Outcome, OutcomeKind, PlayerId, Priority, ProductionEdit, QueueItemId, Rect, Result, Revision,
    RoundScore, SurvivalStatus, Terrain, TerrainCell, Tick, Tile, TimedAdjudication, TurnDraft,
};
use atemporal_sim::player_timeline::{OwnEntity, PlayerFrame, PlayerTimeline};

use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

mod tactical;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    #[serde(alias = "rusher", alias = "builder", alias = "raider")]
    Standard,
    Tactical,
}
/// Offline Tactical tuning knobs. The default preserves the live policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalParams {
    pub opening_grunts: u8,
    pub fresh_entry_ticks: Tick,
    pub rally_distance_band: u8,
}

impl Default for TacticalParams {
    fn default() -> Self {
        Self {
            opening_grunts: 8,
            fresh_entry_ticks: 600,
            rally_distance_band: 8,
        }
    }
}

impl TacticalParams {
    pub fn validate(self) -> Result<()> {
        if self.opening_grunts > 16
            || !(100..=1800).contains(&self.fresh_entry_ticks)
            || !(1..=24).contains(&self.rally_distance_band)
        {
            return Err("Tactical parameters outside supported ranges".into());
        }
        Ok(())
    }
}

/// The coordinator advertises this policy to quick play.
pub const DEFAULT_STYLE: Style = Style::Tactical;
impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard 1.0",
            Self::Tactical => "Tactical 2.0",
        }
    }
    fn interval(self) -> Tick {
        match self {
            Self::Standard => 150,
            Self::Tactical => 100,
        }
    }
    fn unit(self) -> &'static str {
        match self {
            Self::Standard => "grunt",
            Self::Tactical => "tank",
        }
    }
}

/// Public turn markers omit opposing players' command payloads.
#[derive(Clone, Debug, Serialize)]
pub struct TurnMarker {
    pub round: u32,
    pub player: PlayerId,
    pub tick: Option<Tick>,
    pub commands: usize,
    pub duration_ms: u64,
}

impl TurnMarker {
    pub fn for_player(turn: &atemporal_contracts::AcceptedTurn, player: PlayerId) -> Self {
        Self {
            round: turn.round,
            player: turn.player,
            tick: (turn.player == player || !turn.commands.is_empty()).then_some(turn.tick),
            commands: turn.commands.len(),
            duration_ms: turn.duration_ms.get(),
        }
    }
}

/// Public information from the published round picker and scoreboard.
#[derive(Clone, Debug, Serialize)]
pub struct PublishedRound {
    pub revision: Revision,
    pub parent: Option<Revision>,
    pub round: u32,
    pub outcome: Outcome,
    pub score: Option<RoundScore>,
    pub timed: Option<TimedAdjudication>,
}

pub trait History: Send + Sync {
    fn revisions(&self) -> Box<dyn Iterator<Item = Revision> + '_>;
    fn round(&self, revision: Revision) -> Option<&PublishedRound>;
    fn turns(&self, revision: Revision) -> Option<&[TurnMarker]>;
    fn analyze(&self, revision: Revision) -> Result<PlayerTimeline>;
}

pub struct MatchRules {
    pub config: MatchConfig,
    pub content: Content,
}

pub struct PublishedWindow {
    pub revision: Revision,
    pub player: PlayerId,
    pub editable: RangeInclusive<Tick>,
}

pub struct PolicyTimeline {
    timeline: PlayerTimeline,
    history: Box<dyn History>,
    rules: MatchRules,
    window: PublishedWindow,
}

impl PolicyTimeline {
    pub fn new(
        timeline: PlayerTimeline,
        history: Box<dyn History>,
        rules: MatchRules,
        window: PublishedWindow,
    ) -> Result<Self> {
        if window.editable.is_empty()
            || timeline.frame(*window.editable.start()).is_none()
            || timeline.frame(*window.editable.end()).is_none()
        {
            return Err("player timeline does not cover the published editable range".into());
        }
        Ok(Self {
            timeline,
            history,
            rules,
            window,
        })
    }

    pub fn timeline(&self) -> &PlayerTimeline {
        &self.timeline
    }

    pub fn history(&self) -> &dyn History {
        self.history.as_ref()
    }

    pub fn revision(&self) -> Revision {
        self.window.revision
    }

    pub fn player(&self) -> PlayerId {
        self.window.player
    }

    pub fn config(&self) -> &MatchConfig {
        &self.rules.config
    }

    pub fn content(&self) -> &Content {
        &self.rules.content
    }

    pub fn editable_ticks(&self) -> RangeInclusive<Tick> {
        self.window.editable.clone()
    }
}

#[derive(Clone, Copy)]
pub struct FrameRules<'a> {
    pub config: &'a MatchConfig,
    pub content: &'a Content,
    pub terrain: &'a Terrain,
}

fn ore_at(frame: &PlayerFrame, index: usize) -> f64 {
    frame
        .visible_ore
        .binary_search_by_key(&(index as u32), |cell| cell.index)
        .map_or(0., |i| frame.visible_ore[i].remaining)
}

fn occupied(frame: &PlayerFrame, tile: Tile) -> bool {
    frame.own_entities.iter().any(|entity| entity.tile == tile)
        || frame
            .visible_entities
            .iter()
            .any(|entity| entity.tile == tile)
}

fn reserved_factory_outputs(frame: &PlayerFrame, terrain: &Terrain) -> impl Iterator<Item = Tile> {
    use atemporal_sim::placement::output_tile;
    frame
        .own_entities
        .iter()
        .filter_map(|e| {
            e.production
                .as_ref()
                .filter(|p| p.silo.is_none())
                .map(|p| p.output_tile)
        })
        .chain(
            frame
                .visible_entities
                .iter()
                .filter_map(|e| output_tile(terrain, e.tile, e.output_direction?)),
        )
        .chain(
            frame
                .own_blueprints
                .iter()
                .filter_map(|b| output_tile(terrain, b.tile, b.output_direction?)),
        )
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
impl PolicyTimeline {
    fn outcome(&self) -> Option<&Outcome> {
        self.history
            .round(self.revision())
            .map(|round| &round.outcome)
    }

    fn last_turn(&self) -> Option<Tick> {
        self.timeline.turns.iter().map(|turn| turn.tick).max()
    }

    fn loss_tick(&self) -> Option<Tick> {
        let outcome = self.outcome()?;
        if !outcome.eliminated.contains(&self.player()) {
            return None;
        }
        outcome
            .survival_transitions
            .iter()
            .rev()
            .find(|transition| {
                transition.player_id == self.player()
                    && transition.status == SurvivalStatus::Eliminated
            })
            .map(|loss| loss.resolved_tick)
    }

    fn stalemate(&self) -> bool {
        self.outcome()
            .is_some_and(|outcome| outcome.kind == OutcomeKind::Stalemate)
    }

    fn winning(&self) -> bool {
        self.outcome().is_some_and(|outcome| {
            outcome.kind == OutcomeKind::Win
                && outcome.survivors.contains(&self.player())
                && outcome
                    .survivors
                    .iter()
                    .all(|p| allied(self.config(), self.player(), *p))
        })
    }
}

/// Choose an intervention using only the published timeline.
pub fn intervention(input: &PolicyTimeline, style: Style) -> Tick {
    let editable = input.editable_ticks();
    let (editable_from, available_through) = (*editable.start(), *editable.end());
    let wanted = input
        .last_turn()
        .map_or(editable_from, |tick| tick.saturating_add(style.interval()))
        .clamp(editable_from, available_through);
    if let Some(loss) = input.loss_tick() {
        let rescue = loss.saturating_sub(200).max(editable_from);
        if rescue < wanted {
            return rescue;
        }
    }
    let has_mobile_weapons = |frame: &PlayerFrame, minimum| {
        frame
            .own_entities
            .iter()
            .filter(|e| {
                input
                    .content()
                    .types
                    .iter()
                    .any(|d| d.key == e.type_key && d.weapon.is_some() && d.movement.is_some())
            })
            .take(minimum)
            .count()
            >= minimum
    };
    let first_army = |lookahead, minimum| {
        let through = wanted.saturating_add(lookahead).min(available_through);
        (wanted..=through).find(|&tick| {
            input
                .timeline
                .frame(tick)
                .is_some_and(|frame| has_mobile_weapons(frame, minimum))
        })
    };
    if style == Style::Tactical
        && input.stalemate()
        && let Some(tick) = first_army(500, 20)
    {
        return tick;
    }
    first_army(style.interval(), 1).unwrap_or(wanted)
}

fn append(
    commands: &mut Vec<DraftCommand>,
    command: Command<DraftItemRef<BlueprintId>, DraftItemRef<QueueItemId>>,
) -> String {
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
}

/// Scoreboard wins keep counting when both players pass.
pub fn published_plan(input: &PolicyTimeline, style: Style) -> TurnDraft {
    published_plan_with_params(input, style, TacticalParams::default())
}

pub fn published_plan_with_params(
    input: &PolicyTimeline,
    style: Style,
    params: TacticalParams,
) -> TurnDraft {
    let tick = intervention(input, style);
    if input.winning() && style == Style::Tactical {
        return TurnDraft {
            based_on_revision: input.revision(),
            tick,
            commands: vec![],
        };
    }
    let frame = input
        .timeline
        .frame(tick)
        .expect("published timeline covers every editable tick");
    let rules = FrameRules {
        config: input.config(),
        content: input.content(),
        terrain: &input.timeline.terrain,
    };
    if style == Style::Tactical {
        return tactical::plan(input, frame, rules, params);
    }
    plan_frame(frame, input.player(), style, rules, input.revision())
}

pub fn plan_frame(
    state: &PlayerFrame,
    player: PlayerId,
    style: Style,
    rules: FrameRules<'_>,
    revision: Revision,
) -> TurnDraft {
    plan_frame_with_rally(
        state,
        player,
        style,
        rules,
        revision,
        None,
        TacticalParams::default(),
    )
}

fn plan_frame_with_rally(
    state: &PlayerFrame,
    player: PlayerId,
    style: Style,
    rules: FrameRules<'_>,
    revision: Revision,
    rally: Option<Tile>,
    params: TacticalParams,
) -> TurnDraft {
    let FrameRules {
        config,
        content,
        terrain,
    } = rules;
    let mut commands = vec![];
    let mut push = |command| append(&mut commands, command);
    let own: Vec<_> = state
        .own_entities
        .iter()
        .filter(|e| e.owner == player && e.lifecycle == Lifecycle::Complete)
        .collect();
    let def = |e: &OwnEntity| content.types.iter().find(|t| t.key == e.type_key);
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
            x: terrain.width - 1,
            y: terrain.height - 1,
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
    let target = rally
        .or_else(|| {
            state
                .visible_entities
                .iter()
                .filter(|e| !allied(config, player, e.owner))
                .min_by_key(|e| distance(e.tile, home))
                .map(|e| e.tile)
        })
        .unwrap_or_else(|| {
            let desired = Tile {
                x: terrain.width - 1 - home.x,
                y: terrain.height - 1 - home.y,
            };
            (0..terrain.height)
                .flat_map(|y| (0..terrain.width).map(move |x| Tile { x, y }))
                .filter(|t| {
                    terrain.cells[usize::from(t.y) * usize::from(terrain.width) + usize::from(t.x)]
                        == TerrainCell::Floor
                })
                .min_by_key(|t| distance(*t, desired))
                .unwrap_or(home)
        });
    let recipes = content
        .types
        .iter()
        .find(|d| d.key == "factory")
        .and_then(|d| d.production.as_ref())
        .map_or(&[][..], |p| p.recipes.as_slice());
    let recipe = recipes
        .iter()
        .find(|key| key.as_str() == style.unit())
        .or_else(|| recipes.first())
        .cloned();
    let can_train = |key: &str| {
        if style == Style::Standard {
            content.types.iter().any(|d| d.key == key)
        } else {
            recipes.iter().any(|r| r == key)
        }
    };
    let count = |key: &str| {
        state
            .own_entities
            .iter()
            .filter(|e| e.owner == player && e.type_key == key)
            .count()
            + state
                .own_blueprints
                .iter()
                .filter(|b| b.owner == player && b.type_key == key && b.site_id.is_none())
                .count()
    };
    let factory_count = count("factory");
    let bank = state.player.bank;
    let threatened = state.visible_entities.iter().any(|e| {
        !allied(config, player, e.owner)
            && content
                .types
                .iter()
                .any(|t| t.key == e.type_key && t.weapon.is_some())
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
        let west = style == Style::Tactical && target.x < home.x;
        let direction = if west {
            CardinalDirection::W
        } else {
            CardinalDirection::E
        };
        let outputs: std::collections::BTreeSet<_> =
            reserved_factory_outputs(state, terrain).collect();
        let site = (0..terrain.height)
            .flat_map(|y| (0..terrain.width).map(move |x| Tile { x, y }))
            .filter(|t| {
                let i = usize::from(t.y) * usize::from(terrain.width) + usize::from(t.x);
                distance(*t, home) <= 64
                    && terrain.cells[i] == TerrainCell::Floor
                    && ore_at(state, i) == 0.0
                    && !occupied(state, *t)
                    && !state.own_blueprints.iter().any(|b| b.tile == *t)
                    && !outputs.contains(t)
                    && (structure != "factory"
                        || tactical::adjacent(*t, direction, terrain).is_some_and(|out| {
                            atemporal_sim::placement::is_floor(terrain, out)
                                && !occupied(state, out)
                                && !state.own_blueprints.iter().any(|b| b.tile == out)
                        }))
            })
            .min_by_key(|t| (distance(*t, home), t.y, t.x));
        if let Some(tile) = site {
            let placement = push(Command::PlaceBlueprints {
                type_key: structure.into(),
                tiles: vec![tile],
                priority: Priority::High,
                output_directions: (structure == "factory").then(|| vec![direction]),
            });
            // The following reference is resolved by the ordinary draft machinery.
            if structure == "factory" {
                let mut queue = vec![];
                if factory_count == 0 && can_train("miner") {
                    queue.extend(std::iter::repeat_n(
                        "miner".into(),
                        if style == Style::Tactical { 6 } else { 2 },
                    ));
                }
                let flags = if style == Style::Tactical && factory_count == 0 {
                    let loop_start = queue.len();
                    for key in ["miner", "miner", "constructor", "grunt"] {
                        if can_train(key) {
                            queue.push(key.into());
                        }
                    }
                    if queue.len() == loop_start {
                        queue.push(recipe.clone().unwrap());
                    }
                    (0..queue.len()).map(|i| i >= loop_start).collect()
                } else {
                    queue.push(recipe.clone().unwrap());
                    (0..queue.len()).map(|i| i + 1 == queue.len()).collect()
                };
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
        if can_train(key) {
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
            let newborn_order = if style == Style::Tactical
                && matches!(p.stored_order, Order::Construct { .. })
                && p.active_item
                    .iter()
                    .map(|i| &i.type_key)
                    .chain(p.pending_items.iter().map(|i| &i.type_key))
                    .any(|key| key == "constructor")
            {
                p.stored_order.clone()
            } else if feeding_economy {
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
                    || e.activity == atemporal_contracts::Activity::Idle)
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
    if style == Style::Tactical {
        tactical::develop(state, player, rules, home, target, params, &mut commands);
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

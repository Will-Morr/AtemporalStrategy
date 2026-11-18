//! Repeatable, mirrored base-rush scenarios with the production simulation.
#[path = "../tests/common/mod.rs"]
mod common;

use atemporal_sim::*;
use common::{World, attack_move, tile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const WIDTH: u16 = 31;
const HEIGHT: u16 = 15;
const BASE_X: u16 = 2;
const BASE_Y: u16 = HEIGHT / 2;

const USAGE: &str = "balance_sweep <army|factory> [--budget MATTER] [--income MATTER_PER_TICK]
              [--ticks N] [--arena open|choke|snake|straight] [--content PATH]
              [--armed-only]
              [--a UNIT[+UNIT...] --b UNIT[+UNIT...]] [--output PATH]
Without --a and --b, run every pure and two-unit roster against each other, including self-play.
Combat rosters exclude miners and constructors, including in explicit matchups.
Without --output, write JSONL to stdout. A matching output file resumes completed trials.";

type Roster = Vec<TypeKey>;

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Army,
    Factory,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Arena {
    Open,
    Choke,
    Snake,
    Straight,
}

const LANE_WIDTH: usize = 3;
const LANE_HALF: usize = LANE_WIDTH / 2;
const ROOM_X_END: u16 = 5;
const ROOM_Y_START: u16 = 4;
const ROOM_Y_END: u16 = 10;

impl Arena {
    fn has_staging_rooms(self) -> bool {
        matches!(self, Self::Snake | Self::Straight)
    }

    fn seat_tile(self, width: u16, x: u16, y: u16, seat: PlayerId) -> Tile {
        if seat == 0 {
            tile(x, y)
        } else {
            let y = if self.has_staging_rooms() {
                HEIGHT - 1 - y
            } else {
                y
            };
            tile(width - 1 - x, y)
        }
    }

    fn rows(self) -> Vec<String> {
        let width = if matches!(self, Self::Straight) {
            let distance = cardinal_distance(&Self::Snake.rows()).expect("connected snake arena");
            distance + 2 * usize::from(BASE_X) + 1
        } else {
            usize::from(WIDTH)
        };
        let height = usize::from(HEIGHT);
        if !self.has_staging_rooms() {
            let mut rows = vec![".".repeat(width); height];
            if matches!(self, Self::Choke) {
                for (y, row) in rows.iter_mut().enumerate() {
                    if !(6..=8).contains(&y) {
                        row.replace_range(14..17, "###");
                    }
                }
            }
            return rows;
        }

        let mut rows = vec![vec![b'#'; width]; height];
        let room_x = usize::from(ROOM_X_END);
        let mid_y = usize::from(BASE_Y);
        let mut carve = |x0, x1, y0, y1| {
            for row in &mut rows[y0..=y1] {
                row[x0..=x1].fill(b'.');
            }
        };
        let room_top = usize::from(ROOM_Y_START);
        let room_bottom = usize::from(ROOM_Y_END);
        carve(0, room_x, room_top, room_bottom);
        carve(width - 1 - room_x, width - 1, room_top, room_bottom);

        match self {
            Self::Snake => {
                // Three full traverses through the center, with an alternating hairpin
                // at either end of the middle traverse. Walls separate the staging bays
                // from the middle.
                let top_y = 2;
                let bottom_y = height - 1 - top_y;
                let left_turn = 8;
                let right_turn = width - 1 - left_turn;
                let entrance = room_x - LANE_WIDTH + 1;
                carve(
                    entrance,
                    right_turn + LANE_HALF,
                    top_y - LANE_HALF,
                    top_y + LANE_HALF,
                );
                carve(
                    left_turn - LANE_HALF,
                    right_turn + LANE_HALF,
                    mid_y - LANE_HALF,
                    mid_y + LANE_HALF,
                );
                carve(
                    left_turn - LANE_HALF,
                    width - 1 - entrance,
                    bottom_y - LANE_HALF,
                    bottom_y + LANE_HALF,
                );
                carve(right_turn - LANE_HALF, right_turn + LANE_HALF, top_y, mid_y);
                carve(
                    left_turn - LANE_HALF,
                    left_turn + LANE_HALF,
                    mid_y,
                    bottom_y,
                );
            }
            Self::Straight => carve(
                room_x,
                width - 1 - room_x,
                mid_y - LANE_HALF,
                mid_y + LANE_HALF,
            ),
            _ => unreachable!("arena without staging rooms handled above"),
        }

        rows.into_iter()
            .map(|row| String::from_utf8(row).expect("ASCII arena"))
            .collect()
    }
}

fn cardinal_distance(rows: &[String]) -> Option<usize> {
    let width = rows[0].len();
    let height = rows.len();
    let start = (usize::from(BASE_X), usize::from(BASE_Y));
    let goal = (width - 1 - start.0, start.1);
    let is_floor = |x, y| {
        rows.get(y)
            .and_then(|row: &String| row.as_bytes().get(x))
            .is_some_and(|&cell| cell == b'.')
    };
    if !is_floor(start.0, start.1) || !is_floor(goal.0, goal.1) {
        return None;
    }
    let mut visited = vec![vec![false; width]; height];
    visited[start.1][start.0] = true;
    let mut queue = VecDeque::from([(start, 0)]);
    while let Some(((x, y), steps)) = queue.pop_front() {
        if (x, y) == goal {
            return Some(steps);
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (Some(nx), Some(ny)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
                continue;
            };
            if is_floor(nx, ny) && !visited[ny][nx] {
                visited[ny][nx] = true;
                queue.push_back(((nx, ny), steps + 1));
            }
        }
    }
    None
}

struct Options {
    mode: Mode,
    arena: Arena,
    ticks: Tick,
    income_per_tick: f64,
    army_budget: f64,
    armed_only: bool,
    content: Option<PathBuf>,
    output: Option<PathBuf>,
    a: Option<Roster>,
    b: Option<Roster>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self> {
        let mut options = Self {
            mode: match args.next().as_deref() {
                Some("army") => Mode::Army,
                Some("factory") => Mode::Factory,
                _ => return Err("choose army or factory mode".into()),
            },
            arena: Arena::Open,
            ticks: 900,
            income_per_tick: 3.0,
            army_budget: 600.0,
            armed_only: false,
            content: None,
            output: None,
            a: None,
            b: None,
        };
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--armed-only" => {
                    options.armed_only = true;
                    continue;
                }
                "--include-miners" => {
                    return Err("combat rosters exclude miners and constructors".into());
                }
                _ => {}
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{flag} requires a value"))?;
            match flag.as_str() {
                "--arena" => {
                    options.arena = match value.as_str() {
                        "open" => Arena::Open,
                        "choke" => Arena::Choke,
                        "snake" => Arena::Snake,
                        "straight" => Arena::Straight,
                        _ => return Err("--arena must be open, choke, snake or straight".into()),
                    };
                }
                "--ticks" => {
                    options.ticks = value.parse().map_err(|_| "invalid --ticks")?;
                    if options.ticks == 0 || options.ticks > Tick::MAX / 2 {
                        return Err("--ticks must be positive and below 2^31".into());
                    }
                }
                "--income" => options.income_per_tick = positive(&value, "--income")?,
                "--budget" => options.army_budget = positive(&value, "--budget")?,
                "--content" => options.content = Some(value.into()),
                "--output" => options.output = Some(value.into()),
                "--a" => options.a = Some(parse_roster(&value)?),
                "--b" => options.b = Some(parse_roster(&value)?),
                _ => return Err(format!("unknown option {flag}")),
            }
        }
        if options.a.is_some() != options.b.is_some() {
            return Err("--a and --b must be supplied together".into());
        }
        if !(options.income_per_tick * f64::from(options.ticks)).is_finite() {
            return Err("total factory income must be finite".into());
        }
        Ok(options)
    }
}

fn positive(value: &str, flag: &str) -> Result<f64> {
    let amount: f64 = value.parse().map_err(|_| format!("invalid {flag}"))?;
    if amount.is_finite() && amount > 0.0 {
        Ok(amount)
    } else {
        Err(format!("{flag} must be finite and positive"))
    }
}

fn parse_roster(value: &str) -> Result<Roster> {
    let roster: Roster = value.split('+').map(str::to_owned).collect();
    if roster.iter().any(String::is_empty)
        || roster.iter().collect::<BTreeSet<_>>().len() != roster.len()
    {
        return Err("rosters must contain distinct unit keys separated by '+'".into());
    }
    Ok(roster)
}

fn definition<'a>(content: &'a Content, key: &str) -> &'a TypeDefinition {
    content
        .types
        .iter()
        .find(|def| def.key == key)
        .expect("validated scenario type")
}

fn factory_roster(content: &Content, armed_only: bool) -> Result<Roster> {
    let factory = content
        .types
        .iter()
        .find(|def| def.key == "factory" && def.silo.is_none())
        .ok_or("content lacks a factory")?;
    let production = factory
        .production
        .as_ref()
        .ok_or("factory lacks production")?;
    let mut keys = Vec::new();
    for key in &production.recipes {
        if matches!(key.as_str(), "miner" | "constructor") {
            continue;
        }
        let def = definition(content, key);
        if def.kind == TypeKind::Unit
            && def.movement.is_some()
            && (!armed_only || def.weapon.is_some())
        {
            if def.matter_cost <= 0.0 {
                return Err(format!("{key} must have a positive matter cost"));
            }
            keys.push(key.clone());
        }
    }
    if keys.is_empty() {
        return Err("no eligible factory-buildable mobile units".into());
    }
    Ok(keys)
}

#[derive(Clone, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct TrialKey {
    a: Roster,
    b: Roster,
    a_seat: PlayerId,
}

fn trials(options: &Options, units: &Roster) -> Result<(Vec<Roster>, Vec<TrialKey>)> {
    let mut rosters = Vec::new();
    let mut pairs = Vec::new();
    if let (Some(a), Some(b)) = (&options.a, &options.b) {
        for key in a.iter().chain(b) {
            if !units.contains(key) {
                return Err(format!("unit {key} is outside the eligible factory roster"));
            }
        }
        rosters.push(a.clone());
        if a != b {
            rosters.push(b.clone());
        }
        pairs.push((a.clone(), b.clone()));
    } else {
        rosters.extend(units.iter().map(|key| vec![key.clone()]));
        for (i, first) in units.iter().enumerate() {
            for second in &units[i + 1..] {
                rosters.push(vec![first.clone(), second.clone()]);
            }
        }
        for (i, a) in rosters.iter().enumerate() {
            for b in &rosters[i..] {
                pairs.push((a.clone(), b.clone()));
            }
        }
    }
    let trials = pairs
        .into_iter()
        .flat_map(|(a, b)| {
            // In self-play a single simulation reports both seats of the identical roster.
            let seats = if a == b { 0..1 } else { 0..2 };
            seats.map(move |a_seat| TrialKey {
                a: a.clone(),
                b: b.clone(),
                a_seat,
            })
        })
        .collect();
    Ok((rosters, trials))
}

fn arena(options: &Options, mut content: Content, rows: &[String]) -> World {
    if options.mode == Mode::Factory {
        // A funded order can finish during the current tick. The real output tile, push
        // rules, birth limit and rally order still apply.
        let rate = content
            .types
            .iter()
            .map(|d| d.matter_cost)
            .fold(0.0, f64::max);
        content
            .types
            .iter_mut()
            .find(|def| def.key == "factory")
            .unwrap()
            .production
            .as_mut()
            .unwrap()
            .rate = rate;
    }
    let mut world = World::with_content(
        &rows.iter().map(String::as_str).collect::<Vec<_>>(),
        content,
    );
    world.config.max_tick = options.ticks;
    world.config.snapshot_interval = options.ticks;
    world.config.checkpoint_interval = options.ticks;
    world
}

/// Each type earns an equal share. The next purchase is the unit whose cumulative
/// per-type cost is lowest, with roster order breaking simultaneous deadlines.
struct Queue<'a> {
    items: Vec<(&'a str, f64, u32)>,
}

impl<'a> Queue<'a> {
    fn new(content: &Content, roster: &'a Roster) -> Self {
        Self {
            items: roster
                .iter()
                .map(|key| (key.as_str(), definition(content, key).matter_cost, 0))
                .collect(),
        }
    }

    fn buy(&mut self, total_matter: f64) -> Option<&'a str> {
        let share = total_matter / self.items.len() as f64;
        let (next, cost) = self
            .items
            .iter()
            .enumerate()
            .map(|(index, (_, price, purchased))| (index, price * f64::from(purchased + 1)))
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap();
        if cost > share {
            return None;
        }
        let (key, _, purchased) = &mut self.items[next];
        *purchased += 1;
        Some(key)
    }
}

fn base(world: &mut World, arena: Arena, seat: PlayerId) -> EntityId {
    let position = arena.seat_tile(world.state.terrain.width, BASE_X, BASE_Y, seat);
    let id = world.spawn(seat, "factory", position.x, position.y);
    let factory = world.entity_mut(&id);
    factory.priority = Priority::Off;
    id
}

fn deploy_army(
    world: &mut World,
    arena: Arena,
    seat: PlayerId,
    roster: &Roster,
    budget: f64,
) -> Result<f64> {
    let mut queue = Queue::new(&world.content, roster);
    let columns = [5, 4, 3, 2, 1, 0, 6, 7, 8, 9, 10, 11, 12, 13];
    let rows = [7, 6, 8, 5, 9, 4, 10, 3, 11, 2, 12, 1, 13, 0, 14];
    let mut positions = columns
        .into_iter()
        .flat_map(|x| rows.into_iter().map(move |y| (x, y)))
        .filter(|&(x, y)| {
            !arena.has_staging_rooms()
                || (x <= ROOM_X_END && (ROOM_Y_START..=ROOM_Y_END).contains(&y))
        })
        .filter(|&position| position != (BASE_X, BASE_Y))
        .map(|(x, y)| arena.seat_tile(world.state.terrain.width, x, y, seat))
        .filter(|&position| is_floor(&world.state.terrain, position));
    let mut deployments = Vec::new();
    let mut invested = 0.0;
    while let Some(key) = queue.buy(budget) {
        let position = positions
            .next()
            .ok_or("army budget exceeds arena deployment capacity")?;
        deployments.push((key, position));
        invested += world.def(key).matter_cost;
    }
    let target = arena.seat_tile(world.state.terrain.width, BASE_X, BASE_Y, 1 - seat);
    let rally = attack_move(target.x, target.y);
    for (key, position) in deployments {
        world.spawn_with(seat, key, position.x, position.y, rally.clone());
    }
    world.bank(seat, (budget - invested).max(0.0));
    Ok(invested)
}

fn is_floor(terrain: &Terrain, position: Tile) -> bool {
    let index = usize::from(position.y) * usize::from(terrain.width) + usize::from(position.x);
    terrain.cells[index] == TerrainCell::Floor
}

fn configure_factory(world: &mut World, arena: Arena, seat: PlayerId, id: &EntityId) {
    let width = world.state.terrain.width;
    let target = arena.seat_tile(width, BASE_X, BASE_Y, 1 - seat);
    let output = arena.seat_tile(width, BASE_X + 1, BASE_Y, seat);
    let entity = world.entity_mut(id);
    entity.priority = Priority::Medium;
    let factory = entity.production.as_mut().unwrap();
    factory.stored_order = attack_move(target.x, target.y);
    factory.output_tile = output;
    factory.output_direction = if seat == 0 {
        CardinalDirection::E
    } else {
        CardinalDirection::W
    };
}

fn schedule(sim: &mut Sim, id: &EntityId, queue: &mut Queue<'_>, income: f64) -> Result<()> {
    let tick = sim.state.tick;
    let entity = sim
        .state
        .entities
        .iter_mut()
        .find(|entity| &entity.id == id)
        .unwrap();
    sim.state.players[usize::from(entity.owner)].bank += income;
    if let Some(key) = queue.buy(f64::from(tick + 1) * income) {
        let factory = entity.production.as_mut().unwrap();
        factory.pending_items.push(QueueItem {
            item_id: identity::queue_item(&identity::command_id(tick + 1, entity.owner, 0), 0, 0)?,
            type_key: key.into(),
            loop_enabled: false,
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct Survivor {
    type_key: TypeKey,
    count: usize,
    matter: f64,
    health_weighted_matter: f64,
}

#[derive(Serialize)]
struct SeatResult {
    base_hp: f64,
    bank: f64,
    unit_spend: f64,
    lost_invested_matter: f64,
    surviving_unit_count: usize,
    surviving_unit_matter: f64,
    health_weighted_unit_matter: f64,
    survivors: Vec<Survivor>,
}

fn summarize(
    world: &World,
    state: &WorldState,
    seat: PlayerId,
    base_id: &EntityId,
    initial_unit_matter: f64,
) -> SeatResult {
    let player = &state.players[usize::from(seat)];
    let mut summary = SeatResult {
        base_hp: 0.0,
        bank: player.bank,
        unit_spend: initial_unit_matter + player.counters.unit_spend,
        lost_invested_matter: player.counters.lost_invested_matter,
        surviving_unit_count: 0,
        surviving_unit_matter: 0.0,
        health_weighted_unit_matter: 0.0,
        survivors: Vec::new(),
    };
    let mut by_type = BTreeMap::new();
    for entity in state.entities.iter().filter(|e| e.owner == seat) {
        if &entity.id == base_id {
            summary.base_hp = entity.hp;
            continue;
        }
        let def = world.def(&entity.type_key);
        let matter = def.matter_cost;
        let weighted = matter * (entity.hp / def.max_hp).clamp(0.0, 1.0);
        let survivor = by_type.entry(&entity.type_key).or_insert_with(|| Survivor {
            type_key: entity.type_key.clone(),
            count: 0,
            matter: 0.0,
            health_weighted_matter: 0.0,
        });
        survivor.count += 1;
        survivor.matter += matter;
        survivor.health_weighted_matter += weighted;
        summary.surviving_unit_count += 1;
        summary.surviving_unit_matter += matter;
        summary.health_weighted_unit_matter += weighted;
    }
    summary.survivors = by_type.into_values().collect();
    summary
}

#[derive(Serialize)]
struct Trial {
    kind: &'static str,
    #[serde(flatten)]
    key: TrialKey,
    winner: Option<&'static str>,
    stop: &'static str,
    ticks: Tick,
    final_hash: String,
    a_result: SeatResult,
    b_result: SeatResult,
}

fn trial(options: &Options, template: &World, key: TrialKey) -> Result<Trial> {
    let mut world = World::with_content(&["."], template.content.clone());
    world.config = template.config.clone();
    world.state = template.state.clone();
    let seats = [key.a_seat, 1 - key.a_seat];
    let bases = seats.map(|seat| base(&mut world, options.arena, seat));
    let rosters = [&key.a, &key.b];
    let mut queues = rosters.map(|roster| Queue::new(&template.content, roster));
    let mut initial_unit_matter = [0.0; 2];
    for i in 0..2 {
        match options.mode {
            Mode::Army => {
                initial_unit_matter[i] = deploy_army(
                    &mut world,
                    options.arena,
                    seats[i],
                    rosters[i],
                    options.army_budget,
                )?;
            }
            Mode::Factory => configure_factory(&mut world, options.arena, seats[i], &bases[i]),
        }
    }
    let mut sim = Sim::new(&world.request())?;
    let mut base_alive = [true; 2];
    while sim.state.tick < options.ticks {
        if options.mode == Mode::Factory {
            for i in 0..2 {
                schedule(&mut sim, &bases[i], &mut queues[i], options.income_per_tick)?;
            }
        }
        sim.step(&mut |_| {})?;
        base_alive = bases
            .each_ref()
            .map(|id| sim.state.entities.iter().any(|e| &e.id == id));
        if !base_alive.iter().all(|&alive| alive) {
            break;
        }
    }
    Ok(Trial {
        kind: "trial",
        ticks: sim.state.tick,
        final_hash: sim.hash_state()?,
        winner: match base_alive {
            [true, false] => Some("a"),
            [false, true] => Some("b"),
            _ => None,
        },
        stop: if base_alive == [true, true] {
            "horizon"
        } else {
            "base_destroyed"
        },
        a_result: summarize(
            &world,
            &sim.state,
            seats[0],
            &bases[0],
            initial_unit_matter[0],
        ),
        b_result: summarize(
            &world,
            &sim.state,
            seats[1],
            &bases[1],
            initial_unit_matter[1],
        ),
        key,
    })
}

#[derive(Serialize)]
struct Metadata<'a> {
    kind: &'static str,
    schema_version: u32,
    sim_rules_version: u32,
    mode: Mode,
    arena: Arena,
    ticks: Tick,
    income_per_tick: f64,
    army_budget: f64,
    rosters: &'a [Roster],
    expected_trials: usize,
    config: &'a MatchConfig,
    content: &'a Content,
    effective_content_hash: String,
    map_width: u16,
    map_height: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    map_rows: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_length_tiles: Option<usize>,
}

#[derive(Deserialize)]
struct PriorTrial {
    kind: String,
    #[serde(flatten)]
    key: TrialKey,
    final_hash: String,
    a_result: PriorSeat,
    b_result: PriorSeat,
}

#[derive(Deserialize)]
struct PriorSeat {
    survivors: Vec<PriorSurvivor>,
}

impl PriorSeat {
    fn matches(&self, roster: &Roster) -> bool {
        self.survivors
            .iter()
            .all(|survivor| roster.contains(&survivor.type_key))
    }
}

#[derive(Deserialize)]
struct PriorSurvivor {
    type_key: TypeKey,
}

fn open_results(
    path: &Path,
    header: &str,
    keys: &[TrialKey],
) -> std::result::Result<(File, BTreeSet<TrialKey>), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    file.try_lock()?;
    let mut reader = BufReader::new(&file);
    let mut line = String::new();
    let mut checked_bytes = 0;
    let mut completed = BTreeSet::new();
    let expected: BTreeSet<_> = keys.iter().collect();
    while reader.read_line(&mut line)? > 0 {
        if !line.ends_with('\n') {
            // An interrupted append may leave a partial record. Keep the complete prefix.
            file.set_len(checked_bytes)?;
            break;
        }
        if checked_bytes == 0 {
            if line.trim_end_matches('\n') != header {
                return Err(format!("{} has different sweep metadata", path.display()).into());
            }
        } else {
            let prior: PriorTrial = serde_json::from_str(&line)?;
            if prior.kind != "trial"
                || prior.final_hash.len() != 64
                || !expected.contains(&prior.key)
                || !prior.a_result.matches(&prior.key.a)
                || !prior.b_result.matches(&prior.key.b)
                || !completed.insert(prior.key)
            {
                return Err(format!("{} has an invalid or duplicate trial", path.display()).into());
            }
        }
        checked_bytes += line.len() as u64;
        line.clear();
    }
    if checked_bytes == 0 {
        writeln!(file, "{header}")?;
        file.flush()?;
    }
    Ok((file, completed))
}

fn execute(options: Options) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let content = match &options.content {
        Some(path) => load_content(&std::fs::read_to_string(path)?)?,
        None => load_content(include_str!("../../../config/content.yaml"))?,
    };
    let units = factory_roster(&content, options.armed_only)?;
    let (rosters, keys) = trials(&options, &units)?;
    let rows = options.arena.rows();
    let template = arena(&options, content, &rows);
    let header = serde_json::to_string(&Metadata {
        kind: "metadata",
        schema_version: 1,
        sim_rules_version: RULES_VERSION,
        mode: options.mode,
        arena: options.arena,
        ticks: options.ticks,
        income_per_tick: options.income_per_tick,
        army_budget: options.army_budget,
        rosters: &rosters,
        expected_trials: keys.len(),
        config: &template.config,
        content: &template.content,
        effective_content_hash: identity::canonical_hash(&template.content)?,
        map_width: template.state.terrain.width,
        map_height: template.state.terrain.height,
        map_rows: options.arena.has_staging_rooms().then_some(&rows),
        route_length_tiles: options
            .arena
            .has_staging_rooms()
            .then(|| cardinal_distance(&rows).expect("connected corridor arena")),
    })?;
    let (mut output, completed): (Box<dyn Write>, _) = if let Some(path) = &options.output {
        let (file, completed) = open_results(path, &header, &keys)?;
        eprintln!(
            "{} of {} trials already complete",
            completed.len(),
            keys.len()
        );
        (Box::new(file), completed)
    } else {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{header}")?;
        (Box::new(stdout), BTreeSet::new())
    };
    let total = keys.len();
    let mut count = completed.len();
    for key in keys {
        if completed.contains(&key) {
            continue;
        }
        let result = trial(&options, &template, key)?;
        let mut line = serde_json::to_vec(&result)?;
        line.push(b'\n');
        output.write_all(&line)?;
        output.flush()?;
        count += 1;
        if options.output.is_some() && (count % 50 == 0 || count == total) {
            eprintln!("{count} of {total} trials complete");
        }
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    if matches!(args.peek().map(String::as_str), Some("-h" | "--help")) {
        println!("{USAGE}");
        return;
    }
    let result = Options::parse(args).map_err(Into::into).and_then(execute);
    if let Err(error) = result {
        eprintln!("{error}\n\n{USAGE}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Content {
        load_content(include_str!("../../../config/content.yaml")).unwrap()
    }

    #[test]
    fn default_pairings_only_use_combat_rosters() {
        let units = factory_roster(&content(), false).unwrap();
        assert_eq!(units, ["artillery", "grinder", "grunt", "scout", "tank"]);
        for mode in ["army", "factory"] {
            let options = Options::parse([mode].map(str::to_owned).into_iter()).unwrap();
            let (rosters, keys) = trials(&options, &units).unwrap();
            assert_eq!(rosters.len(), 15);
            assert_eq!(keys.len(), 225);
            assert!(rosters.iter().flatten().all(|key| units.contains(key)));
            assert!(
                keys.iter()
                    .all(|key| rosters.contains(&key.a) && rosters.contains(&key.b))
            );
        }
    }

    #[test]
    fn rejects_utility_units_in_explicit_rosters() {
        let units = factory_roster(&content(), false).unwrap();
        for mode in ["army", "factory"] {
            assert!(
                Options::parse([mode, "--include-miners"].map(str::to_owned).into_iter()).is_err()
            );
            for roster in ["miner", "constructor", "grunt+miner", "constructor+tank"] {
                for (a, b) in [(roster, "tank"), ("tank", roster)] {
                    let options =
                        Options::parse([mode, "--a", a, "--b", b].map(str::to_owned).into_iter())
                            .unwrap();
                    assert!(trials(&options, &units).is_err());
                }
            }
        }
    }

    #[test]
    fn snake_requires_all_three_narrow_traverses() {
        let rows = Arena::Snake.rows();
        assert_eq!(cardinal_distance(&rows), Some(66));
        assert_eq!(cardinal_distance(&Arena::Straight.rows()), Some(66));

        let center_x = usize::from(WIDTH / 2);
        let floor_at_center: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.as_bytes()[center_x] == b'.')
            .map(|(y, _)| y)
            .collect();
        assert_eq!(floor_at_center, [1, 2, 3, 6, 7, 8, 11, 12, 13]);
        for traverses in floor_at_center.chunks_exact(LANE_WIDTH) {
            let mut cut = rows.clone();
            for &y in traverses {
                cut[y].replace_range(center_x..=center_x, "#");
            }
            assert_eq!(
                cardinal_distance(&cut),
                None,
                "bypass around traverse {traverses:?}"
            );
        }

        let straight = Arena::Straight.rows();
        let straight_x = straight[0].len() / 2;
        assert_eq!(
            straight
                .iter()
                .filter(|row| row.as_bytes()[straight_x] == b'.')
                .count(),
            LANE_WIDTH
        );
    }

    #[test]
    fn corridor_staging_and_factory_outputs_are_floor_and_rotationally_symmetric() {
        let mut options =
            Options::parse(["factory", "--ticks", "5"].map(str::to_owned).into_iter()).unwrap();
        let roster = vec!["grunt".to_owned()];
        let mut west_deployment = Vec::new();
        for layout in [Arena::Snake, Arena::Straight] {
            options.arena = layout;
            let rows = layout.rows();
            for (row, rotated) in rows.iter().zip(rows.iter().rev()) {
                assert!(row.bytes().eq(rotated.bytes().rev()));
            }
            let template = arena(&options, content(), &rows);
            let result = trial(
                &options,
                &template,
                TrialKey {
                    a: roster.clone(),
                    b: roster.clone(),
                    a_seat: 0,
                },
            )
            .unwrap();
            assert_eq!(result.a_result.surviving_unit_count, 1);
            assert_eq!(result.b_result.surviving_unit_count, 1);

            let mut world = arena(&options, content(), &rows);
            for seat in 0..2 {
                base(&mut world, layout, seat);
                assert_eq!(
                    deploy_army(&mut world, layout, seat, &roster, 600.0).unwrap(),
                    600.0
                );
            }
            assert_eq!(world.state.entities.len(), 82);
            let (west, east) = world.state.entities.split_at(41);
            for (left, right) in west.iter().zip(east) {
                assert!(is_floor(&world.state.terrain, left.tile));
                assert!(is_floor(&world.state.terrain, right.tile));
                assert!(left.tile.x <= ROOM_X_END);
                assert!((ROOM_Y_START..=ROOM_Y_END).contains(&left.tile.y));
                assert_eq!(
                    right.tile,
                    layout.seat_tile(world.state.terrain.width, left.tile.x, left.tile.y, 1)
                );
            }
            let spawn_tiles: Vec<_> = west.iter().map(|entity| entity.tile).collect();
            if matches!(layout, Arena::Snake) {
                west_deployment = spawn_tiles;
            } else {
                assert_eq!(west_deployment, spawn_tiles);
            }
        }
    }

    #[test]
    fn factory_mix_budgets_each_type_independently() {
        let options = Options::parse(
            [
                "factory",
                "--ticks",
                "29",
                "--a",
                "grunt+scout",
                "--b",
                "tank",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .unwrap();
        let content = content();
        let units = factory_roster(&content, false).unwrap();
        let (_, keys) = trials(&options, &units).unwrap();
        assert_eq!(keys.len(), 2);
        let world = arena(&options, content, &options.arena.rows());
        for key in keys {
            let result = trial(&options, &world, key).unwrap();
            assert_eq!(result.stop, "horizon");
            assert_eq!(result.a_result.unit_spend, 70.0);
            assert_eq!(result.a_result.bank, 17.0);
            assert_eq!(result.a_result.health_weighted_unit_matter, 70.0);
            assert_eq!(result.a_result.survivors[0].type_key, "grunt");
            assert_eq!(result.a_result.survivors[0].count, 2);
            assert_eq!(result.a_result.survivors[1].type_key, "scout");
            assert_eq!(result.a_result.survivors[1].count, 2);
            assert_eq!(result.b_result.unit_spend, 0.0);
            assert_eq!(result.b_result.bank, 87.0);
        }
    }

    #[test]
    fn army_reports_deployment_spend() {
        let options = Options::parse(
            [
                "army", "--ticks", "1", "--budget", "613", "--a", "tank", "--b", "grunt",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .unwrap();
        let content = content();
        let units = factory_roster(&content, false).unwrap();
        let (_, keys) = trials(&options, &units).unwrap();
        let world = arena(&options, content, &options.arena.rows());
        for key in keys {
            let result = trial(&options, &world, key).unwrap();
            assert_eq!(result.winner, None);
            assert_eq!(result.stop, "horizon");
            assert_eq!(result.a_result.unit_spend, 600.0);
            assert_eq!(result.b_result.unit_spend, 600.0);
            assert_eq!(result.a_result.bank, 13.0);
            assert_eq!(result.b_result.bank, 13.0);
            assert!(
                result.b_result.health_weighted_unit_matter
                    <= result.b_result.surviving_unit_matter
            );
        }
    }

    #[test]
    fn surviving_value_weights_each_unit_by_its_own_health() {
        let mut world = World::with_content(&["....."], content());
        let base_id = world.spawn(0, "factory", 0, 0);
        let tank = world.spawn(0, "tank", 1, 0);
        world.entity_mut(&tank).hp = world.def("tank").max_hp / 4.0;
        world.spawn(0, "grunt", 2, 0);
        let result = summarize(&world, &world.state, 0, &base_id, 115.0);
        assert_eq!(result.surviving_unit_count, 2);
        assert_eq!(result.surviving_unit_matter, 115.0);
        assert_eq!(result.health_weighted_unit_matter, 40.0);
    }
}

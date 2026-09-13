//! Deterministic fixture map and genesis world for the slice: bordered floor, mirrored rock
//! blobs with 180° rotational symmetry, ore patches near each start and a guaranteed corridor.
use crate::*;
use std::collections::VecDeque;

fn mix(seed: u64, x: u64, y: u64) -> u64 {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    for v in [x, y] {
        h ^= v.wrapping_add(0x9E37_79B9_7F4A_7C15);
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 31;
    }
    h
}

pub struct Start {
    pub anchor: Tile,
    pub entities: Vec<(TypeKey, Tile, Direction)>,
    pub ore: Vec<Tile>,
}

fn rotate(t: Tile, size: u16) -> Tile {
    Tile {
        x: size - 1 - t.x,
        y: size - 1 - t.y,
    }
}

/// Two mirrored starts; asymmetric mode uses the same layout with unmirrored rock.
pub fn starts(size: u16, content: &Content) -> Result<Vec<Start>> {
    if size < 16 {
        return Err("slice map generation needs at least 16 tiles".into());
    }
    let anchor = Tile { x: 4, y: 4 };
    let slots = [
        Tile { x: 4, y: 7 },
        Tile { x: 6, y: 5 },
        Tile { x: 3, y: 3 },
    ];
    let mut entities = vec![];
    for (i, key) in content.starting_roster.iter().enumerate() {
        let tile = slots
            .get(i)
            .copied()
            .ok_or("starting roster exceeds slice start slots")?;
        entities.push((key.clone(), tile, Direction::Se));
    }
    let ore: Vec<Tile> = (2..5)
        .flat_map(|x| (9..12).map(move |y| Tile { x, y }))
        .collect();
    let first = Start {
        anchor,
        entities,
        ore,
    };
    let second = Start {
        anchor: rotate(anchor, size),
        entities: first
            .entities
            .iter()
            .map(|(k, t, _)| (k.clone(), rotate(*t, size), Direction::Nw))
            .collect(),
        ore: first.ore.iter().map(|t| rotate(*t, size)).collect(),
    };
    Ok(vec![first, second])
}

pub fn generate(config: &MatchConfig, content: &Content) -> Result<WorldState> {
    if config.player_count != 2 {
        return Err("the slice map generator supports exactly two players".into());
    }
    let content = normalize_content(content.clone())?;
    let size = config.map_size;
    let n = usize::from(size);
    let mut cells = vec![TerrainCell::Floor; n * n];
    let seed = config.seed.get();
    let idx = |t: Tile| usize::from(t.y) * n + usize::from(t.x);
    let starts = starts(size, &content)?;
    // Sparse seeds dilated once give cave-like blobs; symmetry mirrors the first half.
    let mut seeds = vec![false; n * n];
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let source = if config.symmetric && (y > size / 2 || (y == size / 2 && x >= size / 2)) {
                rotate(t, size)
            } else {
                t
            };
            let near_start = starts.iter().any(|s| {
                (i32::from(s.anchor.x) - i32::from(x)).abs() <= 8
                    && (i32::from(s.anchor.y) - i32::from(y)).abs() <= 8
            });
            seeds[idx(t)] =
                !near_start && mix(seed, u64::from(source.x), u64::from(source.y)) % 100 < 5;
        }
    }
    for y in 0..size {
        for x in 0..size {
            let t = Tile { x, y };
            let border = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            let blob = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (nx, ny) = (i32::from(x) + dx, i32::from(y) + dy);
                    nx >= 0
                        && ny >= 0
                        && nx < i32::from(size)
                        && ny < i32::from(size)
                        && seeds[ny as usize * n + nx as usize]
                })
            });
            if border || blob {
                cells[idx(t)] = TerrainCell::Wall;
            }
        }
    }
    // Guarantee a route between the anchors by carving the straight segment (self-symmetric).
    let (a, b) = (starts[0].anchor, starts[1].anchor);
    let steps = (i32::from(b.x) - i32::from(a.x))
        .abs()
        .max((i32::from(b.y) - i32::from(a.y)).abs());
    for k in 0..=steps {
        let x = i32::from(a.x) + (i32::from(b.x) - i32::from(a.x)) * k / steps;
        let y = i32::from(a.y) + (i32::from(b.y) - i32::from(a.y)) * k / steps;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1)] {
            let (cx, cy) = (x + dx, y + dy);
            if cx > 0 && cy > 0 && cx < i32::from(size) - 1 && cy < i32::from(size) - 1 {
                cells[cy as usize * n + cx as usize] = TerrainCell::Floor;
            }
        }
    }
    let mut ore = vec![0.0; n * n];
    for s in &starts {
        let per_tile = config.ore_matter_per_start / s.ore.len() as f64;
        for t in &s.ore {
            cells[idx(*t)] = TerrainCell::Floor;
            ore[idx(*t)] = per_tile;
        }
        for (_, t, _) in &s.entities {
            cells[idx(*t)] = TerrainCell::Floor;
        }
    }
    // Connectivity check between anchors over four-neighbor floor.
    let mut seen = vec![false; n * n];
    let mut queue = VecDeque::from([a]);
    seen[idx(a)] = true;
    while let Some(t) = queue.pop_front() {
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let (x, y) = (i32::from(t.x) + dx, i32::from(t.y) + dy);
            if x < 0 || y < 0 || x >= i32::from(size) || y >= i32::from(size) {
                continue;
            }
            let nt = Tile {
                x: x as u16,
                y: y as u16,
            };
            if cells[idx(nt)] == TerrainCell::Floor && !seen[idx(nt)] {
                seen[idx(nt)] = true;
                queue.push_back(nt);
            }
        }
    }
    if !seen[idx(b)] {
        return Err("generated map does not connect the starts".into());
    }
    let mut entities = vec![];
    for (player, s) in starts.iter().enumerate() {
        for (slot, (key, tile, facing)) in s.entities.iter().enumerate() {
            let def = content
                .types
                .iter()
                .find(|t| t.key == *key)
                .ok_or("unknown roster type")?;
            entities.push(EntityState {
                id: identity::genesis(player as u8, slot as u32)?,
                owner: player as u8,
                type_key: key.clone(),
                tile: *tile,
                last_move_direction: *facing,
                hp: def.max_hp,
                paid_matter: 0.0,
                lifecycle: Lifecycle::Complete,
                blueprint_id: None,
                action: Order::Idle {},
                priority: Priority::Medium,
                next_action_tick: 0,
                next_move_tick: 0,
                production: None,
                support_target: None,
                engaged_target: None,
                resolved_destination: None,
                failed_move_attempts: 0,
                blocked_step: None,
                born_at_tick: None,
                order_locks: vec![],
                goal_settled: false,
                local_detour: vec![],
            });
        }
    }
    let players = (0..config.player_count)
        .map(|player_id| PlayerState {
            player_id,
            bank: config.starting_matter,
            counters: SpendCounters {
                mined: 0.0,
                total_spend: 0.0,
                unit_spend: 0.0,
                structure_spend: 0.0,
                lost_invested_matter: 0.0,
                destroyed_replacement_value: 0.0,
                damage_dealt: 0.0,
            },
            currently_eliminated: false,
            status_since_tick: 0,
            elimination_reasons: vec![],
        })
        .collect();
    let control_groups = (0..config.player_count)
        .flat_map(|owner| {
            (0..10u8).map(move |slot| ControlGroupState {
                id: ControlGroupId {
                    owner,
                    slot: slot.try_into().unwrap(),
                },
                members: vec![],
                latest_order: None,
                order_locks: vec![],
            })
        })
        .collect();
    identity::canonical_world(&WorldState {
        schema_version: Version::default(),
        tick: 0,
        last_progress_tick: 0,
        inactivity_deadline: config.stall_ticks,
        terrain: Terrain {
            width: size,
            height: size,
            cells,
        },
        ore,
        players,
        entities,
        blueprints: vec![],
        control_groups,
        survival_transitions: vec![],
        rng_state: "counter-hash:v1".into(),
    })
}

//! Export one seat's observations of a published match revision as newline-delimited JSON.
use atemporal_bots::TurnMarker;
use atemporal_content::load_archived_content;
use atemporal_server::{archive::Archive, controller::archived_request};
use atemporal_sim::{
    PlayerId, Result, Revision,
    player_timeline::{self, PlayerFrame, PublicStatsSample},
};
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
};

fn write_row(output: &mut impl Write, row: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *output, row).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct FrameRow<'a> {
    frame: &'a PlayerFrame,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_stats: Option<&'a PublicStatsSample>,
    // Half-open ranges of row-major tile indices. Omission repeats the previous mask.
    #[serde(skip_serializing_if = "Option::is_none")]
    visible_tile_ranges: Option<Vec<[usize; 2]>>,
}

fn visible_ranges(words: &[u64], tile_count: usize) -> Vec<[usize; 2]> {
    let mut ranges = Vec::new();
    let mut start = None;
    for index in 0..tile_count {
        let visible = words[index / 64] & (1 << (index % 64)) != 0;
        if visible && start.is_none() {
            start = Some(index);
        } else if !visible && let Some(first) = start.take() {
            ranges.push([first, index]);
        }
    }
    if let Some(first) = start {
        ranges.push([first, tile_count]);
    }
    ranges
}

fn export(
    path: &Path,
    player: PlayerId,
    revision: Option<Revision>,
    output: &mut impl Write,
) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("expected a match archive directory")?;
    let (_, archive) = Archive::open(path.parent().unwrap_or(Path::new(".")), name)?;
    let record = match revision {
        Some(revision) => archive.rounds.iter().find(|r| r.revision == revision),
        None => archive.rounds.last(),
    }
    .ok_or("requested revision has not been published")?;
    let recorded = &archive.manifest.fingerprint;
    let content = load_archived_content(&archive.content_yaml, &recorded.content_hash)?;
    let request = archived_request(&archive, &content, record.revision)?;
    let view = player_timeline::build_verified(
        &request,
        &[player],
        &AtomicBool::new(false),
        record.outcome.terminal_state_tick,
        &record.final_hash,
    )?
    .remove(&player)
    .ok_or("player is outside the match")?;
    let markers: Vec<_> = request
        .events
        .iter()
        .map(|turn| TurnMarker::for_player(turn, player))
        .collect();
    write_row(
        output,
        &serde_json::json!({
            "format": "atemporal-player-replay-v1", "revision": record.revision, "player": player,
            "round": record.round, "config": request.config, "content": content,
            "terrain": view.terrain, "outcome": record.outcome, "score": record.score, "timed": record.timed,
            "turn_markers": markers, "own_turns": view.turns, "destructions": view.destructions
        }),
    )?;
    let mut previous = None;
    for frame in view.frames() {
        let words = frame.visible_tile_words();
        let mask_changed = previous != Some(words);
        write_row(
            output,
            &FrameRow {
                frame,
                public_stats: view.public_stats.get(&frame.tick),
                visible_tile_ranges: mask_changed
                    .then(|| visible_ranges(words, view.terrain.cells.len())),
            },
        )?;
        previous = Some(words);
    }
    output.flush().map_err(|e| e.to_string())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: player_replay MATCH_ARCHIVE_DIRECTORY PLAYER_ID [REVISION]".into());
    }
    let player = args[1].parse().map_err(|_| "invalid player ID")?;
    let revision = args
        .get(2)
        .map(|v| v.parse::<Revision>().map_err(|_| "invalid revision"))
        .transpose()?;
    let mut output = io::BufWriter::new(io::stdout().lock());
    export(Path::new(&args[0]), player, revision, &mut output)
}

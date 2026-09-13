import type { MatchConfig, ServerMessage } from './contracts.generated';
type Round = Extract<ServerMessage, {kind:'round_result'}>;
/** Count completed timelines on the current match lineage, never sampled or provisional results. */
export function scoreboard(config: MatchConfig, latest: number, rounds: Map<number, Round>) {
  const result=rounds.get(latest), wins=Array<number>(config.player_count).fill(0);
  let cursor=result, complete=!!result;
  const seen=new Set<number>();
  while(cursor) {
    if(seen.has(cursor.revision)){complete=false;break;}seen.add(cursor.revision);
    if(cursor.round>0 && cursor.outcome.kind==='win') for(const player of cursor.outcome.survivors)wins[player]++;
    if(cursor.parent_revision===null || cursor.parent_revision===undefined)break;
    cursor=rounds.get(cursor.parent_revision);if(!cursor)complete=false;
  }
  const entries=result?.score?.entries??[], highest=Math.max(0,...entries.map(e=>e.adjusted_total));
  const leaders=entries.filter(e=>e.adjusted_total===highest && highest>0);
  const rule=config.objective.kind==='timed'?null:config.objective.rules.victory_rule;
  const goal=rule?.kind==='fixed_target'?`First to ${rule.points}`:rule?.kind==='lead'?`Lead by ${rule.margin}`:'Fixed-time match';
  const rows=wins.map((count,player)=>{
    const team=config.multiplayer.kind==='teams'?config.multiplayer.assignments.find(a=>a.player_id===player)?.team_id:null;
    const entry=entries.find(e=>e.side_id.kind==='player'?e.side_id.player_id===player:e.side_id.team_id===team);
    const winner=!!entry&&!!result?.score?.match_winners.some(side=>JSON.stringify(side)===JSON.stringify(entry.side_id));
    const leading=!!entry&&leaders.length===1&&leaders[0]===entry;
    const points=entry?.adjusted_total??0;
    return {player,wins:complete?count:null,team,points,winner,leading,showPoints:!!team || points!==count || (config.objective.kind!=='timed'&&config.objective.rules.time_penalty!=='none')};
  });
  return {rows,goal,round:result?.round??0,complete};
}

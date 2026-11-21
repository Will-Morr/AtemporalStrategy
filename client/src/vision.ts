import type {EntityView} from './game';
import type {MatchConfig, MissileFlight, ReconZone, Terrain, Tile, TypeDefinition} from './contracts.generated';

export function flightPosition(flight: MissileFlight, tick: number): Tile {
  const fraction = Math.max(0, Math.min(1, (tick-flight.launch_tick)/(flight.impact_tick-flight.launch_tick)));
  return {
    x: flight.origin.x + (flight.target.x-flight.origin.x)*fraction,
    y: flight.origin.y + (flight.target.y-flight.origin.y)*fraction,
  };
}

// Mirrors the eight-neighbor terrain flood in the simulation. Cache by observer tile
// because a stationary observer scans the same footprint on many playback frames.
export class GroundVision {
  private terrain?: Terrain;
  private wallKey='';
  private walls=new Set<number>();
  private footprints=new Map<string, Set<string>>();

  update(terrain:Terrain, entities:Pick<EntityView,'type_key'|'x'|'y'>[]):void {
    const walls=entities.filter(e=>e.type_key==='wall').map(e=>e.y*terrain.width+e.x);
    const key=walls.join(',');
    if(terrain===this.terrain && key===this.wallKey)return;
    this.terrain=terrain;
    this.wallKey=key;
    this.walls=new Set(walls);
    this.footprints.clear();
  }

  tiles(origin:Tile, radius:number):ReadonlySet<string> {
    const terrain=this.terrain!;
    const ox=Math.floor(origin.x), oy=Math.floor(origin.y);
    const key=ox+','+oy+':'+radius;
    const cached=this.footprints.get(key);
    if(cached)return cached;
    const index=(x:number,y:number)=>y*terrain.width+x;
    const blocked=(x:number,y:number)=>terrain.cells[index(x,y)]==='wall' || this.walls.has(index(x,y));
    const result=new Set<string>();
    const queue=[[ox,oy,0]];
    const visited=new Set([index(ox,oy)]);
    for(let i=0;i<queue.length;i++){
      const [x,y,steps]=queue[i];
      if(steps<=Math.max(Math.abs(x-ox),Math.abs(y-oy))+2)result.add(x+','+y);
      if(steps>=Math.ceil(radius) || (steps && blocked(x,y)))continue;
      for(let dy=-1;dy<=1;dy++)for(let dx=-1;dx<=1;dx++){
        const nx=x+dx,ny=y+dy;
        if(nx<0||ny<0||nx>=terrain.width||ny>=terrain.height||visited.has(index(nx,ny)))continue;
        if((nx-ox)**2+(ny-oy)**2>radius**2)continue;
        if(dx && dy && (blocked(nx,y)||blocked(x,ny)))continue;
        visited.add(index(nx,ny));
        queue.push([nx,ny,steps+1]);
      }
    }
    if(this.footprints.size>=4096)this.footprints.clear();
    this.footprints.set(key,result);
    return result;
  }
}

export function allied(multiplayer: MatchConfig['multiplayer'], player: number | null, owner: number): boolean {
  if (player === owner) return true;
  if (multiplayer.kind !== 'teams') return false;
  const side = multiplayer.assignments.find(a => a.player_id === player)?.team_id;
  return side !== undefined && multiplayer.assignments.some(a => a.player_id === owner && a.team_id === side);
}

export function playerSeesEntity(
  player: number | null,
  visible: ReadonlySet<string>,
  entity: Pick<EntityView,'owner'|'x'|'y'>,
): boolean {
  return entity.owner === player || visible.has(`${Math.floor(entity.x)},${Math.floor(entity.y)}`);
}

type Observer = Pick<EntityView,'type_key'|'x'|'y'|'owner'|'lifecycle'>;
type VisionSources = {missiles: MissileFlight[]; recon: ReconZone[]};

export function playerVisibility(
  ground: GroundVision,
  terrain: Terrain,
  entities: Observer[],
  sources: VisionSources,
  types: ReadonlyMap<string, TypeDefinition>,
  multiplayer: MatchConfig['multiplayer'],
  player: number | null,
  tick: number,
  legacy: boolean,
): Set<string> {
  const visible = new Set<string>();
  const revealCircle = (center: Tile, radius: number) => {
    for(let y=Math.max(0,Math.ceil(center.y-radius));y<=Math.min(terrain.height-1,Math.floor(center.y+radius));y++)
      for(let x=Math.max(0,Math.ceil(center.x-radius));x<=Math.min(terrain.width-1,Math.floor(center.x+radius));x++)
        if((x-center.x)**2+(y-center.y)**2<=radius**2)visible.add(`${x},${y}`);
  };
  const sharesSight = (owner: number) => player === null || allied(multiplayer, player, owner);
  ground.update(terrain, entities);
  for (const e of entities) {
    const def = types.get(e.type_key);
    if (!def || e.lifecycle !== 'complete' || !sharesSight(e.owner)) continue;
    if (legacy) revealCircle(e, def.vision);
    else for (const tile of ground.tiles(e, def.vision)) visible.add(tile);
  }
  for (const flight of sources.missiles) {
    const missile = types.get(flight.type_key)?.missile;
    if (missile?.effect === 'satellite' && sharesSight(flight.owner) &&
        flight.launch_tick < tick && tick <= flight.impact_tick) {
      revealCircle(flightPosition(flight,tick), missile.radius);
    }
  }
  for (const zone of sources.recon) {
    if (sharesSight(zone.owner) && zone.starts_at <= tick && tick < zone.expires_at) {
      revealCircle(zone.center, zone.radius);
    }
  }
  return visible;
}

interface RememberedEntity extends EntityView {
  key: string;
}

interface SightFrame {
  tick: number;
  exact: boolean;
  mask: Uint32Array;
  sightings: RememberedEntity[];
}

export class SightMemory {
  // Samples can arrive out of order, and exact seeks override sampled frames.
  private frames: SightFrame[] = [];
  private next = 0;
  private known = new Map<string, RememberedEntity>();
  private latestAppearance = new Map<string, RememberedEntity>();
  constructor(private readonly width: number, private readonly height: number) {}

  observe(tick: number, visible: ReadonlySet<string>, entities: EntityView[], exact = false): void {
    let low = 0, high = this.frames.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (this.frames[mid].tick < tick) low = mid + 1;
      else high = mid;
    }
    const existing = this.frames[low]?.tick === tick;
    if (existing && this.frames[low].exact && !exact) return;
    const bits = new Uint32Array(Math.ceil(this.width * this.height / 32));
    for (const tile of visible) {
      const comma = tile.indexOf(',');
      const index = Number(tile.slice(comma + 1)) * this.width + Number(tile.slice(0, comma));
      bits[index >>> 5] |= 1 << (index & 31);
    }
    // Keep an entry per observed tick so a later exact seek can replace any one
    // sample. Identical consecutive observations share both of their payloads.
    const previous = this.frames[low - 1] ?? this.frames[low];
    const mask = previous && bits.every((word, i) => word === previous.mask[i]) ? previous.mask : bits;
    let sightings: RememberedEntity[] = [];
    for (const e of entities) {
      if (!visible.has(`${e.x},${e.y}`)) continue;
      const key = JSON.stringify(e.id);
      let appearance = this.latestAppearance.get(key);
      if (!appearance || appearance.index !== e.index || appearance.owner !== e.owner ||
          appearance.type_key !== e.type_key || appearance.x !== e.x || appearance.y !== e.y ||
          appearance.facing !== e.facing || appearance.lifecycle !== e.lifecycle ||
          appearance.hp !== e.hp || appearance.maxHp !== e.maxHp) {
        appearance = {
          key, index:e.index, id:e.id, owner:e.owner, type_key:e.type_key, x:e.x, y:e.y,
          facing:e.facing, lifecycle:e.lifecycle, hp:e.hp, maxHp:e.maxHp,
          activity:'last seen', engaged:null, remembered:true,
        };
        this.latestAppearance.set(key, appearance);
      }
      sightings.push(appearance);
    }
    if (previous && sightings.length === previous.sightings.length &&
        sightings.every((e, i) => e === previous.sightings[i])) sightings = previous.sightings;
    this.frames.splice(low, existing ? 1 : 0, {tick, exact, mask, sightings});
    if (low < this.next) this.rewind();
  }

  private rewind(): void {
    this.known.clear();
    this.next = 0;
  }

  remembered(tick: number, visible: ReadonlySet<string>, liveIds: ReadonlySet<string> = new Set()): EntityView[] {
    if (this.next && this.frames[this.next - 1].tick > tick) this.rewind();
    while (this.next < this.frames.length && this.frames[this.next].tick <= tick) {
      const previous = this.frames[this.next - 1];
      const frame = this.frames[this.next++];
      if (frame.mask === previous?.mask && frame.sightings === previous.sightings) continue;
      for (const [id, e] of this.known) {
        const index = e.y * this.width + e.x;
        if (frame.mask[index >>> 5] & (1 << (index & 31))) this.known.delete(id);
      }
      for (const e of frame.sightings) this.known.set(e.key, e);
    }
    return [...this.known].filter(([id, e]) =>
      !visible.has(`${e.x},${e.y}`) && !liveIds.has(id)).map(([, e]) => e);
  }
}

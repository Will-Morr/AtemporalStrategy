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
  ground.update(terrain, entities);
  for (const e of entities) {
    const def = types.get(e.type_key);
    if (!def || e.lifecycle !== 'complete' || !allied(multiplayer, player, e.owner)) continue;
    if (legacy) revealCircle(e, def.vision);
    else for (const tile of ground.tiles(e, def.vision)) visible.add(tile);
  }
  for (const flight of sources.missiles) {
    const missile = types.get(flight.type_key)?.missile;
    if (missile?.effect === 'satellite' && allied(multiplayer, player, flight.owner) &&
        flight.launch_tick < tick && tick <= flight.impact_tick) {
      revealCircle(flightPosition(flight,tick), missile.radius);
    }
  }
  for (const zone of sources.recon) {
    if (allied(multiplayer, player, zone.owner) && zone.starts_at <= tick && tick < zone.expires_at) {
      revealCircle(zone.center, zone.radius);
    }
  }
  return visible;
}

interface Sighting {
  tick: number;
  building: EntityView | null;
}

export class BuildingMemory {
  // Time indexing prevents later sightings from leaking backwards when scrubbing.
  // Each revision owns independent memory.
  private records = new Map<string, Sighting[]>();
  private frames = new Map<number, Uint32Array>();
  private latestMask?: Uint32Array;
  private exactTicks = new Set<number>();

  constructor(private readonly width: number, private readonly height: number) {}

  private upperBound(records: Sighting[], tick: number): number {
    let lo = 0, hi = records.length;
    while (lo < hi) {
      const mid = (lo+hi) >>> 1;
      if (records[mid].tick <= tick) lo = mid+1;
      else hi = mid;
    }
    return lo;
  }

  private tileIndex(tile: string): number {
    const comma = tile.indexOf(',');
    return Number(tile.slice(comma+1))*this.width + Number(tile.slice(0,comma));
  }

  observe(tick: number, visible: ReadonlySet<string>, buildings: EntityView[], exact = false): void {
    if (!exact && this.exactTicks.has(tick)) return;
    if (exact) this.exactTicks.add(tick);

    // Keep a packed mask for each observed frame so an earlier, out-of-order
    // discovery can account for empty observations already made at later ticks.
    const mask = new Uint32Array(Math.ceil(this.width*this.height/32));
    for (const tile of visible) {
      const index = this.tileIndex(tile);
      mask[index>>>5] |= 1<<(index&31);
    }
    const previous = this.latestMask;
    const current = previous && mask.every((word,index) => word === previous[index]) ? previous : mask;
    this.latestMask = current;
    this.frames.set(tick, current);

    const observed = new Map(buildings.map(building => [`${building.x},${building.y}`, building]));
    for (const tile of new Set([...this.records.keys(), ...observed.keys()])) {
      const building = observed.get(tile);
      let records = this.records.get(tile);
      if (!visible.has(tile)) {
        // An exact snapshot supersedes any earlier observation of this tick.
        if (records) {
          const index = this.upperBound(records, tick)-1;
          if (records[index]?.tick === tick) records.splice(index, 1);
        }
        continue;
      }
      if (!records) {
        if (!building) continue;
        const index = this.tileIndex(tile);
        records = [];
        for (const [at, sight] of this.frames) {
          if (at !== tick && (sight[index>>>5] & (1<<(index&31)))) records.push({tick:at, building:null});
        }
        records.sort((a,b) => a.tick-b.tick);
        this.records.set(tile, records);
      }

      const next = this.upperBound(records, tick);
      const value = building
        ? {...building, exact:undefined, silo:undefined, engaged:null, activity:'last seen', remembered:true}
        : null;
      const observation = {tick, building:value};
      if (records[next-1]?.tick === tick) records[next-1] = observation;
      else records.splice(next, 0, observation);
    }
  }

  remembered(tick: number, visible: ReadonlySet<string>): EntityView[] {
    const remembered: EntityView[] = [];
    for (const [tile, records] of this.records) {
      if (visible.has(tile)) continue;
      const last = records[this.upperBound(records,tick)-1];
      if (last?.building) remembered.push(last.building);
    }
    return remembered;
  }
}

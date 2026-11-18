import type {EntityView} from './game';
import type {Terrain, Tile} from './contracts.generated';

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

interface Sighting {tick:number; building:EntityView|null}
export class BuildingMemory {
  // Each tile stores changes to its last observed occupant. Time indexing prevents
  // a later sighting from leaking backwards when scrubbing or switching revisions.
  private records=new Map<string,Sighting[]>();

  observe(tick:number, sees:(tile:string)=>boolean, buildings:EntityView[]):EntityView[] {
    const observed=new Map(buildings.map(b=>[b.x+','+b.y,b]));
    const remembered:EntityView[]=[];
    for(const tile of new Set([...this.records.keys(),...observed.keys()])){
      const records=this.records.get(tile)??[];
      let lo=0,hi=records.length;
      while(lo<hi){const mid=(lo+hi)>>>1;if(records[mid].tick<=tick)lo=mid+1;else hi=mid;}
      const last=records[lo-1];
      if(!sees(tile)){
        if(last?.building)remembered.push(last.building);
        continue;
      }
      const building=observed.get(tile);
      if(!building && !last?.building)continue;
      const value=building ? {...building, exact:undefined, silo:undefined, engaged:null, activity:'last seen', remembered:true} : null;
      if(last?.building?.index===value?.index && last?.building?.lifecycle===value?.lifecycle && last?.building?.hp===value?.hp)continue;
      const observation={tick,building:value};
      if(last?.tick===tick)records[lo-1]=observation;
      else records.splice(lo,0,observation);
      this.records.set(tile,records);
    }
    return remembered;
  }
}

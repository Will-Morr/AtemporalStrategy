import type {Neighbors, Terrain, Tile} from './contracts.generated';

// Same neighbor order, unit-cost BFS and corner rule as sim/{world,fields}.rs.
const DIRS = [[0,-1],[1,0],[0,1],[-1,0],[1,-1],[1,1],[-1,1],[-1,-1]];
export class PreviewFields {
  private fields = new Map<string, Int32Array>();
  constructor(readonly terrain: Terrain, readonly blocked: Set<number>) {}
  index(t: Tile): number { return t.y*this.terrain.width+t.x; }
  open(t: Tile): boolean {
    return t.x>=0 && t.y>=0 && t.x<this.terrain.width && t.y<this.terrain.height && this.terrain.cells[this.index(t)]==='floor' && !this.blocked.has(this.index(t));
  }
  private step(t: Tile, dx: number, dy: number): Tile | null {
    const to={x:t.x+dx,y:t.y+dy};
    return this.open(to) && (!dx || !dy || (this.open({x:t.x+dx,y:t.y}) && this.open({x:t.x,y:t.y+dy}))) ? to : null;
  }
  field(goal: Tile, rule: Neighbors, adjacent=false): Int32Array {
    const key=`${goal.x},${goal.y}:${rule}:${adjacent}`;
    const cached=this.fields.get(key);if(cached)return cached;
    const dist=new Int32Array(this.terrain.cells.length).fill(-1),queue:Tile[]=[];
    const seed=(t:Tile)=>{if(this.open(t)&&dist[this.index(t)]<0){dist[this.index(t)]=0;queue.push(t);}};
    const dirs=DIRS.slice(0,rule==='four'?4:8);
    if(adjacent)for(const [dx,dy] of DIRS)seed({x:goal.x+dx,y:goal.y+dy});
    else if(this.open(goal))seed(goal);
    else for(const [dx,dy] of dirs){const t={x:goal.x+dx,y:goal.y+dy};if(!dx||!dy||this.step(t,-dx,-dy))seed(t);}
    for(let head=0;head<queue.length;head++)for(const [dx,dy] of dirs){const t=this.step(queue[head],dx,dy);if(t&&dist[this.index(t)]<0){dist[this.index(t)]=dist[this.index(queue[head])]+1;queue.push(t);}}
    if(this.fields.size>=256)this.fields.clear();this.fields.set(key,dist);return dist;
  }
  route(from: Tile, goal: Tile, rule: Neighbors, adjacent=false, facing="n"): Tile[] {
    const dist=this.field(goal,rule,adjacent),route=[from];
    if(!this.open(from)||dist[this.index(from)]<0)return [];
    const dirs=DIRS.slice(0,rule==='four'?4:8);
    while(dist[this.index(from)]>0){
      let next:Tile|null=null,best=dist[this.index(from)],bestRepeat=2;
      const horizontal=facing==='e'||facing==='w';
      for(const [dx,dy] of dirs){const t=this.step(from,dx,dy);const repeat=rule==='four'&&((dx!==0)===horizontal)?1:0;if(t&&dist[this.index(t)]>=0&&(dist[this.index(t)]<best||(next&&dist[this.index(t)]===best&&repeat<bestRepeat))){next=t;best=dist[this.index(t)];bestRepeat=repeat;}}
      if(!next)return [];route.push(next);facing=next.x!==from.x?'e':'n';from=next;
    }
    return route;
  }
}

export interface MiningPreview {
  key: string;
  from: Tile;
  area: {min: Tile; max: Tile};
  rule: Neighbors;
  retained?: Tile | null;
}

/** Mirror the mining prepass: retain reachable reservations, then acquire unclaimed ore.
 * Input order is authoritative entity order, not selection order. */
export function miningTargets(fields: PreviewFields, miners: MiningPreview[], oreAt: (index: number)=>number): Map<string,Tile> {
  const targets=new Map<string,Tile>(),claims=new Map<number,string>();
  const inOre=(m:MiningPreview,t:Tile)=>t.x>=m.area.min.x&&t.y>=m.area.min.y&&t.x<=m.area.max.x&&t.y<=m.area.max.y&&oreAt(fields.index(t))>0;
  for(const m of miners){const t=m.retained&&inOre(m,m.retained)?m.retained:inOre(m,m.from)?m.from:null;if(t&&!claims.has(fields.index(t)))claims.set(fields.index(t),m.key);}
  for(const m of miners){
    const valid=(t:Tile)=>inOre(m,t)&&fields.open(t)&&(!claims.has(fields.index(t))||claims.get(fields.index(t))===m.key);
    let goal:Tile|undefined;
    if(valid(m.from))goal=m.from;
    else if(m.retained&&valid(m.retained)&&fields.field(m.retained,m.rule)[fields.index(m.from)]>=0)goal=m.retained;
    else {
      const distances=fields.field(m.from,m.rule);let best=Infinity;
      // Tile ordering in the simulation breaks ties by x, then y.
      for(let x=m.area.min.x;x<=Math.min(m.area.max.x,fields.terrain.width-1);x++)for(let y=m.area.min.y;y<=Math.min(m.area.max.y,fields.terrain.height-1);y++){
        const t={x,y},d=distances[fields.index(t)];if(valid(t)&&d>=0&&d<best){goal=t;best=d;}
      }
    }
    if(m.retained&&claims.get(fields.index(m.retained))===m.key)claims.delete(fields.index(m.retained));
    if(goal){targets.set(m.key,goal);claims.set(fields.index(goal),m.key);}
  }
  return targets;
}

/** Include the entire dark backing stroke inside the target tile, even at minimum zoom. */
export function targetCorners(scale: number): {inset: number; span: number; arm: number; backing: number; stroke: number} {
  const backing=Math.min(4,scale*.16),inset=backing/2+scale*.04;
  return {inset,span:scale-2*inset,arm:scale*.22,backing,stroke:Math.min(2,scale*.08)};
}

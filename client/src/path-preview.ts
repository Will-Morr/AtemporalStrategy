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
  route(from: Tile, goal: Tile, rule: Neighbors, adjacent=false): Tile[] {
    const dist=this.field(goal,rule,adjacent),route=[from];
    if(!this.open(from)||dist[this.index(from)]<0)return [];
    const dirs=DIRS.slice(0,rule==='four'?4:8);
    while(dist[this.index(from)]>0){
      let next:Tile|null=null,best=dist[this.index(from)];
      for(const [dx,dy] of dirs){const t=this.step(from,dx,dy);if(t&&dist[this.index(t)]>=0&&dist[this.index(t)]<best){next=t;best=dist[this.index(t)];}}
      if(!next)return [];route.push(next);from=next;
    }
    return route;
  }
}

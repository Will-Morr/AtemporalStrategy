import type { CardinalDirection, Tile } from './contracts.generated';
import { idKey, type EntityView, type Game } from './game';

const ACTIVITY_COLORS: Record<string, string> = { combat: '#ef5350', construction: '#ffca28', mining: '#ffcd38', movement: '#81c784', idle: '#616161' };
const FACING: Record<string, [number, number]> = { n: [0, -1], ne: [1, -1], e: [1, 0], se: [1, 1], s: [0, 1], sw: [-1, 1], w: [-1, 0], nw: [-1, -1] };
const OUTPUTS: CardinalDirection[] = ['n', 'e', 's', 'w'];
const OFFSET: Record<CardinalDirection, [number, number]> = { n: [0, -1], e: [1, 0], s: [0, 1], w: [-1, 0] };

/** Canvas map, minimap and timeline. Grayscale terrain; entities, ore and orders in color. */
export class Renderer {
  camera = { x: 24, y: 24, scale: 18 };
  keys = { w: false, a: false, s: false, d: false };
  dragPan: { x: number; y: number } | null = null;
  outputDirection: CardinalDirection = 'e';
  private ctx: CanvasRenderingContext2D;
  private mini: CanvasRenderingContext2D;
  private tl: CanvasRenderingContext2D;
  private terrainCanvas: HTMLCanvasElement | null = null;

  constructor(private map: HTMLCanvasElement, private minimap: HTMLCanvasElement, private timeline: HTMLCanvasElement, private game: Game) {
    this.ctx = map.getContext('2d')!;
    this.mini = minimap.getContext('2d')!;
    this.tl = timeline.getContext('2d')!;
    window.addEventListener('keyup', e => {
      const k = e.key as keyof typeof this.keys;
      if (k in this.keys) this.keys[k] = false;
    });
  }

  resize(): void {
    this.map.width = window.innerWidth;
    this.headerHeight = document.getElementById('top')!.offsetHeight;
    this.map.height = Math.max(100, document.getElementById('panels')!.getBoundingClientRect().top);
    this.map.style.height = `${this.map.height}px`;
    this.minimap.width = this.minimap.clientWidth || 200;
    this.minimap.height = this.minimap.clientHeight || 200;
    this.timeline.style.minHeight = `${this.game.config.player_count * 12 + 14}px`;
    this.timeline.width = this.timeline.clientWidth || 600;
    this.timeline.height = this.timeline.clientHeight || 80;
  }
  private headerHeight = 76;
  centerOn(x: number, y: number): void {
    this.camera.x = x;
    this.camera.y = y;
    this.clamp();
  }
  /** Keep the map on screen: it may slide within the viewport but never leave it entirely. */
  clamp(): void {
    const t = this.game.terrain;
    if (!t) return;
    const vw = this.map.width / this.camera.scale, vh = (this.map.height - this.headerHeight) / this.camera.scale;
    const margin = 2;
    const range = (view: number, size: number, value: number) => {
      const a = view / 2 - margin, b = size - view / 2 + margin;
      return Math.max(Math.min(a, b), Math.min(Math.max(a, b), value));
    };
    this.camera.x = range(vw, t.width, this.camera.x);
    this.camera.y = range(vh, t.height, this.camera.y);
  }
  /** Initial zoom: the whole map above the bottom panels when possible, centered; returns whether it fits. */
  fit(): boolean {
    const t = this.game.terrain;
    if (!t) return false;
    if (!this.game.spectator) {this.camera.scale=26;const e=this.game.rawEntities().find(e=>e.owner===this.game.player);if(e)this.centerOn(e.x,e.y);return true;}
    const usable = this.map.height - this.headerHeight - 20;
    this.camera.scale = Math.max(1, Math.min(32, Math.floor(Math.min(usable / t.height, (this.map.width - 40) / t.width))));
    const fits = t.height * this.camera.scale <= usable && t.width * this.camera.scale <= this.map.width;
    this.camera.x = t.width / 2;
    this.camera.y = t.height / 2;
    return fits;
  }
  pan(dt: number): void {
    const v = (dt * 600) / this.camera.scale;
    if (this.keys.w) this.camera.y -= v;
    if (this.keys.s) this.camera.y += v;
    if (this.keys.a) this.camera.x -= v;
    if (this.keys.d) this.camera.x += v;
    if (this.keys.w || this.keys.s || this.keys.a || this.keys.d) this.clamp();
  }
  panBy(dx: number, dy: number): void {
    this.camera.x -= dx / this.camera.scale;
    this.camera.y -= dy / this.camera.scale;
    this.clamp();
  }
  zoomAt(px: number, py: number, factor: number): void {
    const before = this.worldAt(px, py);
    this.camera.scale = Math.max(1, Math.min(64, this.camera.scale * factor));
    const after = this.worldAt(px, py);
    this.camera.x += before.x - after.x;
    this.camera.y += before.y - after.y;
    this.clamp();
  }
  cycleOutput(): void {
    this.outputDirection = OUTPUTS[(OUTPUTS.indexOf(this.outputDirection) + 1) % OUTPUTS.length];
  }

  worldAt(px: number, py: number): { x: number; y: number } {
    return { x: (px - this.map.width / 2) / this.camera.scale + this.camera.x, y: (py - (this.map.height + this.headerHeight) / 2) / this.camera.scale + this.camera.y };
  }
  tileAt(px: number, py: number): Tile {
    const w = this.worldAt(px, py);
    const t = this.game.terrain;
    const clamp = (v: number, max: number) => Math.max(0, Math.min(max - 1, Math.floor(v)));
    return { x: clamp(w.x, t?.width ?? 1), y: clamp(w.y, t?.height ?? 1) };
  }
  screen(x: number, y: number): [number, number] {
    return [(x - this.camera.x) * this.camera.scale + this.map.width / 2, (y - this.camera.y) * this.camera.scale + (this.map.height + this.headerHeight) / 2];
  }
  minimapTile(px: number, py: number): Tile | null {
    const t = this.game.terrain;
    if (!t) return null;
    const r = this.minimap.getBoundingClientRect();
    return { x: Math.floor(((px - r.left) / r.width) * t.width), y: Math.floor(((py - r.top) / r.height) * t.height) };
  }
  timelineGutter(): number { return Math.min(110, this.timeline.clientWidth * .28); }
  timelinePlotWidth(): number { return Math.max(1, this.timeline.clientWidth - this.timelineGutter()); }
  timelineTick(px: number): number {
    const r = this.timeline.getBoundingClientRect();
    const { t0, t1 } = this.game.view;
    return t0 + Math.max(0, Math.min(1, (px - r.left - this.timelineGutter()) / this.timelinePlotWidth())) * (t1 - t0);
  }

  private terrainImage(): HTMLCanvasElement | null {
    const t = this.game.terrain;
    if (!t) return null;
    if (this.terrainCanvas) return this.terrainCanvas;
    const c = document.createElement('canvas');
    c.width = t.width;
    c.height = t.height;
    const g = c.getContext('2d')!;
    for (let y = 0; y < t.height; y++)
      for (let x = 0; x < t.width; x++) {
        g.fillStyle = t.cells[y * t.width + x] === 'wall' ? '#25262b' : '#74747a';
        g.fillRect(x, y, 1, 1);
      }
    this.terrainCanvas = c;
    return c;
  }

  private groupLabels = new Map<string,string>();
  draw(): void {
    this.groupLabels.clear();
    for(const group of this.game.experience.groups()) for(const id of group.members){const key=idKey(id);this.groupLabels.set(key,[this.groupLabels.get(key),String(group.id.slot)].filter(Boolean).join(','));}
    const ctx = this.ctx;
    const { width, height } = this.map;
    ctx.fillStyle = '#141416';
    ctx.fillRect(0, 0, width, height);
    const terrain = this.terrainImage();
    const t = this.game.terrain;
    if (!terrain || !t) return;
    const s = this.camera.scale;
    const [ox, oy] = this.screen(0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(terrain, ox, oy, t.width * s, t.height * s);
    // Ore stays yellow until exhausted; spent deposits are unambiguously gray.
    for (let i = 0; i < this.game.initialOre.length; i++) {
      const initial = this.game.initialOre[i];
      if (initial <= 0) continue;
      const remaining = this.game.oreAt(i);
      const x = i % t.width, y = Math.floor(i / t.width);
      const [px, py] = this.screen(x, y);
      ctx.fillStyle = remaining <= 0 ? '#85858b' : `rgba(255,205,56,${0.35 + 0.6 * Math.max(0, remaining / initial)})`;
      ctx.fillRect(px + 1, py + 1, s - 2, s - 2);
    }
    const visible = this.game.visibility();
    const sees = (tile: Tile) => this.game.spectator || visible.has(`${tile.x},${tile.y}`);
    const cornerA=this.worldAt(0,this.headerHeight),cornerB=this.worldAt(width,height);
    if (!this.game.spectator) for (let y=Math.max(0,Math.floor(cornerA.y));y<Math.min(t.height,Math.ceil(cornerB.y));y++) for(let x=Math.max(0,Math.floor(cornerA.x));x<Math.min(t.width,Math.ceil(cornerB.x));x++) if(t.cells[y*t.width+x]==='floor' && !sees({x,y})) { const [px,py]=this.screen(x,y);ctx.fillStyle='#48484f';ctx.fillRect(px,py,s+.5,s+.5); }
    const views = this.game.entities();
    const byIndex = new Map(views.map(v => [v.index, v]));
    // Combat effects from events near the playhead (presentation only).
    const tick = this.game.playhead;
    for (const e of this.game.eventsNear(Math.floor(tick), 3)) {
      const age = tick - e.tick;
      const ev = e.event;
      if (ev.kind === 'attack') {
        if (!sees(ev.source_tile) || !sees(ev.target_tile)) continue;
        const [ax, ay] = this.screen(ev.source_tile.x + 0.5, ev.source_tile.y + 0.5);
        const [bx, by] = this.screen(ev.target_tile.x + 0.5, ev.target_tile.y + 0.5);
        ctx.strokeStyle = ev.visual_style === 'artillery' ? '#ffd54f' : ev.visual_style === 'melee' ? '#ff7043' : '#ffffff';
        ctx.globalAlpha = Math.max(0, 1 - age / 3);
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        if (ev.visual_style === 'artillery') ctx.quadraticCurveTo((ax + bx) / 2, Math.min(ay, by) - s * 2, bx, by);
        else ctx.lineTo(bx, by);
        ctx.stroke();
        ctx.globalAlpha = 1;
      } else if (ev.kind === 'impact' || ev.kind === 'destroyed') {
        const tile = ev.kind === 'impact' ? ev.target_tile : ev.tile;
        if (!sees(tile)) continue;
        const [cx, cy] = this.screen(tile.x + 0.5, tile.y + 0.5);
        ctx.strokeStyle = ev.kind === 'destroyed' ? '#ff5252' : '#fff59d';
        ctx.globalAlpha = Math.max(0, 1 - age / 3);
        ctx.beginPath();
        ctx.arc(cx, cy, (ev.kind === 'destroyed' ? 0.8 : 0.35) * s * (0.5 + age / 3), 0, Math.PI * 2);
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
    }
    // Entities.
    for (const v of views) {
      const [px,py]=this.screen(v.x,v.y);
      if(px+s<0 || py+s<this.headerHeight || px>width || py>height) continue;
      this.drawEntity(ctx, v, s, byIndex);
    }
    // Selection highlight and orders.
    for (const v of views) {
      if (!v.id || !this.game.selection.has(idKey(v.id))) continue;
      const [px, py] = this.screen(v.x, v.y);
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 2;
      ctx.strokeRect(px - 1, py - 1, s + 2, s + 2);
      ctx.lineWidth = 1;
      const a = this.game.effectiveOrder(v);
      if (a?.kind === 'attack_move') {
        const [dx, dy] = this.screen(a.destination.x + 0.5, a.destination.y + 0.5);
        ctx.strokeStyle = '#ef5350';
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(px + s / 2, py + s / 2);
        ctx.lineTo(dx, dy);
        ctx.stroke();
        ctx.setLineDash([]);
      } else if (a?.kind === 'mine' || a?.kind === 'construct') {
        const [rx, ry] = this.screen(a.area.min.x, a.area.min.y);
        ctx.strokeStyle = a.kind === 'mine' ? '#4dd0e1' : '#ffca28';
        ctx.strokeRect(rx, ry, (a.area.max.x - a.area.min.x + 1) * s, (a.area.max.y - a.area.min.y + 1) * s);
      }
    }
    // Placement preview with output arrow.
    if (this.game.mode.kind === 'place' && this.game.hover) {
      const h = this.game.hover;
      const [px, py] = this.screen(h.x, h.y);
      ctx.strokeStyle = this.game.validPlacement(h,!!this.game.types.get(this.game.mode.type_key)?.production) ? '#ffffff' : '#ff5252';
      ctx.strokeRect(px + 1, py + 1, s - 2, s - 2);
      if (this.game.types.get(this.game.mode.type_key)?.production) {
        const [dx, dy] = OFFSET[this.outputDirection];
        const [qx, qy] = this.screen(h.x + dx, h.y + dy);
        ctx.strokeStyle = this.game.validPlacement(h,true) ? '#81c784' : '#ff5252';
        ctx.strokeRect(qx + 3, qy + 3, s - 6, s - 6);
        ctx.beginPath();ctx.moveTo(px+s/2,py+s/2);ctx.lineTo(qx+s/2,qy+s/2);ctx.stroke();
      }
    }
    // Drag rectangle.
    const d = this.game.drag;
    if (d && this.game.mode.kind === 'place') {
      for (const tile of this.game.placementTiles(this.tileAt(d.x0,d.y0),this.tileAt(d.x1,d.y1))) { const [x,y]=this.screen(tile.x,tile.y); ctx.strokeStyle=this.game.validPlacement(tile,false)?'#ffca28':'#ff5252';ctx.strokeRect(x+1,y+1,s-2,s-2); }
    }
    if (d) {
      ctx.strokeStyle = this.game.mode.kind === 'area' ? '#ffca28' : '#ffffff';
      ctx.strokeRect(Math.min(d.x0, d.x1), Math.min(d.y0, d.y1), Math.abs(d.x1 - d.x0), Math.abs(d.y1 - d.y0));
    }
    this.drawMinimap(views);
    this.drawTimeline();
  }

  private drawEntity(ctx: CanvasRenderingContext2D, v: EntityView, s: number, byIndex: Map<number, EntityView>): void {
    const def = this.game.types.get(v.type_key);
    const [px, py] = this.screen(v.x, v.y);
    const color = this.game.color(v.owner);
    const team = this.game.profiles[v.owner]?.profile?.team_id;
    if (team) { ctx.strokeStyle=color;ctx.strokeRect(px-2,py-2,s+4,s+4); } 
    ctx.fillStyle = color;
    ctx.globalAlpha = v.lifecycle === 'site' ? 0.45 : 1;
    const ghost = v.lifecycle === 'blueprint' || v.lifecycle === 'site';
    ctx.globalAlpha = ghost ? .5 : 1;
    ctx.save();
    ctx.translate(px+s/2,py+s/2);
    const [fx,fy]=FACING[v.facing] ?? [0,-1];
    if (def?.kind !== 'structure') ctx.rotate(Math.atan2(fy,fx)+Math.PI/2);
    ctx.strokeStyle='#071019';ctx.lineWidth=Math.max(1,s*.06);
    const box = (x:number,y:number,w:number,h:number) => {ctx.fillRect(x*s,y*s,w*s,h*s);ctx.strokeRect(x*s,y*s,w*s,h*s);};
    const circle = (x:number,y:number,r:number) => {ctx.beginPath();ctx.arc(x*s,y*s,r*s,0,Math.PI*2);ctx.fill();ctx.stroke();};
    const triangle = (w:number,h:number) => {ctx.beginPath();ctx.moveTo(0,-h*s);ctx.lineTo(w*s,h*s);ctx.lineTo(-w*s,h*s);ctx.closePath();ctx.fill();ctx.stroke();};
    if (def?.kind === 'structure') {
      box(-.4,-.4,.8,.8);
      ctx.fillStyle='#d7efff';
      if(v.type_key==='turret'){circle(0,0,.25);box(-.06,-.43,.12,.43);}
      else if(v.type_key==='factory'){box(-.25,-.23,.15,.3);box(.1,-.23,.15,.3);ctx.fillStyle='#071019';box(-.19,.17,.38,.23);}
      else {ctx.fillStyle='#ffffff55';box(-.36,-.04,.72,.08);}
    } else if(v.type_key==='scout') {triangle(.29,.4);ctx.fillStyle='#d7efff';triangle(.1,.2);}
    else if(v.type_key==='tank' || v.type_key==='artillery') {
      box(-.39,-.32,.18,.69);box(.21,-.32,.18,.69);box(-.24,-.29,.48,.6);
      ctx.fillStyle='#d7efff';circle(0,0,.17);box(-.06,v.type_key==='artillery'?-.49:-.39,.12,.43);
    } else if(v.type_key==='constructor') {box(-.28,-.25,.56,.55);ctx.fillStyle='#ffe28a';box(-.36,-.42,.12,.35);box(.24,-.42,.12,.35);box(-.17,-.09,.34,.12);box(-.06,-.2,.12,.34);}
    else if(v.type_key==='miner') {box(-.34,-.26,.15,.56);box(.19,-.26,.15,.56);box(-.24,-.25,.48,.52);box(-.32,-.36,.64,.13);ctx.fillStyle='#e1d9b5';box(-.13,-.15,.26,.09);}
    else if(v.type_key==='grinder') {box(-.26,-.12,.52,.5);ctx.fillStyle='#ffb69b';circle(-.2,-.24,.2);circle(.2,-.24,.2);}
    else {box(-.3,.14,.19,.25);box(.11,.14,.19,.25);triangle(.28,.23);ctx.fillStyle='#d7efff';circle(0,-.1,.12);}
    ctx.restore();ctx.globalAlpha=1;
    if(ghost){ctx.strokeStyle=color;ctx.setLineDash([3,2]);ctx.strokeRect(px,py,s,s);ctx.setLineDash([]);}
    const groups=this.groupLabels.get(idKey(v.id));
    if(s>=16 && groups){ctx.fillStyle='#fff';ctx.font='bold 10px system-ui';ctx.fillText(groups,px+s-6,py+s+9);}
    const frac=Math.max(0,Math.min(1,v.hp/v.maxHp));
    if(v.lifecycle !== 'blueprint' && frac < .999999){ctx.fillStyle='#000b';ctx.fillRect(px,py-4,s,3);ctx.fillStyle=frac>.5?'#79dc9c':frac>.25?'#ffc46b':'#ff6375';ctx.fillRect(px,py-4,s*frac,3);}
    if(v.lifecycle==='site'){ctx.fillStyle='#000b';ctx.fillRect(px,py-8,s,3);ctx.fillStyle='#ffe28a';ctx.fillRect(px,py-8,s*Math.min(1,v.maxHp/(def?.max_hp??1)),3);}
    if (v.engaged !== null && byIndex.has(v.engaged) && s >= 10) {
      const target = byIndex.get(v.engaged)!;
      const [tx, ty] = this.screen(target.x + 0.5, target.y + 0.5);
      ctx.strokeStyle = '#ef535066';
      ctx.beginPath();
      ctx.moveTo(px + s / 2, py + s / 2);
      ctx.lineTo(tx, ty);
      ctx.stroke();
    }
  }

  private drawMinimap(views: EntityView[]): void {
    const t = this.game.terrain;
    const terrain = this.terrainImage();
    if (!t || !terrain) return;
    const g = this.mini;
    const { width, height } = this.minimap;
    g.imageSmoothingEnabled = false;
    g.drawImage(terrain, 0, 0, width, height);
    const sx = width / t.width, sy = height / t.height;
    for (let i = 0; i < this.game.initialOre.length; i++) {
      if (this.game.initialOre[i] > 0) {
        g.fillStyle = this.game.oreAt(i)<=0 ? '#85858b' : '#ffcd38';
        g.fillRect((i % t.width) * sx, Math.floor(i / t.width) * sy, sx, sy);
      }
    }
    const visible=this.game.visibility();
    if(!this.game.spectator) for(let y=0;y<t.height;y++)for(let x=0;x<t.width;x++)if(t.cells[y*t.width+x]==='floor' && !visible.has(`${x},${y}`)){g.fillStyle='#48484f';g.fillRect(x*sx,y*sy,sx+.5,sy+.5);}
    for (const v of views) {
      g.fillStyle = this.game.color(v.owner);
      g.fillRect(v.x * sx, v.y * sy, Math.max(2, sx), Math.max(2, sy));
    }
    const vw = this.map.width / this.camera.scale, vh = (this.map.height - this.headerHeight) / this.camera.scale;
    g.strokeStyle = '#fff';
    g.strokeRect((this.camera.x - vw / 2) * sx, (this.camera.y - vh / 2) * sy, vw * sx, vh * sy);
  }

  private drawTimeline(): void {
    const g = this.tl;
    const { width, height } = this.timeline;
    g.fillStyle = '#111114';
    g.fillRect(0, 0, width, height);
    const rev = this.game.rev();
    if (!rev) return;
    const { t0, t1 } = this.game.view;
    const gutter = this.timelineGutter();
    const x = (tick: number) => gutter + ((tick - t0) / (t1 - t0)) * (width - gutter);
    const players = this.game.config.player_count;
    const rowH = (height - 14) / players;

    // Immutable history hatch.
    if (this.game.editableFrom > 0) {
      g.fillStyle = '#ffffff12';
      const edge = Math.max(0,Math.min(width,x(this.game.editableFrom)));
      g.fillRect(0, 0, edge, height);
      g.save();g.beginPath();g.rect(0,0,edge,height);g.clip();g.strokeStyle='#aaa5';
      for(let hx=-height;hx<edge;hx+=10){g.beginPath();g.moveTo(hx,0);g.lineTo(hx+height,height);g.stroke();}g.restore();
    }
    const turns = this.game.experience.turns.get(this.game.current) ?? [];
    const latest:Record<number,number>={}, actionTicks:Record<number,number[]>={};
    for(let p=0;p<players;p++) {
      const own=turns.filter(t=>t.player===p && t.commands.length>0);
      const recent=own.reduce<typeof own[number]|undefined>((a,b)=>!a||b.round>a.round?b:a,undefined);
      actionTicks[p]=[...new Set(own.map(t=>t.tick))].sort((a,b)=>a-b);
      if(recent)latest[p]=recent.tick;
      g.fillStyle=this.game.color(p)+'22';g.fillRect(0,p*rowH,width,rowH-1);
      g.fillStyle=this.game.color(p);g.fillRect(0,p*rowH,3,rowH-1);
      for(const tick of actionTicks[p])g.fillRect(x(tick)-1,p*rowH+2,2,rowH-4);
      if(recent){g.fillStyle=this.game.color(p);g.fillRect(x(recent.tick)-3,p*rowH,6,rowH-1);g.strokeStyle='#ffffff';g.strokeRect(x(recent.tick)-4,p*rowH+.5,8,rowH-2);}

    }
    this.timeline.dataset.actionTicks=JSON.stringify(actionTicks);this.timeline.dataset.latestTicks=JSON.stringify(latest);
    // Ruler labels.
    g.fillStyle = '#9a9aa6';
    g.font = '10px system-ui';
    const span = t1 - t0;
    const step = Math.pow(10, Math.floor(Math.log10(span))) / (span / Math.pow(10, Math.floor(Math.log10(span))) < 3 ? 5 : 1);
    for (let tick = Math.ceil(t0 / step) * step; tick <= t1; tick += step) g.fillText(String(tick), x(tick) + 2, height - 3);
    // Authoritative delivery ticks, including indirect group recipients and settings.
    const round=this.game.experience.rounds.get(this.game.current);

    const ticks = new Set<number>();
    for(const turn of turns) for(const c of turn.commands) {
      const outcome=round?.command_outcomes.find(o=>JSON.stringify(o.command_id)===JSON.stringify(c.id));
      if(outcome?.applied_entities.some(id=>this.game.selection.has(idKey(id)))) ticks.add(turn.tick);
    }
    g.fillStyle='#c2a2ff';
    for(const tick of ticks){g.fillRect(x(tick)-2,0,4,height-14);g.beginPath();g.moveTo(x(tick)-5,0);g.lineTo(x(tick)+5,0);g.lineTo(x(tick),7);g.fill();}
    this.timeline.dataset.orderTicks=[...ticks].sort((a,b)=>a-b).join(',');
    if(this.game.viewingPreview){
      const frontier=x(this.game.preview!.through_tick);
      g.fillStyle='#05080bd9';g.fillRect(Math.max(gutter,frontier),0,Math.max(0,width-Math.max(gutter,frontier)),height-14);
      g.fillStyle='#5eead4';g.fillRect(Math.max(gutter,frontier)-1,0,2,height-14);
      this.timeline.dataset.availableThrough=String(this.game.preview!.through_tick);
    } else delete this.timeline.dataset.availableThrough;
    // Draft marker and playhead.
    if (this.game.draft.tick !== null) {
      g.fillStyle = '#ffca28';
      g.fillRect(x(this.game.draft.tick) - 3, 0, 6, 7);
      g.save();g.setLineDash([3,3]);g.strokeStyle='#ffca28';g.beginPath();g.moveTo(x(this.game.draft.tick),0);g.lineTo(x(this.game.draft.tick),height);g.stroke();g.restore();
    }
    g.fillStyle = '#ffffff';
    g.fillRect(x(this.game.playhead) - 1, 0, 2, height);
    if (this.game.progress) {
      g.fillStyle = '#79d7ff';
      g.fillRect(0, height - 2, (this.game.progress.tick / Math.max(1, this.game.progress.end)) * width, 2);
    }
    // Keep player/latest-tick labels separate from dense early action markers.
    g.fillStyle='#152331';g.fillRect(0,0,gutter-2,height);
    g.font='9px system-ui';
    for(let p=0;p<players;p++) {
      g.fillStyle=this.game.color(p);g.fillRect(0,p*rowH,3,rowH-1);
      g.fillStyle='#e5f2ff';g.fillText(`P${p} · ${latest[p] === undefined ? '—' : `tick ${latest[p]}`}`,7,p*rowH+10);
    }
    g.fillStyle='#9aafc1';g.font='9px system-ui';g.fillText('Latest write',7,height-3);
  }
}

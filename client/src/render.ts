import type { CardinalDirection, Tile } from './contracts.generated';
import { idKey, type EntityView, type Game } from './game';

const ACTIVITY_COLORS: Record<string, string> = { combat: '#ef5350', construction: '#ffca28', mining: '#4dd0e1', movement: '#81c784', idle: '#616161' };
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
    this.map.height = window.innerHeight;
    this.minimap.width = this.minimap.clientWidth || 200;
    this.minimap.height = this.minimap.clientHeight || 200;
    this.timeline.width = this.timeline.clientWidth || 600;
    this.timeline.height = this.timeline.clientHeight || 80;
  }
  centerOn(x: number, y: number): void {
    this.camera.x = x;
    this.camera.y = y;
    this.clamp();
  }
  /** Keep the map on screen: it may slide within the viewport but never leave it entirely. */
  clamp(): void {
    const t = this.game.terrain;
    if (!t) return;
    const vw = this.map.width / this.camera.scale, vh = this.map.height / this.camera.scale;
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
    const usable = this.map.height - 270;
    this.camera.scale = Math.max(8, Math.min(32, Math.floor(Math.min(usable / t.height, (this.map.width - 40) / t.width))));
    const fits = t.height * this.camera.scale <= usable && t.width * this.camera.scale <= this.map.width;
    if (fits) {
      this.camera.x = t.width / 2;
      this.camera.y = t.height / 2 + 135 / this.camera.scale;
    }
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
    this.camera.scale = Math.max(4, Math.min(64, this.camera.scale * factor));
    const after = this.worldAt(px, py);
    this.camera.x += before.x - after.x;
    this.camera.y += before.y - after.y;
    this.clamp();
  }
  cycleOutput(): void {
    for (let i=0;i<4;i++) {
      this.outputDirection = OUTPUTS[(OUTPUTS.indexOf(this.outputDirection) + 1) % OUTPUTS.length];
      if (!this.game.hover || this.game.validPlacement(this.game.hover,true)) break;
    }
  }
  worldAt(px: number, py: number): { x: number; y: number } {
    return { x: (px - this.map.width / 2) / this.camera.scale + this.camera.x, y: (py - this.map.height / 2) / this.camera.scale + this.camera.y };
  }
  tileAt(px: number, py: number): Tile {
    const w = this.worldAt(px, py);
    const t = this.game.terrain;
    const clamp = (v: number, max: number) => Math.max(0, Math.min(max - 1, Math.floor(v)));
    return { x: clamp(w.x, t?.width ?? 1), y: clamp(w.y, t?.height ?? 1) };
  }
  screen(x: number, y: number): [number, number] {
    return [(x - this.camera.x) * this.camera.scale + this.map.width / 2, (y - this.camera.y) * this.camera.scale + this.map.height / 2];
  }
  minimapTile(px: number, py: number): Tile | null {
    const t = this.game.terrain;
    if (!t) return null;
    const r = this.minimap.getBoundingClientRect();
    return { x: Math.floor(((px - r.left) / r.width) * t.width), y: Math.floor(((py - r.top) / r.height) * t.height) };
  }
  timelineTick(px: number): number {
    const r = this.timeline.getBoundingClientRect();
    const { t0, t1 } = this.game.view;
    return t0 + ((px - r.left) / r.width) * (t1 - t0);
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
        g.fillStyle = t.cells[y * t.width + x] === 'wall' ? '#2a2a2e' : '#5c5c62';
        g.fillRect(x, y, 1, 1);
      }
    this.terrainCanvas = c;
    return c;
  }

  draw(): void {
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
    // Ore: cyan tint scaled by remaining matter.
    for (let i = 0; i < this.game.initialOre.length; i++) {
      const initial = this.game.initialOre[i];
      if (initial <= 0) continue;
      const remaining = this.game.oreAt(i);
      const x = i % t.width, y = Math.floor(i / t.width);
      const [px, py] = this.screen(x, y);
      ctx.fillStyle = `rgba(77,208,225,${0.15 + 0.6 * Math.max(0, remaining / initial)})`;
      ctx.fillRect(px + 1, py + 1, s - 2, s - 2);
    }
    // Blueprints from exact state.
    const exact = this.game.exact?.state;
    if (exact && this.game.exact?.revision === this.game.current && this.game.exact.tick === Math.floor(this.game.playhead)) {
      for (const b of exact.blueprints) {
        if (b.site_id) continue;
        const [px, py] = this.screen(b.tile.x, b.tile.y);
        ctx.strokeStyle = this.game.color(b.owner);
        ctx.setLineDash([3, 3]);
        ctx.strokeRect(px + 2, py + 2, s - 4, s - 4);
        ctx.setLineDash([]);
      }
    }
    const views = this.game.entities();
    const byIndex = new Map(views.map(v => [v.index, v]));
    // Combat effects from events near the playhead (presentation only).
    const tick = this.game.playhead;
    for (const e of this.game.eventsNear(Math.floor(tick), 3)) {
      const age = tick - e.tick;
      const ev = e.event;
      if (ev.kind === 'attack') {
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
    for (const v of views) this.drawEntity(ctx, v, s, byIndex);
    // Selection highlight and orders.
    for (const v of views) {
      if (!v.id || !this.game.selection.has(idKey(v.id))) continue;
      const [px, py] = this.screen(v.x, v.y);
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 2;
      ctx.strokeRect(px - 1, py - 1, s + 2, s + 2);
      ctx.lineWidth = 1;
      const a = v.exact?.action;
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
    // Draft ghosts.
    for (const c of this.game.draft.commands) {
      const cmd = c.command;
      if (cmd.kind === 'place_blueprints') {
        for (const tile of cmd.tiles) {
          const [px, py] = this.screen(tile.x, tile.y);
          ctx.strokeStyle = '#ffca28';
          ctx.setLineDash([2, 2]);
          ctx.strokeRect(px + 2, py + 2, s - 4, s - 4);
          ctx.setLineDash([]);
          ctx.fillStyle = '#ffca28';
          ctx.font = `${Math.max(8, s * 0.5)}px system-ui`;
          ctx.fillText(cmd.type_key[0].toUpperCase(), px + s * 0.3, py + s * 0.7);
        }
      } else if (cmd.kind === 'assign_order' || cmd.kind === 'assign_group_order') {
        const o = cmd.order;
        if (o.kind === 'attack_move') {
          const [dx, dy] = this.screen(o.destination.x + 0.5, o.destination.y + 0.5);
          ctx.strokeStyle = '#ffca28';
          ctx.beginPath();
          ctx.arc(dx, dy, s * 0.4, 0, Math.PI * 2);
          ctx.stroke();
        } else if (o.kind === 'mine' || o.kind === 'construct') {
          const [rx, ry] = this.screen(o.area.min.x, o.area.min.y);
          ctx.strokeStyle = '#ffca28';
          ctx.setLineDash([4, 2]);
          ctx.strokeRect(rx, ry, (o.area.max.x - o.area.min.x + 1) * s, (o.area.max.y - o.area.min.y + 1) * s);
          ctx.setLineDash([]);
        }
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
    if (def?.kind === 'structure') {
      ctx.fillRect(px + s * 0.1, py + s * 0.1, s * 0.8, s * 0.8);
      ctx.fillStyle = '#000';
      ctx.font = `${Math.max(7, s * 0.45)}px system-ui`;
      ctx.fillText(v.type_key[0].toUpperCase(), px + s * 0.32, py + s * 0.68);
    } else {
      ctx.beginPath();
      ctx.arc(px + s / 2, py + s / 2, s * 0.36, 0, Math.PI * 2);
      ctx.fill();
      const [fx, fy] = FACING[v.facing] ?? [0, -1];
      const len = Math.hypot(fx, fy) || 1;
      ctx.strokeStyle = '#000';
      ctx.lineWidth = Math.max(1, s * 0.08);
      ctx.beginPath();
      ctx.moveTo(px + s / 2, py + s / 2);
      ctx.lineTo(px + s / 2 + (fx / len) * s * 0.36, py + s / 2 + (fy / len) * s * 0.36);
      ctx.stroke();
      ctx.lineWidth = 1;
      ctx.fillStyle = '#000';
      ctx.font = `${Math.max(6, s * 0.35)}px system-ui`;
      ctx.fillText(v.type_key[0], px + s * 0.38, py + s * 0.62);
    }
    ctx.globalAlpha = 1;
    if (s >= 10) {ctx.fillStyle='#fff';ctx.font='7px system-ui';ctx.fillText(String(v.owner),px+s-6,py+s-1);}
    // Health bar: filled versus dim segment; sites show completion instead.
    const frac = Math.max(0, Math.min(1, v.hp / v.maxHp));
    ctx.fillStyle = '#0008';
    ctx.fillRect(px + 1, py - 3, s - 2, 3);
    ctx.fillStyle = v.lifecycle === 'site' ? '#ffca28' : frac > 0.5 ? '#66bb6a' : frac > 0.25 ? '#ffa726' : '#ef5350';
    ctx.fillRect(px + 1, py - 3, (s - 2) * (v.lifecycle === 'site' ? Math.min(1, v.maxHp / (def?.max_hp ?? 1)) : frac), 3);
    if (v.lifecycle === 'site') {
      ctx.fillStyle='#0008';ctx.fillRect(px+1,py-7,s-2,3);ctx.fillStyle=frac>.5?'#66bb6a':'#ef5350';ctx.fillRect(px+1,py-7,(s-2)*frac,3);
    }
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
        g.fillStyle = '#4dd0e1';
        g.fillRect((i % t.width) * sx, Math.floor(i / t.width) * sy, sx, sy);
      }
    }
    for (const v of views) {
      g.fillStyle = this.game.color(v.owner);
      g.fillRect(v.x * sx, v.y * sy, Math.max(2, sx), Math.max(2, sy));
    }
    const vw = this.map.width / this.camera.scale, vh = this.map.height / this.camera.scale;
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
    const x = (tick: number) => ((tick - t0) / (t1 - t0)) * width;
    const players = this.game.config.player_count;
    const rowH = (height - 14) / players;
    const max = Math.max(1, ...rev.timeline.map(b => b.affected_entities));
    // Immutable history hatch.
    if (this.game.editableFrom > 0) {
      g.fillStyle = '#ffffff12';
      const edge = Math.max(0,Math.min(width,x(this.game.editableFrom)));
      g.fillRect(0, 0, edge, height);
      g.save();g.beginPath();g.rect(0,0,edge,height);g.clip();g.strokeStyle='#aaa5';
      for(let hx=-height;hx<edge;hx+=10){g.beginPath();g.moveTo(hx,0);g.lineTo(hx+height,height);g.stroke();}g.restore();
    }
    for (const b of rev.timeline) {
      const bx = x(b.from_tick), bw = Math.max(1, x(b.to_tick_exclusive) - bx);
      if (bx > width || bx + bw < 0) continue;
      const h = Math.max(2, (b.affected_entities / max) * (rowH - 2));
      g.fillStyle = ACTIVITY_COLORS[b.activity] ?? '#888';
      g.fillRect(bx, b.player_id * rowH + (rowH - h), bw, h);
    }
    for (let p = 0; p < players; p++) {
      g.fillStyle = this.game.color(p);
      g.fillRect(0, p * rowH, 3, rowH - 1);
    }
    // Ruler labels.
    g.fillStyle = '#9a9aa6';
    g.font = '10px system-ui';
    const span = t1 - t0;
    const step = Math.pow(10, Math.floor(Math.log10(span))) / (span / Math.pow(10, Math.floor(Math.log10(span))) < 3 ? 5 : 1);
    for (let tick = Math.ceil(t0 / step) * step; tick <= t1; tick += step) g.fillText(String(tick), x(tick) + 2, height - 3);
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
  }
}

import type {CardinalDirection, DraftCommand, DraftItemRef, Tile, TypeDefinition, WorldState} from './contracts.generated';

const offsets = {n: [0, -1], e: [1, 0], s: [0, 1], w: [-1, 0]};
const directions: CardinalDirection[] = ['n', 'e', 's', 'w'];
const tileKey = (t: Tile) => `${t.x},${t.y}`;
const blueprintKey = (ref: DraftItemRef) => ref.kind === 'persistent'
  ? JSON.stringify(ref.id) : `${ref.local_id}:${ref.item_index}`;
const coordinates = (t: Tile) => `(${t.x}, ${t.y})`;

export function outputTile(tile: Tile, direction: CardinalDirection): Tile {
  const [dx, dy] = offsets[direction];
  return {x: tile.x + dx, y: tile.y + dy};
}

type Placement = Extract<DraftCommand['command'], {kind: 'place_blueprints'}>;
type Blocker = {
  tile: Tile;
  typeKey: string;
  output?: Tile;
  blueprint?: boolean;
};
type Index = Map<string, Blocker[]>;

function indexBlocker(index: Index, tile: Tile, blocker: Blocker): void {
  const key = tileKey(tile);
  const blockers = index.get(key) ?? [];
  blockers.push(blocker);
  index.set(key, blockers);
}

function unindexBlocker(index: Index, tile: Tile, blocker: Blocker): void {
  const key = tileKey(tile);
  const remaining = index.get(key)?.filter(entry => entry !== blocker) ?? [];
  if (remaining.length) index.set(key, remaining);
  else index.delete(key);
}

export class PlacementPlan {
  private readonly bodies: Index = new Map();
  private readonly outputs: Index = new Map();
  private readonly blueprints = new Map<string, Blocker>();

  constructor(
    private readonly terrain: WorldState['terrain'],
    private readonly types: ReadonlyMap<string, TypeDefinition>,
    state: WorldState,
    player: number | null,
    visible: ReadonlySet<string>,
    fullVision: boolean,
    private readonly reserveFactoryOutputs: boolean,
  ) {
    const sites = new Map(state.blueprints.flatMap(b => b.site_id && b.owner === player
      ? [[JSON.stringify(b.site_id), b.id] as const] : []));
    for (const e of state.entities) {
      if (types.get(e.type_key)?.kind !== 'structure' || (!fullVision && e.owner !== player && !visible.has(tileKey(e.tile)))) continue;
      const blueprintId = e.owner === player && e.lifecycle === 'site' ? e.blueprint_id ?? sites.get(JSON.stringify(e.id)) : null;
      const ref = blueprintId ? {kind: 'persistent' as const, id: blueprintId} : undefined;
      this.add({
        tile: e.tile,
        typeKey: e.type_key,
        output: e.production && !types.get(e.type_key)?.silo ? e.production.output_tile : undefined,
      }, ref);
    }
    for (const b of state.blueprints) {
      if (b.site_id || b.owner !== player) continue;
      const output = b.output_direction ? outputTile(b.tile, b.output_direction) : undefined;
      this.add({
        tile: b.tile,
        typeKey: b.type_key,
        output,
        blueprint: true,
      }, {kind: 'persistent', id: b.id});
    }
  }

  private add(blocker: Blocker, ref?: DraftItemRef): void {
    indexBlocker(this.bodies, blocker.tile, blocker);
    if (this.reserveFactoryOutputs && blocker.output) indexBlocker(this.outputs, blocker.output, blocker);
    if (ref) this.blueprints.set(blueprintKey(ref), blocker);
  }

  private cancel(ref: DraftItemRef): void {
    const key = blueprintKey(ref);
    const blocker = this.blueprints.get(key);
    if (!blocker) return;
    unindexBlocker(this.bodies, blocker.tile, blocker);
    if (this.reserveFactoryOutputs && blocker.output) unindexBlocker(this.outputs, blocker.output, blocker);
    this.blueprints.delete(key);
  }

  apply(draft: DraftCommand): void {
    const command = draft.command;
    if (command.kind === 'cancel_blueprints') {
      for (const ref of command.blueprint_ids) this.cancel(ref);
    } else if (command.kind === 'place_blueprints') {
      const def = this.types.get(command.type_key);
      for (const [item_index, tile] of command.tiles.entries()) {
        const output = def?.production && !def.silo
          ? this.chooseOutput(tile, command.output_directions?.[item_index], undefined) : undefined;
        this.add({tile, typeKey: command.type_key, output, blueprint: true},
          {kind: 'draft', local_id: draft.local_id, item_index});
      }
    }
  }

  private terrainIssue(tile: Tile): string | undefined {
    const {width, height, cells} = this.terrain;
    if (tile.x < 0 || tile.x >= width || tile.y < 0 || tile.y >= height) return 'is outside the map';
    if (cells[tile.y * width + tile.x] !== 'floor') return 'is on rock';
  }

  private chooseOutput(tile: Tile, direction: CardinalDirection | undefined, newBodies?: Index): Tile | undefined {
    if (direction) return outputTile(tile, direction);
    return directions.map(candidate => outputTile(tile, candidate)).find(out =>
      !this.terrainIssue(out) && !this.bodies.has(tileKey(out)) && !newBodies?.has(tileKey(out)));
  }

  private describe(blocker: Blocker): string {
    return `${blocker.typeKey.replaceAll('_', ' ')}${blocker.blueprint ? ' blueprint' : ''} at ${coordinates(blocker.tile)}`;
  }

  issue(command: Placement): string | null {
    const def = this.types.get(command.type_key);
    if (def?.kind !== 'structure') return 'Only buildings can be placed.';
    if (!command.tiles.length) return 'Choose a tile for the building.';
    if (command.output_directions && command.output_directions.length !== command.tiles.length) {
      return 'Choose one output direction for each factory.';
    }
    const newBodies: Index = new Map();
    const newOutputs: Index = new Map();
    const typeKey = command.type_key;
    for (const [index, tile] of command.tiles.entries()) {
      const location = `${typeKey.replaceAll('_', ' ')} at ${coordinates(tile)}`;
      const terrainIssue = this.terrainIssue(tile);
      if (terrainIssue) return `Cannot place ${location}. The tile ${terrainIssue}.`;
      const key = tileKey(tile);
      const body = newBodies.get(key)?.[0] ?? this.bodies.get(key)?.[0];
      if (body) return `Cannot place ${location}. The tile already holds a ${this.describe(body)}.`;
      const output = newOutputs.get(key)?.[0] ?? this.outputs.get(key)?.[0];
      if (output) return `Cannot place ${location}. The tile is the output of the ${this.describe(output)}.`;

      const building = {tile, typeKey, blueprint: true};
      if (def.production && !def.silo) {
        const out = this.chooseOutput(tile, command.output_directions?.[index], newBodies);
        if (!out) return `Cannot place ${location}. No adjacent tile is open for its output.`;
        const outputTerrainIssue = this.terrainIssue(out);
        if (outputTerrainIssue) return `Cannot place ${location}. Its output at ${coordinates(out)} ${outputTerrainIssue}.`;
        const outKey = tileKey(out);
        const obstacle = newBodies.get(outKey)?.[0] ?? this.bodies.get(outKey)?.[0];
        if (obstacle) return `Cannot place ${location}. Its output at ${coordinates(out)} is blocked by the ${this.describe(obstacle)}.`;
        if (this.reserveFactoryOutputs) indexBlocker(newOutputs, out, building);
      }
      indexBlocker(newBodies, tile, building);
    }
    return null;
  }
}

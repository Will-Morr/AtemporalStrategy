import {DraftIds, draftRecoveryNotice, packDraft, type Draft, type DraftRecovery} from './draft-storage';
import type {Command, DraftCommand, DraftItemRef, FutureOrderPolicy} from './contracts.generated';

export class DraftEditor {
  private state: Draft = {tick: null, commands: [], undo: [], redo: [], policy: 'keep', selected: null};
  private readonly ids = new DraftIds();
  private recoveryRecord: DraftRecovery | null = null;
  private batching = false;

  get draft(): Readonly<Draft> { return this.state; }
  get recovery(): DraftRecovery | null { return this.recoveryRecord; }
  get inBatch(): boolean { return this.batching; }

  newCommand(command: Command, policy: FutureOrderPolicy = 'keep'): DraftCommand {
    return {local_id: this.ids.create(), command, future_orders: policy};
  }

  replace(commands: DraftCommand[], tick: number | null, selected = this.state.selected): void {
    const before = this.state;
    if (before.commands === commands && before.tick === tick && before.selected === selected) return;
    this.state = {...before, commands, tick, selected};
    if (!this.batching) this.recordEdit(before);
  }

  private recordEdit(before: Draft): void {
    this.state = {...this.state, undo: [...before.undo, {commands: before.commands, tick: before.tick}], redo: []};
  }

  batch(action: () => boolean): boolean {
    const before = this.state;
    this.batching = true;
    try {
      if (!action()) {
        this.state = before;
        return false;
      }
      if (this.state.commands !== before.commands || this.state.tick !== before.tick) this.recordEdit(before);
      return true;
    } catch (error) {
      this.state = before;
      throw error;
    } finally {
      this.batching = false;
    }
  }

  write(entry: DraftCommand, tick: number): void {
    const {command, future_orders} = entry;
    const previous = this.state.commands.at(-1);
    const combined = this.batching && previous?.future_orders === future_orders
      ? combineRecipients(previous.command, command) : null;
    if (previous && combined) {
      this.replace([...this.state.commands.slice(0, -1), {...previous, command: combined}], this.state.tick);
      return;
    }

    this.replace([...withoutReassignedRecipients(this.state.commands, command), entry], tick);
  }

  withoutBlueprints(refs: DraftItemRef[]): DraftCommand[] {
    const removed = new Map<string, Set<number>>();
    for (const ref of refs) {
      if (ref.kind !== 'draft') continue;
      const indices = removed.get(ref.local_id) ?? new Set<number>();
      indices.add(ref.item_index);
      removed.set(ref.local_id, indices);
    }

    // A partial wall-line deletion shifts item indices for later settings and cancellations.
    const commands: DraftCommand[] = [];
    const newIndices = new Map<string, Map<number, number>>();
    for (const draft of this.state.commands) {
      const command = draft.command;
      const excluded = removed.get(draft.local_id);
      if (command.kind === 'place_blueprints' && excluded) {
        const indices = new Map<number, number>();
        const tiles = command.tiles.filter((_, index) => {
          if (excluded.has(index)) return false;
          indices.set(index, indices.size);
          return true;
        });
        newIndices.set(draft.local_id, indices);
        const output_directions = command.output_directions?.filter((_, index) => indices.has(index)) ?? null;
        if (tiles.length) commands.push({...draft, command: {...command, tiles, output_directions}});
      } else if ((command.kind === 'configure_blueprints' || command.kind === 'cancel_blueprints') &&
                 command.blueprint_ids.some(ref => ref.kind === 'draft' && newIndices.has(ref.local_id))) {
        const blueprint_ids = command.blueprint_ids.flatMap<DraftItemRef>(ref => {
          if (ref.kind !== 'draft' || !newIndices.has(ref.local_id)) return [ref];
          const item_index = newIndices.get(ref.local_id)!.get(ref.item_index);
          return item_index === undefined ? [] : [{...ref, item_index}];
        });
        if (blueprint_ids.length) commands.push({...draft, command: {...command, blueprint_ids}});
      } else commands.push(draft);
    }
    return commands;
  }

  withoutCommand(index: number): DraftCommand[] {
    const removed = this.state.commands[index];
    if (!removed) return this.state.commands;
    // Remove dependent references to a deleted placement or queue append.
    return this.state.commands.flatMap((d, i) => {
      if (i === index) return [];
      const command = d.command;
      if (command.kind === 'configure_blueprints' || command.kind === 'cancel_blueprints') {
        const blueprint_ids = command.blueprint_ids.filter(ref => ref.kind !== 'draft' || ref.local_id !== removed.local_id);
        if (blueprint_ids.length === command.blueprint_ids.length) return [d];
        return blueprint_ids.length ? [{...d, command: {...command, blueprint_ids}}] : [];
      }
      if (command.kind === 'edit_production' && (command.edit.kind === 'remove_pending' || command.edit.kind === 'set_item_loop')) {
        const item_ids = command.edit.item_ids.filter(ref => ref.kind !== 'draft' || ref.local_id !== removed.local_id);
        if (item_ids.length === command.edit.item_ids.length) return [d];
        return item_ids.length ? [{...d, command: {...command, edit: {...command.edit, item_ids}}}] : [];
      }
      return [d];
    });
  }

  undo(): boolean {
    const previous = this.state.undo.at(-1);
    if (!previous) return false;
    const {commands, tick, undo, redo} = this.state;
    this.state = {...this.state, ...previous, selected: null, undo: undo.slice(0, -1), redo: [...redo, {commands, tick}]};
    return true;
  }

  redo(): boolean {
    const next = this.state.redo.at(-1);
    if (!next) return false;
    const {commands, tick, undo, redo} = this.state;
    this.state = {...this.state, ...next, selected: null, undo: [...undo, {commands, tick}], redo: redo.slice(0, -1)};
    return true;
  }

  clear(): void {
    this.recoveryRecord = null;
    this.state = {tick: null, commands: [], undo: [], redo: [], policy: this.state.policy, selected: null};
  }

  setPolicy(policy: FutureOrderPolicy): void {
    this.state = {...this.state, policy};
  }

  select(index: number | null): void {
    this.state = {...this.state, selected: index};
  }

  restore(original: Draft, savedRecovery: DraftRecovery | null = null) {
    const recovered = this.ids.restore(original);
    this.state = recovered.draft;
    const notice = draftRecoveryNotice(recovered.unresolved, recovered.historyAffected);
    this.recoveryRecord = recovered.evidence ? {original: packDraft(original), notice: notice.trim()} : savedRecovery;
    return {evidence: recovered.evidence, notice};
  }

  restoreUncommitted(commands: DraftCommand[], tick: number) {
    return this.restore({
      ...this.state, commands, tick: commands.length ? tick : null, selected: null, redo: [],
      undo: commands.length ? [{commands: [], tick: null}] : [],
    });
  }
}

// Keep identical payloads for disjoint factory recipients in one protocol command.
function combineRecipients(previous: Command, command: Command): Command | null {
  const field = 'factories' in command ? 'factories' :
    'entities' in command ? 'entities' : command.kind === 'configure_blueprints' ? 'blueprint_ids' : null;
  if (!field) return null;
  const old = previous as unknown as Record<string, unknown>;
  const next = command as unknown as Record<string, unknown>;
  const left = old[field] as unknown[] | undefined;
  const right = next[field] as unknown[];
  const payload = (entry: Record<string, unknown>) => JSON.stringify({...entry, [field]: undefined});
  if (!left || payload(old) !== payload(next)) return null;
  const existing = new Set(left.map(ref => JSON.stringify(ref)));
  if (right.some(ref => existing.has(JSON.stringify(ref)))) return null;
  return {...previous, [field]: [...left, ...right]} as Command;
}

// A later assignment replaces earlier assignments to the same recipients.
function withoutReassignedRecipients(commands: DraftCommand[], command: Command): DraftCommand[] {
  if (command.kind !== 'assign_order' && command.kind !== 'set_stored_order') return commands;
  const ids = command.kind === 'assign_order' ? command.entities : command.factories;
  const assigned = new Set(ids.map(id => JSON.stringify(id)));
  return commands.flatMap(entry => {
    const previous = entry.command;
    if (command.kind === 'assign_order' && previous.kind === 'assign_order') {
      const entities = previous.entities.filter(id => !assigned.has(JSON.stringify(id)));
      if (entities.length === previous.entities.length) return [entry];
      return entities.length ? [{...entry, command: {...previous, entities}}] : [];
    }
    if (command.kind === 'set_stored_order' && previous.kind === 'set_stored_order') {
      const factories = previous.factories.filter(id => !assigned.has(JSON.stringify(id)));
      if (factories.length === previous.factories.length) return [entry];
      return factories.length ? [{...entry, command: {...previous, factories}}] : [];
    }
    return [entry];
  });
}

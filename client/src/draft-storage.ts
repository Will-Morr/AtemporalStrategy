import type {DraftCommand, DraftItemRef, DraftItemRef2, FutureOrderPolicy} from './contracts.generated';

interface Snapshot {
  commands: DraftCommand[];
  tick: number | null;
}
export interface Draft extends Snapshot {
  undo: Snapshot[];
  redo: Snapshot[];
  policy: FutureOrderPolicy;
  selected: number | null;
}

// Adjacent undo states usually differ by one command. Save the shared prefix
// once, including across the current state and the redo stack.
interface SavedDraft {
  states: {keep: number; append: DraftCommand[]; tick: number | null}[];
  current: number;
  policy: FutureOrderPolicy;
  selected: number | null;
}

export function packDraft(draft: Draft): SavedDraft {
  let previous: DraftCommand[] = [];
  const states = [...draft.undo, draft, ...draft.redo.slice().reverse()].map(({commands, tick}) => {
    let keep = 0;
    while (keep < previous.length && keep < commands.length && previous[keep] === commands[keep]) keep++;
    previous = commands;
    return {keep, append: commands.slice(keep), tick};
  });
  return {states, current: draft.undo.length, policy: draft.policy, selected: draft.selected};
}

export function unpackDraft(saved: Draft | SavedDraft): Draft {
  if (!('states' in saved)) return saved;
  let previous: DraftCommand[] = [];
  const states = saved.states.map(({keep, append, tick}) => {
    previous = previous.slice(0, keep).concat(append);
    return {commands: previous, tick};
  });
  return {...states[saved.current], policy: saved.policy, selected: saved.selected,
    undo: states.slice(0, saved.current), redo: states.slice(saved.current + 1).reverse()};
}


export class DraftIds {
  private next = 0;
  private readonly used = new Set<string>();

  create(): string {
    let id: string;
    do { id = `draft-${++this.next}`; } while (this.used.has(id));
    this.used.add(id);
    return id;
  }

  restore(draft: Draft): {draft: Draft; unresolved: number; historyAffected: boolean; evidence?: Snapshot} {
    const encoder = new TextEncoder();
    const valid = new Map<string, boolean>();
    for (const {commands} of [...draft.undo, draft, ...draft.redo]) {
      for (const {local_id} of commands) {
        if (valid.has(local_id)) continue;
        valid.set(local_id, !!local_id && encoder.encode(local_id).length <= 64);
        this.used.add(local_id);
      }
    }

    // Packed snapshots share unchanged command objects. Reuse each repaired variant so
    // restoring an old draft does not expand its shared prefixes in storage.
    const renamed = new WeakMap<DraftCommand, string>();
    const variants = new WeakMap<DraftCommand, Map<string, DraftCommand>>();
    const repair = (snapshot: Snapshot): {snapshot: Snapshot; unresolved: number} => {
      const ids = new Set<string>();
      const invalid = new Set<string>();
      for (const {local_id} of snapshot.commands) {
        if (!valid.get(local_id) || ids.has(local_id)) invalid.add(local_id);
        ids.add(local_id);
      }
      if (!invalid.size) return {snapshot, unresolved: 0};

      let unresolved = 0;
      const earlier = new Map<string, DraftCommand[]>();
      const commands: DraftCommand[] = [];
      const assigned = new Set<string>();
      for (const original of snapshot.commands) {
        const {local_id, command} = original;
        let id = local_id;
        if (!valid.get(local_id) || assigned.has(id)) {
          id = renamed.get(original) ?? this.create();
          if (assigned.has(id)) id = this.create();
          else renamed.set(original, id);
        }
        assigned.add(id);
        const repairRefs = <Ref extends DraftItemRef | DraftItemRef2>(
          refs: Ref[],
          matches: (source: DraftCommand['command'], itemIndex: number) => boolean,
        ): Ref[] => {
          if (!refs.some(ref => ref.kind === 'draft' && invalid.has(ref.local_id))) return refs;
          let changed = false;
          const repaired = refs.flatMap(ref => {
            if (ref.kind !== 'draft' || !invalid.has(ref.local_id)) return [ref];
            const sources = earlier.get(ref.local_id)?.filter(
              source => matches(source.command, ref.item_index),
            );
            if (sources?.length === 1) {
              const sourceId = sources[0].local_id;
              if (sourceId === ref.local_id) return [ref];
              changed = true;
              return [{...ref, local_id: sourceId}];
            }
            changed = true;
            unresolved++;
            return [];
          });
          return changed ? repaired : refs;
        };

        let updated = command;
        if (command.kind === 'configure_blueprints' || command.kind === 'cancel_blueprints') {
          const blueprint_ids = repairRefs(command.blueprint_ids, (source, index) =>
            source.kind === 'place_blueprints' && index >= 0 && index < source.tiles.length,
          );
          if (blueprint_ids !== command.blueprint_ids) {
            if (!blueprint_ids.length) continue;
            updated = {...command, blueprint_ids};
          }
        }
        if (command.kind === 'edit_production' &&
            (command.edit.kind === 'remove_pending' || command.edit.kind === 'set_item_loop')) {
          const item_ids = repairRefs(command.edit.item_ids, (source, index) =>
            source.kind === 'edit_production' &&
            (source.edit.kind === 'append' || source.edit.kind === 'supply_launches' || source.edit.kind === 'replace_pending') &&
            index >= 0 && index < source.edit.items.length &&
            command.factories.every(factory => source.factories.some(candidate => JSON.stringify(candidate) === JSON.stringify(factory))),
          );
          if (item_ids !== command.edit.item_ids) {
            if (!item_ids.length) continue;
            updated = {...command, edit: {...command.edit, item_ids}};
          }
        }

        let entry = original;
        if (id !== local_id || updated !== command) {
          const key = JSON.stringify([id, updated === command ? null : updated]);
          const cached = variants.get(original) ?? new Map<string, DraftCommand>();
          entry = cached.get(key) ?? {...original, local_id: id, command: updated};
          cached.set(key, entry);
          variants.set(original, cached);
        }
        if (invalid.has(local_id)) {
          const prior = earlier.get(local_id) ?? [];
          prior.push(entry);
          earlier.set(local_id, prior);
        }
        commands.push(entry);
      }
      return {snapshot: {commands, tick: commands.length ? snapshot.tick : null}, unresolved};
    };

    const current = repair(draft);
    const undo = draft.undo.map(repair);
    const redo = draft.redo.map(repair);
    const evidence = current.snapshot !== draft ? draft :
      draft.undo.find((snapshot, index) => undo[index].snapshot !== snapshot) ??
      draft.redo.find((snapshot, index) => redo[index].snapshot !== snapshot);
    return {
      draft: {
        ...draft, ...current.snapshot,
        selected: current.snapshot.commands.length === draft.commands.length ? draft.selected : null,
        undo: undo.map(s => s.snapshot), redo: redo.map(s => s.snapshot),
      },
      unresolved: current.unresolved,
      historyAffected: [...undo, ...redo].some(state => state.unresolved > 0),
      evidence,
    };
  }
}

export function draftRecoveryNotice(unresolved: number, historyAffected: boolean): string {
  if (!unresolved) return historyAffected ? ' Ambiguous references were removed from the saved undo history.' : '';
  const references = unresolved === 1 ? '1 blueprint or queue reference' : `${unresolved} blueprint or queue references`;
  return ` Removed ${references} that could not be matched safely. Review your moves before committing.`;
}

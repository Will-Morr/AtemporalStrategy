import type {DraftCommand, FutureOrderPolicy} from './contracts.generated';

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

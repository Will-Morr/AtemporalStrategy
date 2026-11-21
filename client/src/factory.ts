import {siloPlan} from './silo';
import type { Game, EntityView } from './game';
import type { DraftItemRef2, EntityId, Order, Priority, QueueItemId } from './contracts.generated';

function persistentQueueKey(id: QueueItemId): string {
  const {command, target_index} = id.birth_command;
  return `persistent:${command.round}:${command.player}:${command.index}:${target_index}:${id.item_index}`;
}

function queueKey(ref: DraftItemRef2): string {
  return ref.kind === 'persistent'
    ? persistentQueueKey(ref.id)
    : `draft:${ref.item_index}:${ref.local_id}`;
}

function sameEntity(a: EntityId, b: EntityId): boolean {
  const left = a.birth_command;
  const right = b.birth_command;
  return a.occurrence === b.occurrence && a.item_index === b.item_index &&
    left.target_index === right.target_index && left.command.index === right.command.index &&
    left.command.player === right.command.player && left.command.round === right.command.round;
}

type PendingItem = {type: string; ref: DraftItemRef2 | null; loop: boolean};

export function queueBatches(pending: PendingItem[]) {
  const batches: {type: string; loop: boolean; indices: number[]}[] = [];
  pending.forEach((item, index) => {
    const last = batches.at(-1);
    if (last?.type === item.type && last.loop === item.loop) last.indices.push(index);
    else batches.push({type: item.type, loop: item.loop, indices: [index]});
  });
  return batches;
}

export function factoryPlan(g: Game, v: EntityView) {
  const p = v.exact?.production;
  let pending: PendingItem[] = v.blueprint
    ? (v.settings?.queue ?? []).map((type, index) => ({
      type, ref: null, loop: v.settings?.queue_loop_flags?.[index] ?? false,
    }))
    : (p?.pending_items ?? []).map(item => ({
      type: item.type_key, ref: {kind: 'persistent', id: item.item_id}, loop: item.loop_enabled ?? false,
    }));
  let active = p?.active_item ? structuredClone(p.active_item) : null;
  let loop = v.settings?.loop_enabled ?? p?.loop_enabled ?? false;
  let priority: Priority = v.settings?.priority ?? v.exact?.priority ?? 'medium';
  let group = p?.spawn_group;

  if (!v.blueprint && g.current === g.latest && g.draft.tick === Math.floor(g.playhead) && g.draft.commands.length) {
    // Map iteration preserves queue order while edits address items directly by identity.
    const queue = new Map(pending.map(item => [queueKey(item.ref!), item]));
    const activeKey = active ? persistentQueueKey(active.item_id) : null;
    for (const {command, local_id} of g.draft.commands) {
      if (command.kind === 'set_priority' && command.entities.some(id => sameEntity(id, v.id))) {
        priority = command.priority;
      }
      if (!('factories' in command) || !command.factories.some(id => sameEntity(id, v.id))) continue;

      if (command.kind === 'set_queue_loop') loop = command.enabled;
      if (command.kind === 'bind_factory_group') group = command.group;
      if (command.kind !== 'edit_production') continue;

      const edit = command.edit;
      switch (edit.kind) {
        case 'replace_pending':
        case 'append':
        case 'supply_launches':
          if (edit.kind === 'replace_pending') queue.clear();
          for (const [item_index, type] of edit.items.entries()) {
            const ref: DraftItemRef2 = {kind: 'draft', local_id, item_index};
            queue.set(queueKey(ref), {type, ref, loop});
          }
          break;
        case 'remove_pending':
          for (const ref of edit.item_ids) queue.delete(queueKey(ref));
          break;
        case 'set_item_loop':
          for (const ref of edit.item_ids) {
            const key = queueKey(ref);
            const item = queue.get(key);
            if (item) item.loop = edit.enabled;
            if (active && key === activeKey) active.loop_enabled = edit.enabled;
          }
          break;
        case 'cancel_active':
          active = null;
          break;
      }
    }
    pending = [...queue.values()];
  }

  const saved = group
    ? g.experience.groups().find(({id}) => id.owner === group.owner && id.slot === group.slot)?.latest_order?.order
    : null;
  const order: Order = saved ?? g.effectiveOrder(v) ?? {kind: 'idle'};
  return {pending, active, loop, priority, order, group, silo: siloPlan(g, v), inventory: (v.silo ?? p?.silo)?.inventory};
}
export function orderLabel(order: Order): string {
  switch(order.kind){case 'attack_move':return `Attack move → ${order.destination.x}, ${order.destination.y}`;case 'construct':return 'Construct in assigned area';case 'mine':return 'Mine in assigned area';case 'support':return 'Support assigned ally';default:return 'Idle';}
}

/** Persistent queue identities differ even when two factories have the same plan. */
export function factorySignature(g: Game, v: EntityView): string {
  const plan = factoryPlan(g, v);
  return JSON.stringify({
    type: v.type_key,
    pending: plan.pending.map(item => [item.type, item.loop]),
    active: plan.active ? {...plan.active, item_id: undefined} : null,
    loop: plan.loop,
    priority: plan.priority,
    order: plan.order,
    group: plan.group ? [plan.group.owner, plan.group.slot] : null,
  });
}

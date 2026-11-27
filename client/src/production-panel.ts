import {editFactoryQueue, factoryPlan, orderLabel, queueBatches, type QueueChange} from './factory';
import {idKey, type EntityView, type Game} from './game';
import {unitIcon} from './icons';
import {siloPanel} from './silo';

const button = (label: string, action: (event: MouseEvent) => void) => {
  const b = document.createElement('button'); b.textContent = label; b.onclick = action; return b;
};
const queueLabel = (text: string) => {
  const label = document.createElement('span');
  label.className = 'queue-item-label';
  label.textContent = text;
  label.title = text;
  return label;
};

export class ProductionPanel {
  private readonly siloProductionOpen = new Set<string>();
  private focusDelete: (() => void) | null = null;

  constructor(private readonly g: Game, private readonly root: HTMLElement) {
    window.addEventListener('focusin', event => {
      if (event.target instanceof Node && !root.contains(event.target)) this.focusDelete = null;
    });
  }

  deleteFocused(): boolean {
    if (!this.focusDelete) return false;
    this.focusDelete();
    return true;
  }

  render(): void {
    const {g, root} = this;
    const views = g.selectedViews().filter(v => g.types.get(v.type_key)?.production && g.canInspectPlan(v.owner));
    const signature = JSON.stringify([
      g.canStage().ok, g.fullVision, g.current === g.latest,
      views.map(v => [v.id, v.lifecycle, !!v.exact, factoryPlan(g, v)]),
    ]);
    if (root.dataset.plan === signature) return;

    const selection = JSON.stringify(views.map(v => v.id));
    const queueSelector = '.editable-factory>.queue-icons';
    const previous = root.dataset.queueSelection === selection ? root.querySelector(queueSelector) : null;
    const scrollTop = previous?.scrollTop;
    const focused = document.activeElement instanceof HTMLButtonElement ? document.activeElement : null;
    const focusedItem = focused && previous?.contains(focused) ? focused.closest<HTMLElement>('[data-queue-item]') : null;
    const focusedAction = focusedItem && focused ? Array.from(focusedItem.querySelectorAll('button')).indexOf(focused) : -1;
    root.dataset.plan = signature;
    root.dataset.queueSelection = selection;
    root.replaceChildren();
    this.focusDelete = null;
    if (!views.length) {
      delete root.dataset.editor;
      return;
    }

    if (g.fullVision || g.current !== g.latest) {
      for (const v of views) root.append(this.readOnlyQueue(v));
      return;
    }

    const factories = views.filter(v => !g.types.get(v.type_key)?.silo);
    const silos = views.filter(v => g.types.get(v.type_key)?.silo);
    const editors = silos.map(v => ({key: idKey(v.id), label: `Missile silo at ${v.x}, ${v.y}`}));
    if (factories.length) {
      const label = factories.length > 1 ? `${factories.length} factories · shared editor` : 'Factory output';
      editors.unshift({key: 'factories', label});
    }
    if (!editors.some(editor => editor.key === root.dataset.editor)) root.dataset.editor = editors[0].key;
    if (editors.length > 1) {
      const picker = document.createElement('select');
      picker.setAttribute('aria-label', 'Production editor');
      for (const {label, key} of editors) picker.add(new Option(label, key));
      picker.value = root.dataset.editor!;
      picker.onchange = () => {
        root.dataset.editor = picker.value;
        root.dataset.plan = '';
        this.render();
      };
      root.append(picker);
    }
    if (root.dataset.editor === 'factories' && factories.length) root.append(this.productionEditor(factories));
    else {
      const silo = silos.find(v => idKey(v.id) === root.dataset.editor);
      if (silo) root.append(this.productionEditor([silo]));
    }
    const current = root.querySelector(queueSelector);
    if (scrollTop) current?.scrollTo(0, scrollTop);
    if (focusedItem && focusedAction >= 0) {
      const row = Array.from(current?.querySelectorAll<HTMLElement>('[data-queue-item]') ?? [])
        .find(item => item.dataset.queueItem === focusedItem.dataset.queueItem);
      row?.querySelectorAll('button')[focusedAction]?.focus({preventScroll: true});
    }
  }

  private readOnlyQueue(view: EntityView): HTMLElement {
    const {g} = this;
    const card = document.createElement('section');
    card.className = 'factory-plan review-queue';
    const title = document.createElement('strong');
    title.textContent = `${g.name(view.owner)} · ${view.type_key} at ${Math.round(view.x)}, ${Math.round(view.y)}`;
    card.append(title);
    if (!view.exact && !view.blueprint) {
      card.append('Pause at a tick to inspect its queue.');
      return card;
    }
    const plan = factoryPlan(g, view);
    for (const text of [
      plan.active ? `Building ${plan.active.type_key} · ${Math.round(plan.active.paid_matter)} matter paid` : 'No active production',
      'Queue · ' + (plan.pending.map(item => item.type + (item.loop ? ' ↻' : '')).join(' → ') || 'empty'),
      g.types.get(view.type_key)?.silo ? 'Launch plan · ' + (plan.silo.launches.map(item => item.type_key).join(' → ') || 'empty')
        : 'New units · ' + orderLabel(plan.order),
    ]) {
      const item = document.createElement('div');
      item.textContent = text;
      card.append(item);
    }
    return card;
  }

  private productionEditor(views: EntityView[]): HTMLElement {
    const g = this.g;
    const first = views[0];
    const type = g.types.get(first.type_key);
    const shared = views.length > 1;
    const isSilo = !!type?.silo;
    const plans = views.map(v => factoryPlan(g, v));
    const plan = plans[0];
    // Only compare the fields used by each set of shared controls.
    const samePending = !shared || plans.every(p => p.pending.length === plan.pending.length &&
      p.pending.every((item, index) => item.type === plan.pending[index].type && item.loop === plan.pending[index].loop));
    const sameActive = plans.every(p =>
      p.active?.type_key === plan.active?.type_key && !!p.active?.loop_enabled === !!plan.active?.loop_enabled);
    const sameOrder = !shared || new Set(plans.map(p => JSON.stringify(p.order))).size === 1;
    const sameLoop = plans.every(p => p.loop === plan.loop);
    const enabled = g.canStage().ok;
    const color = g.color(shared ? g.player! : first.owner);
    const edit = (change: QueueChange) => {
      if (!shared) {
        editFactoryQueue(g, first, change);
        return;
      }
      g.editFactories(() => {
        for (const factory of views) editFactoryQueue(g, factory, change);
      });
    };

    const card = document.createElement('section');
    card.className = 'factory-plan';
    if (!isSilo) card.classList.add('editable-factory');
    if (shared) card.classList.add('shared-factories');

    const heading = document.createElement('strong');
    if (shared) heading.textContent = `${views.length} factories · shared commands`;
    else {
      const label = isSilo ? 'Missile production' : first.lifecycle === 'complete' ? 'Factory output'
        : first.lifecycle === 'site' ? 'Factory under construction' : 'Planned factory';
      heading.append(unitIcon(first.type_key, color), label);
    }
    card.append(heading);

    const loop = button(`↻ Loop new items: ${!sameLoop ? 'Mixed' : plan.loop ? 'On' : 'Off'}`,
      () => edit({kind: 'set_default_loop', enabled: !sameLoop || !plan.loop}));
    loop.disabled = !enabled;
    loop.setAttribute('aria-pressed', sameLoop ? String(plan.loop) : 'mixed');
    const settings = document.createElement('div');
    settings.className = 'factory-settings';
    settings.append(loop);

    const queue = document.createElement('div');
    queue.className = 'queue-icons';
    if (sameActive && plan.active) {
      const active = plan.active;
      const progress = active.awaiting_output ? 'Output blocked'
        : `${Math.round(active.paid_matter)} / ${g.types.get(active.type_key)?.matter_cost} matter`;
      const row = this.productionRow({
        type: active.type_key, color, enabled, isSilo, shared, loop: !!active.loop_enabled,
        label: shared ? `Building ${active.type_key} ×${views.length}` : `Building ${active.type_key} · ${progress}`,
        repeatLabel: shared ? 'Repeat active on all' : `Loop active ${active.type_key}`,
        removeLabel: shared ? `Cancel active ${active.type_key} on all` : `Cancel active ${active.type_key}`,
        repeat: () => edit({kind: 'set_active_loop', enabled: !active.loop_enabled}),
        remove: () => edit({kind: 'cancel_active'}),
        active: true,
      });
      row.dataset.queueItem = `active:${JSON.stringify(active.item_id)}`;
      queue.append(row);
    } else if (!sameActive) {
      const mixed = document.createElement('span');
      mixed.className = 'muted';
      mixed.textContent = 'Active production differs.';
      queue.append(mixed);
    }
    if (samePending) {
      for (const batch of queueBatches(plan.pending)) {
        const index = batch.indices.at(-1)!;
        const label = `${batch.type} ×${batch.indices.length}`;
        const row = this.productionRow({
          type: batch.type, color, enabled, isSilo, shared, loop: batch.loop, label,
          repeatLabel: shared ? `Loop shared ${batch.type} batch` : `Loop queued ${batch.type}`,
          removeLabel: shared ? `Remove shared item ${index + 1}` : `Remove one queued ${batch.type}`,
          repeat: () => edit({kind: 'set_pending_loop', indices: batch.indices, enabled: !batch.loop}),
          remove: () => edit({kind: 'remove_pending', index}),
          active: false,
        });
        row.dataset.queueItem = `${batch.type}:${batch.indices[0]}`;
        if (!shared) row.title = label;
        queue.append(row);
      }
    }
    if (!samePending || !queue.childElementCount) {
      const empty = document.createElement('span');
      empty.className = 'muted';
      empty.textContent = !samePending ? 'Queues differ across factories.'
        : isSilo ? 'Production empty — build missiles below.' : 'Queue empty — add units.';
      queue.append(empty);
    }

    const recipes = document.createElement('div');
    recipes.className = 'recipe-icons';
    const available = shared
      ? g.producible().filter(key => views.every(v => g.types.get(v.type_key)?.production?.recipes.includes(key)))
      : type?.production?.recipes ?? [];
    for (const key of available) {
      const add = button('', event => edit({kind: 'append', items: Array<string>(event.shiftKey ? 5 : 1).fill(key)}));
      const cost = g.types.get(key)?.matter_cost;
      add.setAttribute('aria-label', `Queue ${key}`);
      add.title = shared ? `${key} · ${cost} matter each · Shift adds 5 to every factory`
        : `Queue ${key} · ${cost} matter · Shift-click adds 5`;
      add.append(unitIcon(key, color), isSilo ? ` ${key.replace('_', ' ')} +` : '+');
      add.disabled = !enabled;
      recipes.append(add);
    }
    const clear = button(shared ? 'Clear all waiting queues' : 'Clear waiting queue', () => edit({kind: 'clear_pending'}));
    clear.disabled = !enabled || plans.every(p => !p.pending.length);

    if (shared) {
      const summary = document.createElement('div');
      summary.className = 'factory-order';
      summary.textContent = `${samePending ? 'Aligned queues · ' : ''}New units: ${sameOrder ? orderLabel(plan.order) : 'Mixed'}`;
      card.append(queue, summary);
    } else {
      const summary = document.createElement('div');
      summary.className = 'factory-order';
      const order = plan.order.kind === 'idle' ? 'Idle at output — give a destination' : orderLabel(plan.order);
      summary.textContent = `New units: ${order}${plan.group ? ` · group ${plan.group.slot}` : ''}`;
      summary.hidden = isSilo;
      card.append(queue, summary);
      if (first.lifecycle !== 'complete') {
        const note = document.createElement('small');
        note.textContent = 'Queue starts as soon as construction finishes.';
        card.append(note);
      }
    }

    if (isSilo) {
      const key = idKey(first.id);
      const details = document.createElement('details');
      details.className = 'silo-production';
      details.open = this.siloProductionOpen.has(key);
      const label = document.createElement('summary');
      label.textContent = `Build missiles · ${plan.pending.length} waiting${plan.active ? ' · building ' + plan.active.type_key.replace('_', ' ') : ''}`;
      heading.after(settings);
      details.append(label, ...Array.from(card.childNodes), recipes, clear);
      card.append(details, siloPanel(g, first));
      details.ontoggle = () => {
        if (!details.isConnected) return;
        if (details.open) this.siloProductionOpen.add(key);
        else this.siloProductionOpen.delete(key);
      };
    } else {
      const controls = document.createElement('div');
      controls.className = 'factory-controls';
      controls.append(shared ? loop : settings, recipes, clear);
      card.append(controls);
    }
    return card;
  }

  private productionRow({
    type, color, enabled, isSilo, shared, loop, label, repeatLabel, removeLabel, repeat, remove, active,
  }: {
    type: string; color: string; enabled: boolean; isSilo: boolean; shared: boolean; loop: boolean; label: string;
    repeatLabel: string; removeLabel: string; repeat: () => void; remove: () => void; active: boolean;
  }): HTMLElement {
    const row = document.createElement('div');
    row.className = active ? 'queue-item active' : 'queue-item';
    row.dataset.loop = String(loop);
    const repeatButton = button(loop ? '↻' : '1×', repeat);
    repeatButton.setAttribute('aria-label', repeatLabel);
    repeatButton.setAttribute('aria-pressed', String(loop));
    if (!active && !shared) repeatButton.title = loop ? 'Repeats after completion' : 'Build once';
    const removeButton = button(active ? '×' : '−', remove);
    removeButton.setAttribute('aria-label', removeLabel);
    if (active) removeButton.title = shared ? 'Cancel this unit in all selected factories; spent matter is lost'
      : 'Cancel this unit; spent matter is lost';
    removeButton.onfocus = () => { this.focusDelete = () => removeButton.click(); };
    repeatButton.disabled = removeButton.disabled = !enabled;

    const content = [unitIcon(type, color), queueLabel(label)];
    if (isSilo) row.append(...content, repeatButton, removeButton);
    else row.append(repeatButton, removeButton, ...content);
    return row;
  }
}

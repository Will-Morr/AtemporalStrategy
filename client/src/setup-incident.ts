// Only fixed diagnostic categories and coordinates in the bundled script go to the server.
export type SetupStage = 'environment' | 'directory' | 'socket' | 'welcome' | 'lobby' | 'game';
type Operation = 'username' | 'top_controls' | 'set_attribute' | 'hidden' | 'value' | 'text_content' | 'closest';
type View = 'standalone' | 'game' | 'replay' | 'practice';

export class MissingSetupElement extends Error {
  constructor(readonly operation: Operation) {
    super('Missing a required setup element');
  }
}

function details(error: unknown) {
  const name = error instanceof Error ? error.name : '';
  const category = name === 'TypeError' ? 'type' : name === 'ReferenceError' ? 'reference'
    : name === 'RangeError' ? 'range' : name === 'SyntaxError' ? 'syntax' : name === 'Error' ? 'error' : 'other';
  const message = error instanceof Error ? error.message : '';
  const chromium = /^Cannot (?:read|set) propert(?:y|ies) of (null|undefined)(?: \((?:reading|setting) '([A-Za-z]+)'\))?/.exec(message);
  const firefox = /^can't access property "([A-Za-z]+)", .* is (null|undefined)$/.exec(message);
  const missing = chromium?.[1] ?? firefox?.[2];
  const property = chromium?.[2] ?? firefox?.[1];
  const operations: Record<string, Operation> = {
    setAttribute: 'set_attribute', hidden: 'hidden', value: 'value', textContent: 'text_content', closest: 'closest',
  };
  const operation = error instanceof MissingSetupElement ? error.operation
    : property && Object.hasOwn(operations, property) ? operations[property] : null;
  const reason = error instanceof MissingSetupElement ? 'missing_ui'
    : missing === 'null' ? 'null_access' : missing === 'undefined' ? 'undefined_access'
    : name === 'SyntaxError' && /JSON|Unexpected token/i.test(message) ? 'invalid_json' : 'unclassified';
  const stack = error instanceof Error ? error.stack ?? '' : '';
  const frames = stack.split('\n').flatMap(frame => {
    const a = /^(?:\s*at\s+(?:.*?[ (])?|[^@\s]*@)\s*(https?:\/\/[^\s)]+|\/[^\s)]+|app\.js):(\d+):(\d+)\)?$/.exec(frame);
    if (!a) return [];
    try {
      const url = new URL(a[1], location.href);
      return url.origin === location.origin
        && /^\/(?:games\/[\w-]+\/(?:replays\/[\w-]+\/)?)?app\.js$/.test(url.pathname)
        && !url.search && !url.hash
        ? [{line: Number(a[2]), column: Number(a[3])}] : [];
    } catch { return []; }
  }).slice(0, 5);
  return {category, reason, operation, frames};
}

export async function reportSetupError(
  error: unknown, stage: SetupStage, isPractice: boolean, round: number | null, revision: number | null,
): Promise<string | null> {
  const match = /^\/games\/([\w-]+)(?:\/replays\/([\w-]+))?\/?$/.exec(location.pathname);
  const view: View = isPractice ? 'practice' : match?.[2] ? 'replay' : match ? 'game' : 'standalone';
  const root = match ? `/api/games/${match[1]}${match[2] ? `/replays/${match[2]}` : ''}` : '/api';
  try {
    const response = await fetch(`${root}/client-incidents`, {
      method: 'POST',
      headers: {'Content-Type': 'application/json'},
      body: JSON.stringify({stage, view, ...details(error), round, revision}),
      referrerPolicy: 'no-referrer',
      signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) return null;
    const result = await response.json();
    return result.saved === true && typeof result.report_id === 'string'
      && /^client-[a-f0-9]{24}$/.test(result.report_id) ? result.report_id : null;
  } catch { return null; }
}

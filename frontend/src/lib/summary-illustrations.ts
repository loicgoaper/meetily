// Meetily perso: meeting screenshots inside the summary editor (BlockNote).
// A screenshot is an image block whose url is `meetily-screen://<file>`; the stored
// Markdown keeps that url and the editor resolves it to a data URL when displaying.

export const SCREEN_SCHEME = 'meetily-screen://';

export const screenUrl = (file: string) => `${SCREEN_SCHEME}${file}`;

export function screenFile(url: unknown): string | null {
  return typeof url === 'string' && url.startsWith(SCREEN_SCHEME) ? url.slice(SCREEN_SCHEME.length) : null;
}

/** Minimal shape of a BlockNote block (works for live blocks and stored summary_json). */
export interface AnyBlock {
  id?: string;
  type: string;
  props?: Record<string, any>;
  content?: any;
  children?: AnyBlock[];
}

export const isScreenImage = (block: AnyBlock) => block.type === 'image' && screenFile(block.props?.url) !== null;

function inlineText(content: any): string {
  if (!content) return '';
  if (Array.isArray(content)) {
    return content
      .map(item => {
        if (typeof item === 'string') return item;
        if (item?.type === 'text') return item.text ?? '';
        if (item?.type === 'link') return inlineText(item.content);
        return inlineText(item?.content);
      })
      .join('');
  }
  if (content.type === 'tableContent') {
    return (content.rows ?? [])
      .map((row: any) => (row.cells ?? []).map((cell: any) => inlineText(Array.isArray(cell) ? cell : cell?.content)).join(' | '))
      .join('\n');
  }
  return '';
}

/** Plain text of a block and its nested blocks. */
export function blockPlainText(block: AnyBlock, depth = 0): string {
  const own = inlineText(block.content);
  const children = (block.children ?? []).map(child => blockPlainText(child, depth + 1)).filter(Boolean);
  return [own, ...children.map(text => `${'  '.repeat(depth + 1)}${text}`)].filter(Boolean).join('\n');
}

function isBoldOnly(block: AnyBlock): boolean {
  const items = Array.isArray(block.content) ? block.content : [];
  return items.length > 0 && items.every((item: any) => item?.type === 'text' && (!item.text?.trim() || item.styles?.bold));
}

function kindOf(block: AnyBlock): string {
  switch (block.type) {
    case 'heading':
      return 'heading';
    case 'paragraph':
      return isBoldOnly(block) ? 'heading' : 'paragraph';
    case 'bulletListItem':
    case 'checkListItem':
      return 'bullet';
    case 'numberedListItem':
      return 'numbered';
    case 'table':
      return 'table';
    case 'quote':
      return 'quote';
    default:
      return 'other';
  }
}

export interface IllustrationBlockInput {
  kind: string;
  text: string;
}

/** Top-level blocks sent to the model (screenshots and empty paragraphs left out). */
export function collectIllustrationBlocks(blocks: AnyBlock[]): { inputs: IllustrationBlockInput[]; ids: string[]; kinds: string[] } {
  const inputs: IllustrationBlockInput[] = [];
  const ids: string[] = [];
  const kinds: string[] = [];
  for (const block of blocks) {
    if (isScreenImage(block) || !block.id) continue;
    const text = blockPlainText(block).trim();
    if (!text) continue;
    const kind = kindOf(block);
    inputs.push({ kind, text: text.slice(0, 600) });
    ids.push(block.id);
    kinds.push(kind);
  }
  return { inputs, ids, kinds };
}

export interface Illustration {
  block: number;
  file: string;
  caption: string;
  t: number;
}

interface IllustratableEditor {
  document: AnyBlock[];
  getBlock: (id: string) => AnyBlock | undefined;
  removeBlocks: (blocks: AnyBlock[]) => unknown;
  insertBlocks: (blocks: any[], reference: string, placement: 'before' | 'after') => unknown;
  transact?: (fn: () => void) => unknown;
}

/**
 * Replace the screenshots of the document by the model's picks (inserted after their
 * block, after the whole run for a numbered list). Returns how many were inserted.
 */
export function applyIllustrations(editor: IllustratableEditor, ids: string[], kinds: string[], picks: Illustration[]): number {
  let inserted = 0;
  const run = () => {
    const previous = editor.document.filter(isScreenImage);
    if (previous.length) editor.removeBlocks(previous);
    const ordered = [...picks].sort((a, b) => b.block - a.block);
    for (const pick of ordered) {
      let index = pick.block;
      while (kinds[index] === 'numbered' && kinds[index + 1] === 'numbered') index += 1;
      const reference = ids[index];
      if (!reference || !editor.getBlock(reference)) continue;
      editor.insertBlocks(
        [{ type: 'image', props: { url: screenUrl(pick.file), caption: pick.caption, name: pick.caption, previewWidth: 640 } }],
        reference,
        'after',
      );
      inserted += 1;
    }
  };
  if (editor.transact) editor.transact(run);
  else run();
  return inserted;
}

function mapBlocks<T extends AnyBlock>(blocks: T[], fn: (block: T) => T): T[] {
  return blocks.map(block => {
    const mapped = fn(block);
    return mapped.children?.length ? { ...mapped, children: mapBlocks(mapped.children as T[], fn) } : mapped;
  });
}

/** After parsing Markdown: the alt text (BlockNote `name`) becomes the visible caption. */
export function withScreenCaptions<T extends AnyBlock>(blocks: T[]): T[] {
  return mapBlocks(blocks, block =>
    isScreenImage(block) && !block.props?.caption && block.props?.name
      ? ({ ...block, props: { ...block.props, caption: block.props.name } } as T)
      : block,
  );
}

/**
 * Copy of the blocks for Markdown export: the caption moves to `name` so BlockNote
 * writes `![caption](url)` instead of a <figure> whose caption would come back as an
 * extra paragraph on every save.
 */
export function blocksForMarkdown<T extends AnyBlock>(blocks: T[]): T[] {
  return mapBlocks(blocks, block =>
    isScreenImage(block)
      ? ({ ...block, props: { ...block.props, name: block.props?.caption || block.props?.name || '', caption: '' } } as T)
      : block,
  );
}

const IMAGE_LINE = /^\s*!\[((?:\\.|[^\]\\])*)\]\(<?meetily-screen:\/\/[A-Za-z0-9_.-]+>?(?:\s+"[^"]*")?\)\s*$/;

/** Markdown for the clipboard: each screenshot becomes "*(label : caption)*". */
export function screenImagesToText(markdown: string, label: string): string {
  return markdown
    .split('\n')
    .flatMap(line => {
      const match = line.match(IMAGE_LINE);
      if (!match) return [line];
      const caption = match[1].replace(/\\(.)/g, '$1').trim();
      return caption && caption !== 'BlockNote image' ? [`*(${label} : ${caption})*`] : [];
    })
    .join('\n');
}

const PLACEHOLDER =
  'data:image/svg+xml;utf8,' +
  encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="640" height="80"><rect width="100%" height="100%" fill="#f3f4f6"/><text x="50%" y="55%" text-anchor="middle" font-family="sans-serif" font-size="14" fill="#9ca3af">Capture indisponible</text></svg>');

/**
 * `resolveFileUrl` for BlockNote: screenshot urls become data URLs (fetched through the
 * given loader, last 30 kept in memory); every other url is left as is.
 */
export function createScreenResolver(
  getMeetingId: () => string | undefined,
  load: (meetingId: string, file: string) => Promise<string>,
): (url: string) => Promise<string> {
  const cache = new Map<string, Promise<string>>();
  return async (url: string) => {
    const file = screenFile(url);
    const meetingId = getMeetingId();
    if (!file || !meetingId) return url;
    const key = `${meetingId}/${file}`;
    let pending = cache.get(key);
    if (!pending) {
      pending = load(meetingId, file).catch(() => PLACEHOLDER);
      cache.set(key, pending);
      if (cache.size > 30) cache.delete(cache.keys().next().value as string);
    }
    return pending;
  };
}

import { describe, expect, test } from 'bun:test';
import {
  applyIllustrations,
  blocksForMarkdown,
  collectIllustrationBlocks,
  createScreenResolver,
  screenImagesToText,
  withScreenCaptions,
  type AnyBlock,
} from '../../src/lib/summary-illustrations';

const text = (value: string, bold = false) => ({ type: 'text', text: value, styles: bold ? { bold: true } : {} });

function documentBlocks(): AnyBlock[] {
  return [
    { id: 'h', type: 'paragraph', content: [text('Points abordés', true)] },
    { id: 'p', type: 'paragraph', content: [text('Le concept 1 est retenu.')] },
    { id: 'empty', type: 'paragraph', content: [] },
    { id: 'b1', type: 'bulletListItem', content: [text('Stand de 10 × 10 pieds')], children: [{ id: 'b1c', type: 'bulletListItem', content: [text('comptoir')] }] },
    { id: 'old', type: 'image', props: { url: 'meetily-screen://0001_00-01.jpg', caption: 'ancienne' } },
    { id: 'n1', type: 'numberedListItem', content: [text('Commander')] },
    { id: 'n2', type: 'numberedListItem', content: [text('Valider')] },
  ];
}

class FakeEditor {
  document: AnyBlock[] = documentBlocks();
  getBlock = (id: string) => this.document.find(b => b.id === id);
  removeBlocks = (blocks: AnyBlock[]) => {
    this.document = this.document.filter(b => !blocks.includes(b));
  };
  insertBlocks = (blocks: AnyBlock[], reference: string) => {
    const index = this.document.findIndex(b => b.id === reference);
    this.document.splice(index + 1, 0, ...blocks.map((b, i) => ({ ...b, id: `new-${reference}-${i}` })));
  };
}

describe('summary illustrations', () => {
  test('blocks sent to the model skip screenshots and empty paragraphs', () => {
    const { inputs, ids, kinds } = collectIllustrationBlocks(documentBlocks());
    expect(ids).toEqual(['h', 'p', 'b1', 'n1', 'n2']);
    expect(kinds).toEqual(['heading', 'paragraph', 'bullet', 'numbered', 'numbered']);
    expect(inputs[2].text).toBe('Stand de 10 × 10 pieds\n  comptoir');
  });

  test('picks replace previous screenshots, after the whole numbered run', () => {
    const editor = new FakeEditor();
    const { ids, kinds } = collectIllustrationBlocks(editor.document);
    const count = applyIllustrations(editor, ids, kinds, [
      { block: 2, file: '0029_22-56.webp', caption: 'Formulaire du stand', t: 1376 },
      { block: 3, file: '0084_49-51.webp', caption: 'Roll-up', t: 2991 },
      { block: 9, file: 'x.webp', caption: 'hors plage', t: 1 },
    ]);
    expect(count).toBe(2);
    const order = editor.document.map(b => (b.type === 'image' ? `img:${b.props?.url}` : b.id));
    expect(order).toEqual([
      'h', 'p', 'empty', 'b1', 'img:meetily-screen://0029_22-56.webp', 'n1', 'n2', 'img:meetily-screen://0084_49-51.webp',
    ]);
    expect(editor.document[4].props).toMatchObject({ caption: 'Formulaire du stand', name: 'Formulaire du stand', previewWidth: 640 });
  });

  test('captions survive the Markdown round trip without duplication', () => {
    const parsed: AnyBlock[] = [{ type: 'image', props: { url: 'meetily-screen://a.webp', name: 'Stand', caption: '' } }];
    const shown = withScreenCaptions(parsed);
    expect(shown[0].props?.caption).toBe('Stand');
    const exported = blocksForMarkdown(shown);
    expect(exported[0].props).toMatchObject({ name: 'Stand', caption: '' });
    expect(shown[0].props?.caption).toBe('Stand');
    // Other images are untouched
    const other: AnyBlock[] = [{ type: 'image', props: { url: 'https://x/y.png', caption: 'c', name: '' } }];
    expect(blocksForMarkdown(other)[0]).toBe(other[0]);
  });

  test('copied Markdown shows captions instead of screenshot links', () => {
    const md = 'Texte\n\n![Formulaire \\[stand\\]](meetily-screen://0029_22-56.webp)\n\n![BlockNote image](meetily-screen://a.jpg)\nFin';
    expect(screenImagesToText(md, "Capture d'écran")).toBe("Texte\n\n*(Capture d'écran : Formulaire [stand])*\n\nFin");
  });

  test('screenshot urls resolve through the loader once', async () => {
    let calls = 0;
    const resolve = createScreenResolver(
      () => 'm1',
      async (meetingId, file) => {
        calls += 1;
        return `data:image/webp;base64,${meetingId}-${file}`;
      },
    );
    expect(await resolve('meetily-screen://a.webp')).toBe('data:image/webp;base64,m1-a.webp');
    await resolve('meetily-screen://a.webp');
    expect(calls).toBe(1);
    expect(await resolve('https://example.com/x.png')).toBe('https://example.com/x.png');
    const failing = createScreenResolver(() => 'm1', async () => {
      throw new Error('missing');
    });
    expect(await failing('meetily-screen://b.webp')).toStartWith('data:image/svg+xml');
  });
});

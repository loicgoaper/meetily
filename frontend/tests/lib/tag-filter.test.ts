import { describe, expect, test } from 'bun:test';
import { fitInLines, foldText } from '../../src/lib/tag-filter';

describe('sidebar tag filter layout', () => {
  test('keeps every chip when they fit on two lines', () => {
    // 100px wide, 4px gap: [40 40] [40 40]
    expect(fitInLines([40, 40, 40, 40], 30, 100, 4, 2)).toBe(4);
  });

  test('leaves room for the "+N" chip on the second line', () => {
    // 6 chips of 40px need 3 lines; keeping 3 leaves [40 40] [40 +N(30)]
    expect(fitInLines([40, 40, 40, 40, 40, 40], 30, 100, 4, 2)).toBe(3);
  });

  test('a chip as wide as the line takes a line of its own', () => {
    expect(fitInLines([100, 100, 100], 30, 100, 4, 2)).toBe(1);
    expect(fitInLines([], 30, 100, 4, 2)).toBe(0);
  });

  test('search ignores accents and case', () => {
    expect(foldText('IA Québec')).toBe('ia quebec');
    expect(foldText('Sécurité').includes(foldText('securite'))).toBe(true);
  });
});

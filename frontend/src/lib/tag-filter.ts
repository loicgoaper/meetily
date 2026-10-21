// Meetily perso: helpers of the sidebar tag filter.

/**
 * How many chips (in order) fit on `maxLines` lines of `available` px, like a CSS
 * flex-wrap row with `gap`, keeping room for a trailing "+N" chip of `plusWidth` px
 * whenever some chips are left out.
 */
export function fitInLines(widths: number[], plusWidth: number, available: number, gap: number, maxLines: number): number {
  const fits = (count: number) => {
    const items = widths.slice(0, count);
    if (count < widths.length) items.push(plusWidth);
    let lines = 1;
    let x = 0;
    for (const width of items) {
      const next = x === 0 ? width : x + gap + width;
      if (next <= available) {
        x = next;
      } else {
        lines += 1;
        x = width;
        if (lines > maxLines) return false;
      }
    }
    return true;
  };
  for (let count = widths.length; count > 0; count--) {
    if (fits(count)) return count;
  }
  return 0;
}

/** Lower case without accents, for searching tags. */
export function foldText(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

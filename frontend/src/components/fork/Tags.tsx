'use client';

import { useTranslation } from 'react-i18next';
import { TAG_COLOR_CLASSES, TAG_COLORS, type Tag } from '@/services/forkService';

export function TagChip({ tag, small }: { tag: Tag; small?: boolean }) {
  const classes = TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default;
  return (
    <span className={`inline-flex items-center rounded border ${small ? 'px-1 text-[10px]' : 'px-1.5 py-0.5 text-xs'} font-medium ${classes.chip}`}>
      {tag.name}
    </span>
  );
}

/** Small coloured dots (sidebar meeting rows). */
export function TagDots({ tags }: { tags: Tag[] }) {
  if (tags.length === 0) return null;
  return (
    <span className="ml-1 inline-flex shrink-0 items-center gap-0.5" title={tags.map(t => t.name).join(', ')}>
      {tags.slice(0, 4).map(tag => (
        <span key={tag.id} className={`h-2 w-2 rounded-full ${(TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default).dot}`} />
      ))}
    </span>
  );
}

export function ColorPicker({ value, onChange }: { value: string; onChange: (color: string) => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-1">
      {TAG_COLORS.map(color => (
        <button
          key={color}
          type="button"
          title={t(`fork.tags.colors.${color}`)}
          onClick={() => onChange(color)}
          className={`h-5 w-5 rounded-full border-2 ${TAG_COLOR_CLASSES[color].dot} ${value === color ? 'border-gray-900' : 'border-white'}`}
        />
      ))}
    </div>
  );
}

import { Monitor, Moon, Sun } from 'lucide-react';
import { type ComponentType } from 'react';

import { useTranslation } from '@/shared/translation';
import { cnTw } from '@/shared/utils';
import { type ThemePreference, useSetThemePreference, useThemePreference } from '@/domains/application';

type Segment = { value: ThemePreference; labelKey: string; icon: ComponentType<{ className?: string }> };

const SEGMENTS: Segment[] = [
  { value: 'system', labelKey: 'feature.themeToggle.device', icon: Monitor },
  { value: 'light', labelKey: 'feature.themeToggle.day', icon: Sun },
  { value: 'dark', labelKey: 'feature.themeToggle.night', icon: Moon },
];

export const ColorModeControl = () => {
  const { t } = useTranslation();
  const preference = useThemePreference();
  const { run: setPreference } = useSetThemePreference();

  return (
    <div role="radiogroup" className="inline-flex items-center gap-0.5 rounded-full bg-bg-surface-nested p-0.5">
      {SEGMENTS.map(({ value, labelKey, icon: Icon }) => {
        const active = preference === value;
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={active}
            className={cnTw(
              'inline-flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm leading-5 font-medium transition-colors',
              'focus-visible:ring-2 focus-visible:ring-focus-ring/50 focus-visible:outline-none',
              active
                ? 'bg-bg-surface-container text-fg-primary shadow-[0_1px_2px_0_var(--shadow-soft)]'
                : 'text-fg-secondary hover:text-fg-primary',
            )}
            onClick={() => setPreference(value)}
          >
            <Icon className="size-4 shrink-0" />
            {t(labelKey)}
          </button>
        );
      })}
    </div>
  );
};

import { SettingsList } from '@/shared/components';
import { useTranslation } from '@/shared/translation';

import { ColorModeControl } from './ColorModeControl';
import { ThemePicker } from './ThemePicker';
import { ThemePreview } from './ThemePreview';

const SectionLabel = ({ children }: { children: string }) => (
  <h2 className="text-sm leading-5 font-semibold text-fg-primary">{children}</h2>
);

export const ThemeSettings = () => {
  const { t } = useTranslation();

  return (
    <SettingsList title={t('feature.themeToggle.title')}>
      <div className="flex flex-col gap-6">
        <ThemePreview />

        <div className="flex flex-col gap-2">
          <SectionLabel>{t('feature.themeToggle.colorMode')}</SectionLabel>
          <ColorModeControl />
        </div>

        <div className="flex flex-col gap-3">
          <SectionLabel>{t('feature.themeToggle.appearance')}</SectionLabel>
          <ThemePicker />
        </div>
      </div>
    </SettingsList>
  );
};

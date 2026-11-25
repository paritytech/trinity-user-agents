import { ListItem, RadioGroup } from '@novasamatech/tr-ui';

import { SettingsList } from '@/shared/components';
import { SUPPORTED_LOCALES, saveLocale, useLocalePreference, useTranslation } from '@/shared/translation';

export const LanguageSettings = () => {
  const { t } = useTranslation();
  const locale = useLocalePreference();

  const handleChange = (value: string) => {
    const selected = SUPPORTED_LOCALES.find(item => item.id === value);
    if (!selected) return;

    saveLocale(selected.id);
  };

  return (
    <SettingsList title={t('feature.languageSettings.title')}>
      <RadioGroup value={locale} onValueChange={handleChange}>
        {SUPPORTED_LOCALES.map(({ id, nativeName }) => (
          <ListItem
            key={id}
            variant="radio"
            value={id}
            title={t(`feature.languageSettings.locale.${id}`)}
            description={nativeName}
          />
        ))}
      </RadioGroup>
    </SettingsList>
  );
};

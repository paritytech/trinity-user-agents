import { createFileRoute } from '@tanstack/react-router';

import { LanguageSettings } from '@/features/language-settings';

export const Route = createFileRoute('/_app/settings/language')({
  component: LanguageSettings,
});

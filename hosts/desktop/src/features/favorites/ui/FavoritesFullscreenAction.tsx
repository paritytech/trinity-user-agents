import { Maximize2 } from 'lucide-react';

import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { widgetTopbarActionButtonClass, widgetTopbarActionVisibilityClass } from '@/features/dashboard';
import { useOpenFavorites } from '../hooks/useOpenFavorites';

// Toolbar action injected into the Favorites folder card's topbar — opens the
// fullscreen SPA. Mirrors the chat widget's ChatFullscreenAction.
export const FavoritesFullscreenAction = () => {
  const { t } = useTranslation();
  const openFavorites = useOpenFavorites();

  return (
    <span className={widgetTopbarActionVisibilityClass}>
      <button
        type="button"
        data-testid={TEST_IDS.favoritesWidgetFullscreenButton}
        aria-label={t('feature.favorites.openFullscreenAria')}
        className={widgetTopbarActionButtonClass}
        onClick={openFavorites}
        onMouseDown={event => event.stopPropagation()}
      >
        <Maximize2 className="size-4" aria-hidden />
      </button>
    </span>
  );
};

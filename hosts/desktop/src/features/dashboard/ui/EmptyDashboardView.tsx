import { Button } from '@novasamatech/tr-ui';

import AddToDashboardIcon from '@/shared/assets/images/add-to-dashboard.svg?jsx';
import LogoIcon from '@/shared/assets/images/logo.svg?jsx';
import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';

type EmptyDashboardViewProps = {
  onAddWidget: () => void;
};

export const EmptyDashboardView = ({ onAddWidget }: EmptyDashboardViewProps) => {
  const { t } = useTranslation();
  const addWidgetLabel = t('feature.dashboard.addWidget.title');

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-bg-surface-main px-6">
      <section
        data-testid={TEST_IDS.dashboardEmptyState}
        className="flex w-full max-w-89 flex-col items-center rounded-xl p-10 text-center"
      >
        <LogoIcon className="mb-2 h-auto w-70 text-fg-primary" aria-hidden />

        <div className="mb-6 flex w-full flex-col items-center gap-1">
          <p className="text-body-large-emphasized text-fg-primary">{t('feature.dashboard.empty.title')}</p>
          <p className="text-body-medium text-fg-secondary">{t('feature.dashboard.empty.description')}</p>
        </div>

        <div data-testid={TEST_IDS.dashboardEmptyStateAddWidget}>
          <Button type="button" variant="default" size="sm" aria-label={addWidgetLabel} onClick={onAddWidget}>
            <AddToDashboardIcon className="h-2.75 w-[10.7px] shrink-0 -scale-y-100 text-fg-primary-inverted" aria-hidden />
            <span>{addWidgetLabel}</span>
          </Button>
        </div>
      </section>
    </div>
  );
};

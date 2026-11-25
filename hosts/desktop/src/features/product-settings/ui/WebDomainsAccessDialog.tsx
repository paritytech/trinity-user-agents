import { useTranslation } from '@/shared/translation';
import { type PermissionStatus, permissionsService, useSetPermissionStatus, useWatchGrantedPatterns } from '@/domains/product';

import { PermissionEntriesDialog } from './PermissionEntriesDialog';

type Props = {
  productId: string;
  productName: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export const WebDomainsAccessDialog = ({ productId, productName, open, onOpenChange }: Props) => {
  const { t } = useTranslation();
  const { data: patterns } = useWatchGrantedPatterns(productId);
  const setStatus = useSetPermissionStatus();

  const entries = patterns.map(({ pattern, status }) => ({ key: pattern, label: pattern, status }));

  const handleStatusChange = (pattern: string, status: PermissionStatus) => {
    const request = permissionsService.toAuthorizationRequest('ExternalRequest', { pattern });
    if (!request) return;

    setStatus.run({ productId, request, status });
  };

  return (
    <PermissionEntriesDialog
      open={open}
      title={t('feature.productSettings.webDomains.dialogTitle', { productName })}
      entries={entries}
      onOpenChange={onOpenChange}
      onStatusChange={handleStatusChange}
    />
  );
};

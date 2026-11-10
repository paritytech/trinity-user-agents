import { RadioGroup } from '@novasamatech/tr-ui';

import { cnTw } from '@/shared/utils';

type Props = {
  value: string;
  title: string;
  description: string;
};

export const ConnectionPreferenceItem = ({ value, title, description }: Props) => {
  return (
    <label
      className={cnTw(
        'flex cursor-pointer items-center gap-3 rounded-xl px-3 py-1',
        'hover:bg-bg-selection-container-hover has-[:focus-visible]:bg-bg-selection-container-hover',
      )}
    >
      <RadioGroup.Item value={value} />
      <div className="flex min-w-0 flex-col">
        <span className="truncate text-sm leading-5 text-fg-primary">{title}</span>
        <span className="truncate text-xs leading-4 font-medium text-fg-tertiary">{description}</span>
      </div>
    </label>
  );
};

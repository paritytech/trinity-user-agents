import { type PropsWithChildren, type ReactNode } from 'react';

type Props = PropsWithChildren<{
  title?: ReactNode;
}>;

export const SettingsSection = ({ title, children }: Props) => {
  return (
    <div className="overflow-hidden rounded-lg border border-stroke-primary bg-bg-surface-nested">
      {title && (
        <div className="border-b border-stroke-primary px-4 py-3">
          <h2 className="text-sm font-semibold text-fg-primary">{title}</h2>
        </div>
      )}
      <div className="p-4">{children}</div>
    </div>
  );
};

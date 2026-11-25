import { type AddWidgetContributedEntry, type AddableDashboardCard } from '../../di';

export type AddWidgetSidebarEntry =
  | { source: 'native'; id: string; card: AddableDashboardCard }
  | { source: 'contributed'; id: string; entry: AddWidgetContributedEntry };

export type AddWidgetModalCardCopy = {
  title: string;
  description: string;
};

export { HOST_AUTH_PRODUCT_ID } from './core-auth/constants';
export type { ResolvedTheme, Theme, ThemeName, ThemePreference } from './theme/types';
export { THEME_NAMES } from './theme/constants';
export { themeService } from './theme/service';
export { saveThemeName, saveThemePreference, themeResource } from './theme/resource';
export { themeUseCase } from './$usecase/theme';
export {
  useSetThemeName,
  useSetThemePreference,
  useTheme,
  useThemeName,
  useThemePreference,
  useThemeVariant,
} from './theme/hooks';
export { commandsService } from './commands/service';
export { useSubmitError } from './statement-store/hooks';
export type { Command } from './commands/types';
export { failActivePeopleChain, lazyClient, setActivePeopleChain, statementStoreAdapter } from './statement-store/service';
export { type Environment, type EnvironmentId } from './environment/types';
export { SETTINGS_STORAGE_KEY } from './environment/constants';
export { environmentService } from './environment/service';
export { environmentUseCase } from './$usecase/environment';
export { useActiveEnvironment, useEnvironment } from './$usecase/environment.hooks';

export { web3SummitGateService } from './web3summit-gate/service';
export { type Web3SummitGateMode, web3SummitGateModeSchema } from './web3summit-gate/schemas';
export { onUserLoggedOutSideEffect, sessionUseCase } from './$usecase/session';

export type {
  ContentCardPayload,
  DashboardCard,
  DashboardCardLayoutRules,
  DashboardCardPayload,
  DashboardLayout,
  FolderCardPayload,
  MainDashboardLayoutSnapshot,
  WidgetSizeHints,
  WidgetSizeIconVariant,
  WidgetSizeKey,
} from './dashboard-layout/types';
export {
  ALLOWED_WIDGET_HEIGHTS,
  DEFAULT_DASHBOARD_WIDGET_PRODUCT_LABEL,
  DEFAULT_RESIZE_HANDLES,
  FAVORITES_FOLDER_ID,
  FAVORITES_GRID_COLS,
  FOLDER_DEFAULT_HEIGHT,
  FOLDER_MIN_HEIGHT,
  MAX_GRID_ROWS,
  MAX_WIDGET_HEIGHT,
  MAX_WIDGET_WIDTH,
  WIDGET_VARIANT_GRID_SIZE,
} from './dashboard-layout/constants';
export { dashboardLayoutService } from './dashboard-layout/service';
export {
  useDashboardLayouts,
  useDashboardProductIds,
  useFavoriteProductIds,
  useSetMainActivePage,
} from './dashboard-layout/hooks';

export { cardsUseCase } from './$usecase/cards';
export { foldersUseCase } from './$usecase/folders';
export { useAddCard, useRemoveCard, useResizeCard } from './$usecase/cards.hooks';
export { useAddToFavorites, useRemoveFolder, useRemoveItemFromFolder } from './$usecase/folders.hooks';
export { mainDashboardLayoutResource } from './dashboard-layout/resource';
export { environmentResource } from './environment/resource';

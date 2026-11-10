import { useAction, useRead } from '@/shared/hooks';
import {
  type AggregatedPermission,
  type GrantedAccountAccess,
  type GrantedPattern,
  type ProductPermissionEntry,
} from '../permissions/types';

import { permissionsUseCase } from './permissions';

// Typed empties: `defaultValue: []` alone infers `never[]`, and an `as` assertion is
// not allowed in production code.
const NO_ENTRIES: ProductPermissionEntry[] = [];
const NO_PATTERNS: GrantedPattern[] = [];
const NO_ACCOUNT_ACCESS: GrantedAccountAccess[] = [];
const NO_AGGREGATED: AggregatedPermission[] = [];

export const useWatchProductPermissions = (productId: Nullable<string>) => {
  return useRead((params: { productId: string }) => permissionsUseCase.watchProductPermissions(params), {
    params: productId ? { productId } : null,
    defaultValue: NO_ENTRIES,
  });
};

export const useWatchGrantedPatterns = (productId: Nullable<string>) => {
  return useRead((params: { productId: string }) => permissionsUseCase.watchGrantedPatterns(params), {
    params: productId ? { productId } : null,
    defaultValue: NO_PATTERNS,
  });
};

export const useWatchGrantedAccountAccess = (productId: Nullable<string>) => {
  return useRead((params: { productId: string }) => permissionsUseCase.watchGrantedAccountAccess(params), {
    params: productId ? { productId } : null,
    defaultValue: NO_ACCOUNT_ACCESS,
  });
};

export const useWatchAggregatedPermissions = () => {
  return useRead(() => permissionsUseCase.watchAggregatedPermissions(), {
    params: {},
    defaultValue: NO_AGGREGATED,
  });
};

export const useSetPermissionStatus = () => {
  return useAction(permissionsUseCase.setPermissionStatus);
};

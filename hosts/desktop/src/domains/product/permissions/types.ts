import { type HostDevicePermissionRequest } from '@parity/truapi';
import { type PermissionAuthorizationRequest, type PermissionAuthorizationStatus } from '@parity/truapi-host';

import { type ExecutableKind } from '../product/manifest/constants';

import { type PermissionId } from './constants';

/**
 * The device permissions the core can ask for. Aliased rather than restated: the
 * core owns the set, and a host that drifted from it would store decisions the
 * core never asks about.
 */
export type DevicePermissionType = HostDevicePermissionRequest;

/**
 * What the OS currently says about a device capability, as the main process reports
 * it. `'not-applicable'` covers every platform and permission the OS does not gate.
 */
export type OsDevicePermissionStatus = 'granted' | 'denied' | 'not-determined' | 'not-applicable';

export type PermissionStatus = 'ask' | 'granted' | 'denied';

// One product's standing on a single permission, used by the cross-product
// aggregation.
export type AppPermissionEntry = {
  productId: string;
  // Roll-up across modalities: 'granted' if any granted, else 'denied' if any denied, else 'ask'.
  status: PermissionStatus;
};

// A permission rolled up across every product that has touched it.
export type AggregatedPermission = {
  id: string;
  grantedCount: number;
  apps: AppPermissionEntry[];
};

export type RemotePermissionIpcRequest = {
  productId: string;
  executable: ExecutableKind;
  request: { tag: 'Remote'; url: string } | { tag: 'ChainSubmit' };
};

/**
 * The core's permission API, injected at bootstrap.
 *
 * The runtime handle lives in an aggregate and a domain may not import one, so this
 * arrives through `bootstrapProduct` rather than being reached for. It is deliberately
 * narrower than the runtime: three calls, no lifecycle.
 */
export type PermissionsAdapter = {
  getStatus(productId: string, request: PermissionAuthorizationRequest): Promise<PermissionAuthorizationStatus>;
  getStatuses(productId: string, requests: PermissionAuthorizationRequest[]): Promise<PermissionAuthorizationStatus[]>;
  setStatus(productId: string, request: PermissionAuthorizationRequest, status: PermissionAuthorizationStatus): Promise<void>;
};

/** One catalogue row of a product's settings page. */
export type ProductPermissionEntry = { permissionId: PermissionId; status: PermissionStatus };

/** One granted (or denied) remote domain, as the web-domains dialog renders it. */
export type GrantedPattern = { pattern: string; status: PermissionStatus };

/** One product whose account context this product may access. */
export type GrantedAccountAccess = { targetProductId: string; status: PermissionStatus };

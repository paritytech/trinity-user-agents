import { describe, expect, it } from 'vitest';

import { permissionsService } from './service';

// Every settings id the host persists as a core device permission.
const STORED_DEVICE_SETTINGS_IDS = [
  'Microphone',
  'Camera',
  'Bluetooth',
  'Location',
  'Notifications',
  'Clipboard',
  'OpenExternalUrl',
  'Biometrics',
  'Motion',
] as const;

describe('device permission settings mapping', () => {
  it('maps app-only device permissions from settings id to stored name', () => {
    expect(permissionsService.getDevicePermissionName('Notifications')).toBe('Notifications');
    expect(permissionsService.getDevicePermissionName('Clipboard')).toBe('Clipboard');
    expect(permissionsService.getDevicePermissionName('Biometrics')).toBe('Biometrics');
  });

  it('maps OpenExternalUrl settings id to OpenUrl device name', () => {
    expect(permissionsService.getDevicePermissionName('OpenExternalUrl')).toBe('OpenUrl');
    expect(permissionsService.getSettingsPermissionId('OpenUrl')).toBe('OpenExternalUrl');
  });

  it('round-trips settings id ↔ device name for every stored device permission', () => {
    for (const settingsId of STORED_DEVICE_SETTINGS_IDS) {
      const deviceName = permissionsService.getDevicePermissionName(settingsId);
      expect(deviceName).not.toBeNull();
      expect(permissionsService.getSettingsPermissionId(deviceName!)).toBe(settingsId);
    }
  });

  it('returns null for remote-only permission ids', () => {
    expect(permissionsService.getDevicePermissionName('ChainSubmit')).toBeNull();
    expect(permissionsService.getDevicePermissionName('WebRtc')).toBeNull();
  });

  it('returns null for Files (UI-only, not in host-api DevicePermission)', () => {
    expect(permissionsService.getDevicePermissionName('Files')).toBeNull();
    expect(permissionsService.isStoredAsDevicePermission('Files')).toBe(false);
  });

  it('returns device name unchanged for unmapped host-api device permissions', () => {
    expect(permissionsService.getSettingsPermissionId('NFC')).toBe('NFC');
  });

  it('detects permissions persisted as device permissions', () => {
    expect(permissionsService.isStoredAsDevicePermission('Notifications')).toBe(true);
    expect(permissionsService.isStoredAsDevicePermission('OpenExternalUrl')).toBe(true);
    expect(permissionsService.isStoredAsDevicePermission('ChainSubmit')).toBe(false);
  });
});

describe('core mapping', () => {
  it('maps device ids, including the OpenUrl alias', () => {
    expect(permissionsService.toAuthorizationRequest('Camera')).toEqual({ tag: 'Device', value: 'Camera' });
    expect(permissionsService.toAuthorizationRequest('OpenExternalUrl')).toEqual({ tag: 'Device', value: 'OpenUrl' });
  });

  it('maps the identity disclosure alias', () => {
    expect(permissionsService.toAuthorizationRequest('UserIdentity')).toEqual({ tag: 'IdentityDisclosure' });
  });

  it('maps payload-free remote kinds', () => {
    expect(permissionsService.toAuthorizationRequest('ChainSubmit')).toEqual({
      tag: 'Remote',
      value: { permission: { tag: 'ChainSubmit' } },
    });
  });

  it('maps Motion and BalanceAccess', () => {
    expect(permissionsService.toAuthorizationRequest('Motion')).toEqual({ tag: 'Device', value: 'Motion' });
    expect(permissionsService.toAuthorizationRequest('BalanceAccess')).toEqual({
      tag: 'Remote',
      value: { permission: { tag: 'BalanceAccess' } },
    });
  });

  it('lists Motion and BalanceAccess in the catalogue', () => {
    const ids = permissionsService.catalogueRequests().map(r => permissionsService.fromAuthorizationRequest(r));

    expect(ids).toContain('Motion');
    expect(ids).toContain('BalanceAccess');
  });

  it('carries the pattern for an external request', () => {
    expect(permissionsService.toAuthorizationRequest('ExternalRequest', { pattern: 'example.com' })).toEqual({
      tag: 'Remote',
      value: { permission: { tag: 'Remote', value: { domains: ['example.com'] } } },
    });
  });

  it('returns null rather than guessing when the option it needs is absent', () => {
    expect(permissionsService.toAuthorizationRequest('ExternalRequest')).toBeNull();
  });

  it('keys account access by the target product', () => {
    expect(permissionsService.toAccountAccessRequest('other.dot')).toEqual({
      tag: 'AccountAccess',
      value: { targetProductId: 'other.dot' },
    });
  });

  it('round-trips every catalogue request back to an id', () => {
    for (const request of permissionsService.catalogueRequests()) {
      expect(permissionsService.fromAuthorizationRequest(request)).not.toBeNull();
    }
  });

  it('keeps unbounded kinds out of the catalogue', () => {
    const ids = permissionsService.catalogueRequests().map(r => permissionsService.fromAuthorizationRequest(r));

    expect(ids).not.toContain('ExternalRequest');
  });

  it('maps statuses both ways', () => {
    expect(permissionsService.toAuthorizationStatus('granted')).toBe('Authorized');
    expect(permissionsService.toAuthorizationStatus('denied')).toBe('Denied');
    expect(permissionsService.toAuthorizationStatus('ask')).toBe('NotDetermined');
    expect(permissionsService.fromAuthorizationStatus('Authorized')).toBe('granted');
    expect(permissionsService.fromAuthorizationStatus('Denied')).toBe('denied');
    expect(permissionsService.fromAuthorizationStatus('NotDetermined')).toBe('ask');
  });
});

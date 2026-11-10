// Canonical permission identifiers — the settings/metadata vocabulary. The
// presentational metadata (icons, labels) keyed by these ids lives in the UI
// layer (@/widgets/Permission); the domain owns only the identity taxonomy.
export const PERMISSION_IDS = [
  'Clipboard',
  'Microphone',
  'Camera',
  'Notifications',
  'Bluetooth',
  'Location',
  'Biometrics',
  'Motion',
  'OpenExternalUrl',
  'ExternalRequest',
  'WebRtc',
  'ChainSubmit',
  'PreimageSubmit',
  'StatementSubmit',
  'BalanceAccess',
  'UserIdentity',
] as const;

export type PermissionId = (typeof PERMISSION_IDS)[number];

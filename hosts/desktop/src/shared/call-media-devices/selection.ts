import { type CallDeviceKind, type CallDeviceSelection } from './types';

const STORAGE_KEY = 'call:device-selection';

const DEFAULT_SELECTION: CallDeviceSelection = { cameraId: null, microphoneId: null, outputId: null };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readString(value: unknown): Nullable<string> {
  return typeof value === 'string' ? value : null;
}

// Load the persisted device selection. Tolerant: any missing/corrupt state
// resolves to "system default" (null) for each kind rather than throwing.
export function loadDeviceSelection(): CallDeviceSelection {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) return { ...DEFAULT_SELECTION };
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed)) return { ...DEFAULT_SELECTION };
    return {
      cameraId: readString(parsed['cameraId']),
      microphoneId: readString(parsed['microphoneId']),
      outputId: readString(parsed['outputId']),
    };
  } catch {
    return { ...DEFAULT_SELECTION };
  }
}

export function saveDeviceSelection(selection: CallDeviceSelection): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(selection));
}

export function deviceIdForKind(selection: CallDeviceSelection, kind: CallDeviceKind): Nullable<string> {
  if (kind === 'camera') return selection.cameraId;
  if (kind === 'microphone') return selection.microphoneId;
  return selection.outputId;
}

export function withDeviceForKind(
  selection: CallDeviceSelection,
  kind: CallDeviceKind,
  deviceId: Nullable<string>,
): CallDeviceSelection {
  if (kind === 'camera') return { ...selection, cameraId: deviceId };
  if (kind === 'microphone') return { ...selection, microphoneId: deviceId };
  return { ...selection, outputId: deviceId };
}

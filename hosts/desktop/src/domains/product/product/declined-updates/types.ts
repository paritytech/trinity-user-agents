import { type HexString } from '@/shared/types';
import { type ExecutableKind } from '../manifest/constants';
import { type SemVer } from '../manifest/schemas';

// A user's dismissal of a specific modality update, keyed by contenthash (the
// stable per-version identity). A later, different version is a different
// contenthash → a different record → the update re-surfaces. The persisted row
// stamps `declinedAt`; callers supply only the identity + version.
export type DeclinedUpdate = {
  baseName: string;
  kind: ExecutableKind;
  contenthash: HexString;
  version: SemVer;
};

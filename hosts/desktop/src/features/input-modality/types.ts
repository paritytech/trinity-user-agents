import { type Attachment, type RankedCandidate } from '@/domains/input-routing';

/** Everything one product answered, in its own rank order. */
export type CandidateGroup = {
  productId: string;
  candidates: RankedCandidate[];
};

/**
 * A draft attachment in the composer. Wraps the domain `Attachment` — which is
 * canonical, and already carries the derived category — with the two things only
 * the UI needs: a list key and a local preview URL.
 */
export type DraftAttachment = {
  id: string;
  // Size as the OS reported it. The domain derives size from `content`, but a
  // loader that has not read the bytes still has to show the user a number.
  sizeBytes: number;
  // Object URL for image/video chips, revoked when the draft is dropped or sent.
  previewUrl?: string;
  routed: Attachment;
};

/**
 * Turns a picked `File` into a draft attachment. Mocked until the upload
 * transport is specified — swap the implementation bound in `state/composer.ts`.
 */
export type AttachmentLoader = (file: File, signal: AbortSignal) => Promise<DraftAttachment>;

import { partition } from 'lodash-es';

import { createState } from '@/shared/rxstate';
import { isFulfilled } from '@/shared/utils';
import { inputRoutingService, routingUseCase } from '@/domains/input-routing';
import { type AttachmentLoader, type DraftAttachment } from '../types';

export const composerAttachments = createState<DraftAttachment[]>([]);
export const composerLoading = createState(false);
/** Files the host declined before delivery — over the attachment cap. */
export const composerDeclined = createState<string[]>([]);

let loadController: AbortController | null = null;
let attachmentCounter = 0;

// ponytail: no bytes are read and nothing is uploaded. The metadata comes off
// the File object, the category is derived by the domain, and the preview is a
// local object URL. Replace with the real upload transport when it is specified.
const mockAttachmentLoader: AttachmentLoader = (file, signal) =>
  new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      const routed = routingUseCase.toAttachment({
        fileName: file.name,
        mimeType: file.type,
        // Real loader reads the file; the mock carries no bytes.
        content: new Uint8Array(),
      });
      const previewUrl = routed.category === 'image' || routed.category === 'video' ? URL.createObjectURL(file) : undefined;

      resolve({ id: `attachment-${attachmentCounter++}`, sizeBytes: file.size, previewUrl, routed });
    }, 400);

    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });

// The swap point — see the domain's `gateway.ts` for the worker-side equivalent.
let attachmentLoader: AttachmentLoader = mockAttachmentLoader;

export function setAttachmentLoader(loader: AttachmentLoader) {
  attachmentLoader = loader;
}

/** Exported so the surface can revoke the drafts it takes out of here. */
export function revoke(attachments: DraftAttachment[]) {
  for (const attachment of attachments) {
    if (attachment.previewUrl) URL.revokeObjectURL(attachment.previewUrl);
  }
}

/**
 * Loads picked files into the draft. Every file type is accepted; one over the
 * host's attachment cap is declined to the user before anything moves, never to
 * a product.
 */
export async function attachFiles(files: File[]): Promise<void> {
  if (files.length === 0) return;

  const [declined, accepted] = partition(files, file => inputRoutingService.exceedsAttachmentCap(file.size));
  composerDeclined.set(declined.map(file => file.name));
  if (accepted.length === 0) return;

  loadController?.abort(new DOMException('Attachment load superseded', 'AbortError'));
  const controller = new AbortController();
  loadController = controller;

  composerLoading.set(true);
  try {
    // allSettled, not all: one unreadable file must not discard its siblings —
    // and with `all` the siblings that already produced an object URL would
    // leak it, since nothing would ever hold them to revoke.
    const results = await Promise.allSettled(accepted.map(file => attachmentLoader(file, controller.signal)));
    const loaded = results.filter(isFulfilled).map(result => result.value);

    if (controller.signal.aborted) {
      revoke(loaded);
      return;
    }
    composerAttachments.set(prev => [...prev, ...loaded]);
  } finally {
    if (loadController === controller) {
      loadController = null;
      composerLoading.set(false);
    }
  }
}

export function removeAttachment(id: string) {
  revoke(composerAttachments.get().filter(attachment => attachment.id === id));
  composerAttachments.set(prev => prev.filter(attachment => attachment.id !== id));
}

/**
 * Hands the draft attachments to the caller and empties the composer. Ownership
 * of each preview URL transfers with them — the caller revokes.
 */
export function takeAttachments(): DraftAttachment[] {
  const attachments = composerAttachments.get();
  composerAttachments.set([]);
  return attachments;
}

export function resetComposer() {
  loadController?.abort(new DOMException('Composer reset', 'AbortError'));
  loadController = null;
  revoke(composerAttachments.get());
  composerAttachments.set([]);
  composerLoading.set(false);
  composerDeclined.set([]);
  attachmentCounter = 0;
}

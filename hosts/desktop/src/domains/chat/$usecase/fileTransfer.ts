import { environmentUseCase } from '@/domains/application';
import { fileTransferGateway } from '../p2p/file-transfer/gateway';
import { type FileAttachment, type FileMeta } from '../session/types';

export type UploadChatFileInput = {
  file: File;
  meta: FileMeta;
  onProgress?: (sent: number, total: number) => void;
};

/**
 * Uploads an attachment to the active environment's HOP relay. Composes the environment
 * use case with the file-transfer gateway (why it's a use case): the gateway takes the
 * endpoints as a parameter so it never reaches for a use case itself.
 */
async function uploadChatFile(input: UploadChatFileInput): Promise<FileAttachment> {
  const environment = await environmentUseCase.getActive();
  const hopEndpoints = environment.bulletinHopEndpoints;
  if (!hopEndpoints?.length) throw new Error(`No HOP endpoint configured for environment: ${environment.id}`);

  return fileTransferGateway.uploadFile({ ...input, hopEndpoints });
}

export const fileTransferUseCase = { uploadChatFile };

import { type TrUApiProductProvider } from '@parity/truapi-host';

import { type FetchResolver, type Sandbox } from '@/shared/sandbox';

export type { FetchResolver, Sandbox };

export type ProductWorkerInstance = {
  readonly productId: string;
  readonly contenthash: string;
  readonly sandbox: Sandbox;
  /**
   * The worker's end of the core wire.
   *
   * Exposed because a product-rendered body is drawn by the product that authored
   * it, and the host reaches that product only through its worker's connection —
   * `render`, `publishRendererAction` and `publishChatAction` all hang off this
   * provider. Consumers MUST NOT post frames on it directly; it is piped to the
   * sandbox and a hand-written frame would race the pipe.
   */
  readonly coreProvider: TrUApiProductProvider;
  readonly disposed: boolean;
  dispose(): void;
};

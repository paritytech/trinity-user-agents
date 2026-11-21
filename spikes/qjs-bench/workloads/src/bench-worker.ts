// The product worker every engine runs: the real @parity/truapi client and
// transport, answering host-initiated render requests with the playground's
// real Chat diagnosis tree. The worker glue is ours because the playground
// worker ties each render to a random run id and to a message id it only
// learns from a live chat host, which a replay cannot reproduce.
import {
  createClient,
  createTransport,
  scale,
  type ProductRendererRenderRequest,
  type RendererNode,
  type WireProvider,
} from "@parity/truapi";
import { ChatDiagnosis, WORKER_DIAGNOSIS_METHODS } from "../../../../playground/worker/diagnosis";

/** What the corpus generator puts in each render request's payload. */
export interface RenderPayload {
  step: number;
  passed: number;
}

export function startBenchWorker(provider: WireProvider): void {
  const client = createClient(createTransport(provider));
  const diagnosis = new ChatDiagnosis();

  client.renderer.onRender((request: ProductRendererRenderRequest, send: (node: RendererNode) => void) => {
    const payload = JSON.parse(new TextDecoder().decode(scale.hexToBytes(request.payload))) as RenderPayload;
    for (const id of WORKER_DIAGNOSIS_METHODS.slice(0, payload.passed)) {
      diagnosis.pass(id, `passed at step ${payload.step}`);
    }
    send(diagnosis.rendererNode());
    return () => {};
  });
}

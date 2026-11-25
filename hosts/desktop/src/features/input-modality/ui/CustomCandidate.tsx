import { useCallback } from 'react';

import { type InputCandidateContent, routingUseCase } from '@/domains/input-routing';
import { type SubscribeToNode, RendererTree } from '@/widgets/RendererTree';

type Props = {
  productId: string;
  content: Extract<InputCandidateContent, { type: 'custom' }>;
};

/**
 * A candidate the product draws itself, through the same renderer a chat custom
 * message uses — so a product's own drawing looks and behaves identically
 * wherever the host shows it, and the visibility gating comes along for free.
 *
 * The payload stays opaque: the host asks the product to draw `candidateId` and
 * receives a render tree, never decoding the bytes it carried.
 */
export const CustomCandidate = ({ productId, content }: Props) => {
  const subscribe = useCallback<SubscribeToNode>(
    ({ onNode }) =>
      routingUseCase.renderCandidate(
        { productId, candidateId: content.candidateId, contentType: content.contentType, payload: content.payload },
        onNode,
      ),
    [productId, content],
  );

  const onAction = useCallback(
    // The action's return path is the other half of the SDK gap named in
    // `gateway.ts`: there is no method to tell a product its candidate's control
    // was used. Saying so beats pretending it landed.
    (actionId: string) => console.warn('[input-modality] candidate actions are not wired', { productId, actionId }),
    [productId],
  );

  return <RendererTree productId={productId} subscribe={subscribe} onAction={onAction} />;
};

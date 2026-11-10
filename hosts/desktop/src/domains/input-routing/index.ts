export type {
  Attachment,
  AttachmentCategory,
  AttachmentData,
  CandidateNode,
  InputCandidateContent,
  InputFile,
  InputMedia,
  InputResponse,
  InputSource,
  Query,
  QueryDeclaration,
  QueryRecipient,
  QueryRound,
  RankedCandidate,
  RoutedInput,
} from './types';

// Only the bound a caller has to honour. The caps and deadlines are enforced
// inside the round; publishing them invites a feature to re-enforce a rule the
// domain already owns, and then to drift from it.
export { DEBOUNCE_MS } from './constants';

export { inputRoutingService } from './service';

// The use case, and the *injection points* that stand in for the unwired worker
// boundary. Registering a handler is what an outside feature legitimately does;
// calling the wire is not — `gateway.ts` stays off the public surface, and a
// caller outside the domain reaches it through `routingUseCase`, so when real
// TrUAPI wiring replaces that file wholesale nothing above it changes.
export {
  type AnswerQueryRequest,
  type RenderCandidateRequest,
  answerQueryTransformer,
  renderCandidateTransformer,
  routingUseCase,
} from './$usecase/routing';

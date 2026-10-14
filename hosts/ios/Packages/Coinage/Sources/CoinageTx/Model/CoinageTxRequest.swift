import DurableTransactions
import ExtrinsicService
import Foundation

/// One transaction to register and submit: what it consumes and mints, plus how to build and
/// sign its extrinsic. A batch of these registers atomically under one ``CoinageTxGroupId``.
public struct CoinageTxRequest {
    public let inputs: [CoinageTxInput]
    public let outputs: [OwnAsset]
    public let builder: ExtrinsicBuilderClosure
    public let origin: any ExtrinsicOriginDefining

    /// Builds this transaction again, with these same assets, once an attempt is proven unable to
    /// land; `nil` for one whose failure is final.
    public let policy: SubmissionPolicy?

    public init(
        inputs: [CoinageTxInput],
        outputs: [OwnAsset],
        builder: @escaping ExtrinsicBuilderClosure,
        origin: any ExtrinsicOriginDefining,
        policy: SubmissionPolicy? = nil
    ) {
        self.inputs = inputs
        self.outputs = outputs
        self.builder = builder
        self.origin = origin
        self.policy = policy
    }
}

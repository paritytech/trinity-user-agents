import DurableTransactions
import Foundation

/// One transaction that `policy` builds and submits later, with the assets it consumes and mints.
///
/// Registering it locks those assets from the moment the registration commits, so nothing else can
/// select them while it waits to be built — which is what lets the slow part of a payment (unload
/// tokens, ring-VRF proofs) happen after the memo has already left.
public struct CoinageScheduledTxRequest: Sendable {
    public let policy: SubmissionPolicy
    public let inputs: [CoinageTxInput]
    public let outputs: [OwnAsset]

    public init(policy: SubmissionPolicy, inputs: [CoinageTxInput], outputs: [OwnAsset]) {
        self.policy = policy
        self.inputs = inputs
        self.outputs = outputs
    }
}

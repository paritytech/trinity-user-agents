import Foundation
import SubstrateSdk

/// Combines the pool-based handoff service with a long-term remote store (chat RFC 0001):
/// claims try the pool first and fall back to on-chain storage when the entry is gone.
/// Acks for chain-sourced entries are skipped — the pool entry no longer exists.
/// The chain-source marks are in-memory only, which is safe because resume flows never re-ack.
public actor HandoffServiceDecorator {
    let handoffService: HandoffServicing
    let remoteStore: LongTermRemoteStoring

    private var chainSourcedHashes: Set<FileHash> = []

    public init(handoffService: HandoffServicing, remoteStore: LongTermRemoteStoring) {
        self.handoffService = handoffService
        self.remoteStore = remoteStore
    }
}

extension HandoffServiceDecorator: HandoffServicing {
    @discardableResult
    public func submitData(
        _ data: Data,
        from sender: SenderProofProviding,
        recipients: Set<MultiSigner>
    ) async throws -> SubmittedData {
        try await handoffService.submitData(data, from: sender, recipients: recipients)
    }

    public func claimData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws -> Data? {
        do {
            return try await handoffService.claimData(by: dataHash, recipient: recipient)
        } catch let error as JSONRPCError where error.code == HOPErrorCode.notFound {
            guard let data = try await remoteStore.downloadData(by: dataHash) else {
                return nil
            }

            chainSourcedHashes.insert(dataHash)

            return data
        }
    }

    public func acknowledgeReceivedData(
        by dataHash: FileHash,
        recipient: RecipientProofProviding
    ) async throws {
        guard chainSourcedHashes.remove(dataHash) == nil else {
            return
        }

        try await handoffService.acknowledgeReceivedData(by: dataHash, recipient: recipient)
    }
}

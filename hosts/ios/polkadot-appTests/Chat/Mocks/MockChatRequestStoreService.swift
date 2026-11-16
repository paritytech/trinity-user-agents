@testable import polkadot_app
import Foundation

/// Records the store calls the request coordination makes; no request exists locally unless seeded.
final class MockChatRequestStoreService: ChatRequestStoreServicing, @unchecked Sendable {
    enum Call: Equatable {
        case newIncoming(requestId: String)
        case update(requestId: String)
        case replace(requestId: String)
        case acceptIncoming
        case acceptOutgoing(requestId: String)
        case declineIncoming(requestId: String)
        case cancelIncoming(requestId: String)
        case storeCancelled(requestId: String)
        case markCancelled(requestId: String)
    }

    private let lock = NSLock()
    private var recordedCalls: [Call] = []
    private var recordedCancelProofRequestIds: [String] = []
    private let localRequests: [String: Chat.Request]
    private let cancelResult: Bool
    private let acceptorDevice: Chat.PeerDevice
    private let markCancelledError: Error?

    init(
        localRequests: [String: Chat.Request] = [:],
        cancelResult: Bool = true,
        acceptorDevice: Chat.PeerDevice = Chat.PeerDevice(
            statementAccountId: Data(repeating: 0x0C, count: 32),
            encryptionPublicKey: Data(repeating: 0x0D, count: 32)
        ),
        markCancelledError: Error? = nil
    ) {
        self.localRequests = localRequests
        self.cancelResult = cancelResult
        self.acceptorDevice = acceptorDevice
        self.markCancelledError = markCancelledError
    }

    var calls: [Call] {
        lock.withLock { recordedCalls }
    }

    var cancelProofRequestIds: [String] {
        lock.withLock { recordedCancelProofRequestIds }
    }

    func fetchByRequestId(_ requestId: String) async throws -> Chat.Request? {
        localRequests[requestId]
    }

    func newIncomingRequestFromRemote(
        _ remote: ChatRequest.ValidatedRemoteModel,
        contact _: Chat.RemoteContact,
        ownKeyId _: Chat.Contact.Own
    ) async throws {
        record(.newIncoming(requestId: remote.requestId))
    }

    func newOutgoingRequestFromText(
        _: String?,
        contact _: Chat.RemoteContact,
        ownKeyId _: Chat.Contact.Own,
        ownPushToken _: Data?
    ) async throws {}

    func updateRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws {
        record(.update(requestId: remote.requestId))
    }

    func replaceRequestFromRemote(_ remote: ChatRequest.ValidatedRemoteModel) async throws {
        record(.replace(requestId: remote.requestId))
    }

    func acceptIncomingRequest(_: ChatRequest.AcceptIncoming) async throws {
        record(.acceptIncoming)
    }

    func acceptOutgoingRequest(
        _ requestId: String,
        remoteIncoming _: ChatRequest.ValidatedRemoteModel?,
        acceptorDevice _: Chat.PeerDevice?,
        peerAcceptance _: ChatRequest.PeerAcceptance?
    ) async throws {
        record(.acceptOutgoing(requestId: requestId))
    }

    func declineIncomingRequest(_ requestId: String) async throws {
        record(.declineIncoming(requestId: requestId))
    }

    func cancelIncomingRequest(_ requestId: String) async throws -> Bool {
        record(.cancelIncoming(requestId: requestId))
        return cancelResult
    }

    func storeCancelledRequest(_ remote: ChatRequest.ValidatedRemoteModel) async throws {
        record(.storeCancelled(requestId: remote.requestId))
    }

    func buildLocalAcceptorDevice() throws -> Chat.PeerDevice {
        acceptorDevice
    }

    func makeOutgoingIdentityProof(
        peerEncryptionPubKey _: Data,
        ownEncryptionKeyId _: String
    ) throws -> Chat.IdentityProof {
        Chat.IdentityProof(
            identityAccountId: Data(repeating: 0x0A, count: 32),
            proof: Data(repeating: 0x0B, count: 32)
        )
    }

    func makeOutgoingCancelProof(
        requestId: String,
        peerEncryptionPubKey _: Data,
        ownEncryptionKeyId _: String
    ) throws -> Chat.IdentityProof {
        lock.withLock { recordedCancelProofRequestIds.append(requestId) }

        return Chat.IdentityProof(
            identityAccountId: Data(repeating: 0x0A, count: 32),
            proof: Data(repeating: 0x0B, count: 32)
        )
    }

    func markOutgoingRequestCancelled(_ requestId: String) async throws {
        record(.markCancelled(requestId: requestId))
        if let markCancelledError { throw markCancelledError }
    }
}

private extension MockChatRequestStoreService {
    func record(_ call: Call) {
        lock.withLock { recordedCalls.append(call) }
    }
}

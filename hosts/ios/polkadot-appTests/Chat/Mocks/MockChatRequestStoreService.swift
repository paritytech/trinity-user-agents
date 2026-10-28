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
    }

    private let lock = NSLock()
    private var recordedCalls: [Call] = []
    private let localRequests: [String: Chat.Request]
    private let acceptorDevice: Chat.PeerDevice

    init(
        localRequests: [String: Chat.Request] = [:],
        acceptorDevice: Chat.PeerDevice = Chat.PeerDevice(
            statementAccountId: Data(repeating: 0x0C, count: 32),
            encryptionPublicKey: Data(repeating: 0x0D, count: 32)
        )
    ) {
        self.localRequests = localRequests
        self.acceptorDevice = acceptorDevice
    }

    var calls: [Call] {
        lock.withLock { recordedCalls }
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
        peerDevice _: Chat.PeerDevice?
    ) async throws {
        record(.acceptOutgoing(requestId: requestId))
    }

    func declineIncomingRequest(_ requestId: String) async throws {
        record(.declineIncoming(requestId: requestId))
    }

    func buildLocalAcceptorDevice() throws -> Chat.PeerDevice {
        acceptorDevice
    }
}

private extension MockChatRequestStoreService {
    func record(_ call: Call) {
        lock.withLock { recordedCalls.append(call) }
    }
}

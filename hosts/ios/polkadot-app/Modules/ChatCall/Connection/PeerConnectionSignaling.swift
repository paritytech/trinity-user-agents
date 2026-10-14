import Foundation
import SubstrateSdk
import AsyncExtensions

protocol PeerConnectionSignaling {
    var signals: AnyAsyncSequence<PeerConnectionSignal> { get async }

    @discardableResult
    func send(_ signals: [PeerConnectionSignal]) async throws -> PeerConnectionSignalSendResult
}

protocol ChatCallMessageHandling: AnyObject {
    var peerAccountId: AccountId { get }

    func receive(message: Chat.RemoteMessage) async
    func handleSent(message: Chat.RemoteMessage) async
    func handleDelivered(message: Chat.RemoteMessage) async
}

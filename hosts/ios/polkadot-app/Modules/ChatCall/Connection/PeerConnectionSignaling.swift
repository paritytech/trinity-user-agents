import Foundation
import SubstrateSdk
import AsyncExtensions

protocol PeerConnectionSignaling {
    var signals: AnyAsyncSequence<PeerConnectionSignal> { get async }

    @discardableResult
    func send(_ signals: [PeerConnectionSignal]) async throws -> PeerConnectionSignalSendResult
}

import Foundation
import StatementStore

/// The account that signs a chat request statement, and the period it is funded for.
struct ChatRequestDeliverySigner {
    let signer: StatementStoreSigning
    let period: UInt32
}

protocol ChatRequestDeliverySigning {
    /// A statement account used for one request in one period, so nothing on chain links it to us or to
    /// another request.
    func anonymous(requestId: String, period: UInt32) throws -> ChatRequestDeliverySigner
}

final class ChatRequestDeliverySigners {
    private let signManager: StatementStoreSignerManaging

    init(signManager: StatementStoreSignerManaging) {
        self.signManager = signManager
    }
}

extension ChatRequestDeliverySigners: ChatRequestDeliverySigning {
    func anonymous(requestId: String, period: UInt32) throws -> ChatRequestDeliverySigner {
        try ChatRequestDeliverySigner(
            signer: signManager.makeSigner(for: "//chat-request//\(requestId)//\(period)"),
            period: period
        )
    }
}

import Foundation
import StatementStore

protocol OutgoingRequestSizeValidating {
    var maxPayloadSize: Int { get }
}

extension OutgoingRequestSizeValidating {
    func scaleEncodedPayloadFits(_ payload: Data) -> Bool {
        maxPayloadSize >= payload.count
    }
}

final class OutgoingRequestSizeValidator {
    let maxStatementSize: Int

    init(maxStatementSize: Int) {
        self.maxStatementSize = maxStatementSize
    }
}

extension OutgoingRequestSizeValidator: OutgoingRequestSizeValidating {
    var maxPayloadSize: Int {
        maxStatementSize - StatementSize.overhead(topicCount: 1)
    }
}

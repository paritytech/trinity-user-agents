import Foundation
import Operation_iOS

struct SSOHandledRequest: Hashable {
    let messageId: String
}

extension SSOHandledRequest: Operation_iOS.Identifiable {
    var identifier: String { messageId }
}

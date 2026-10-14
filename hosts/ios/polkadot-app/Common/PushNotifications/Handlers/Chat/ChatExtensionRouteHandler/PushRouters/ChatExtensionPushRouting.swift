import Foundation

protocol ChatExtensionPushRouting {
    func process(
        userInfo: [AnyHashable: Any],
        chatOpenClosure: (() -> Void)?
    )
}

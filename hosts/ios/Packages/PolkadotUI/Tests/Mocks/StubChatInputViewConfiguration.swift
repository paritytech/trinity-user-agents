import UIKit
@testable import PolkadotUI

struct StubChatInputViewConfiguration: ChatInputViewConfigurationProtocol {
    let contentView: UIView
    var safeAreaFillColor: UIColor?

    var activateOnAppear: Bool {
        false
    }

    func makeContentView(for _: ChatInputHandling?) -> UIView {
        contentView
    }

    func equalsTo(configuration _: any ChatInputViewConfigurationProtocol) -> Bool {
        false
    }
}

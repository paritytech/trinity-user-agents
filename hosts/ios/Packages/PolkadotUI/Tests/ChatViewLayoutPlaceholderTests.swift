import Testing
import UIKit
@testable import PolkadotUI

/// The room screen shows the placeholder text of the view model, and nothing without one.
@MainActor
struct ChatViewLayoutPlaceholderTests {
    @Test func theScreenShowsThePlaceholderOfItsViewModel() {
        let layout = ChatViewLayout(frame: CGRect(x: 0, y: 0, width: 390, height: 700))

        layout.bind(viewModel: viewModel(placeholder: "No games yet"))
        #expect(visibleTexts(in: layout).contains("No games yet"))

        layout.bind(viewModel: viewModel(placeholder: nil))
        #expect(!visibleTexts(in: layout).contains("No games yet"))
    }
}

private extension ChatViewLayoutPlaceholderTests {
    func viewModel(placeholder: String?) -> ChatViewLayout.ViewModel {
        ChatViewLayout.ViewModel(
            headerConfiguration: .empty(),
            chatInputConfiguration: nil,
            scrollDownConfiguration: .init(),
            sections: [],
            footerConfiguration: nil,
            placeholder: placeholder
        )
    }

    func visibleTexts(in layout: ChatViewLayout) -> [String] {
        layout.subviews.compactMap { view in
            guard let label = view as? UILabel, !label.isHidden else {
                return nil
            }
            return label.text
        }
    }
}

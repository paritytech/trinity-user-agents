import UIKit

public class TopBottomLabelView: GenericMultiValueView<Label> {
    public var topLabel: Label { valueTop }
    public var bottomLabel: Label { valueBottom }
}

public extension TopBottomLabelView {
    struct ViewModel {
        public let top: String
        public let bottom: String

        public init(top: String, bottom: String) {
            self.top = top
            self.bottom = bottom
        }
    }

    func bind(viewModel: TopBottomLabelView.ViewModel) {
        topLabel.text = viewModel.top
        bottomLabel.text = viewModel.bottom
    }
}

import UIKit

public class GenericMultiValueView<BottomView: UIView>: GenericPairValueView<Label, BottomView> {
    public var valueTop: Label { fView }

    public var valueBottom: BottomView { sView }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        stackView.axis = .vertical
    }
}

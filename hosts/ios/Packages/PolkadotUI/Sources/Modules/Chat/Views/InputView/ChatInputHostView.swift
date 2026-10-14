import UIKit
internal import SnapKit

public final class ChatInputHostView: UIView {
    private var currentContentView: UIView?
    private var currentConfiguration: (any ChatInputViewConfigurationProtocol)?
    private let safeAreaFillView = UIView()

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        backgroundColor = .clear

        addSubview(safeAreaFillView)
        safeAreaFillView.snp.makeConstraints { make in
            make.leading.trailing.bottom.equalToSuperview()
            make.top.equalTo(safeAreaLayoutGuide.snp.bottom)
        }
    }

    func clear() {
        currentContentView?.removeFromSuperview()
        currentContentView = nil
        currentConfiguration = nil
        safeAreaFillView.backgroundColor = nil
    }

    func bind(
        configuration: any ChatInputViewConfigurationProtocol,
        inputHandler: ChatInputHandling?,
        keyboardGuide: UILayoutGuide
    ) {
        if let currentConfiguration, currentConfiguration.equalsTo(configuration: configuration) {
            return
        }

        let newContentView = configuration.makeContentView(for: inputHandler)

        currentContentView?.removeFromSuperview()

        currentContentView = newContentView
        addSubview(newContentView)

        currentConfiguration = configuration

        newContentView.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
            make.bottom.equalTo(keyboardGuide.snp.top)
        }

        safeAreaFillView.backgroundColor = configuration.safeAreaFillColor
    }

    public var contentHeight: CGFloat {
        guard let currentContentView else {
            return 0
        }
        let height = currentContentView.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize).height
        if currentConfiguration?.safeAreaFillColor != nil {
            return height + safeAreaInsets.bottom
        }
        return height
    }

    public var contentView: UIView? {
        currentContentView
    }

    public var replyInterface: ChatInputReplyPresenting? {
        currentContentView as? ChatInputReplyPresenting
    }

    public var editInterface: ChatInputEditPresenting? {
        currentContentView as? ChatInputEditPresenting
    }

    public var textInputInterface: ChatTextInputPresenting? {
        currentContentView as? ChatTextInputPresenting
    }
}

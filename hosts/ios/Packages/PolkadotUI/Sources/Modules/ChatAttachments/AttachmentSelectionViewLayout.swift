import ExternalAccessibility
import UIKit
internal import SnapKit
import FoundationExt

public final class ChatAttachmentsViewLayout: UIView, KeyboardAdoptableViewLayout {
    public let scrollView: UIScrollView = {
        let sv = UIScrollView()
        sv.isPagingEnabled = true
        sv.showsHorizontalScrollIndicator = false
        sv.showsVerticalScrollIndicator = false
        sv.backgroundColor = .clear
        return sv
    }()

    private let pageStackView: UIStackView = {
        let stack = UIStackView()
        stack.axis = .horizontal
        stack.spacing = 0
        stack.alignment = .fill
        stack.distribution = .fillEqually
        return stack
    }()

    private let activityIndicator: UIActivityIndicatorView = .create { view in
        view.style = .medium
        view.color = .white
        view.hidesWhenStopped = true
    }

    let chatInputView: DSChatInputView = {
        let configuration = ChatInputViewConfiguration.chat(
            canPay: false,
            canAttachFile: false,
            canSendWithoutText: true,
            inputAccessibilityId: nil,
            sendAccessibilityId: AccessibilityID.Chat.attachmentSendButton
        )
        return DSChatInputView(configuration: configuration, handler: nil)
    }()

    private var itemViews: [UIView] = []

    public weak var parentViewController: UIViewController?

    public var onSendTap: ((String) -> Void)?

    private var dimView: UIView = .create { view in
        view.backgroundColor = UIColor(resource: .black45)
        view.isHidden = true
    }

    // MARK: - Init

    override public init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()

        chatInputView.inputHandler = self

        let recognizer = UITapGestureRecognizer(target: self, action: #selector(didTapOnDimView))
        dimView.addGestureRecognizer(recognizer)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Public

    public func setAttachments(_ viewModels: [AttachmentSelectionViewModel]) {
        for itemView in itemViews {
            if let videoView = itemView as? AttachmentSelectionVideoView {
                videoView.cleanup()
            }
            itemView.removeFromSuperview()
        }
        itemViews = []

        for viewModel in viewModels {
            switch viewModel {
            case let .image(imageViewModel):
                let imageView = AttachmentSelectionImageView()
                imageView.contentMode = .scaleAspectFit
                imageView.clipsToBounds = true
                imageView.backgroundColor = .clear

                imageView.bind(imageViewModel: imageViewModel)

                pageStackView.addArrangedSubview(imageView)
                imageView.snp.makeConstraints {
                    $0.width.equalTo(scrollView.snp.width)
                }

                itemViews.append(imageView)
            case let .video(url):
                let videoView = AttachmentSelectionVideoView()

                pageStackView.addArrangedSubview(videoView)

                videoView.snp.makeConstraints { make in
                    make.width.equalTo(scrollView.snp.width)
                }

                if let parentViewController {
                    videoView.configure(url: url, parentViewController: parentViewController)
                }
                itemViews.append(videoView)
            }
        }
    }

    public func startLoading() {
        chatInputView.isHidden = true
        activityIndicator.startAnimating()
    }

    public func stopLoading() {
        chatInputView.isHidden = false
        activityIndicator.stopAnimating()
    }
}

extension ChatAttachmentsViewLayout: ChatInputHandling {
    public func chatInputDidSend(_ text: String) {
        onSendTap?(text)
    }

    public func chatInputDidTransfer() {}
    public func chatInputDidAttachment() {}
    public func chatInputDidCancelReply() {}
    public func chatInputDidCancelEdit() {}
    public func chatInputDidChange() {}
}

// MARK: - Keyboard Adoptable

public extension ChatAttachmentsViewLayout {
    func adoptToVisibleKeyboard(bottomInset _: CGFloat) {
        scrollView.snp.remakeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.bottom.equalTo(safeAreaLayoutGuide.snp.bottom)
        }

        setVideoPlaybackControlsHidden(true)
        showDimView()
    }

    func adoptToHiddenKeyboard() {
        scrollView.snp.remakeConstraints { make in
            make.leading.trailing.equalToSuperview()
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.bottom.equalTo(chatInputView.snp.top)
        }

        setVideoPlaybackControlsHidden(false)
        hideDimView()
    }
}

extension ChatAttachmentsViewLayout {
    func showDimView() {
        dimView.setHidden(false)
        dimView.alpha = 0

        UIView.animate(
            withDuration: 0.25,
            animations: { [weak self] in
                self?.dimView.alpha = 1
            },
            completion: nil
        )
    }

    func hideDimView() {
        UIView.animate(
            withDuration: 0.25,
            animations: { [weak self] in
                self?.dimView.alpha = 0
            },
            completion: { [weak self] _ in
                self?.dimView.setHidden(true)
            }
        )
    }

    func setVideoPlaybackControlsHidden(_ hidden: Bool) {
        for itemView in itemViews {
            if let videoView = itemView as? AttachmentSelectionVideoView {
                videoView.setPlaybackControlsHidden(hidden)
            }
        }
    }

    @objc func didTapOnDimView() {
        chatInputView.textView.resignFirstResponder()
    }
}

// MARK: - Layout

private extension ChatAttachmentsViewLayout {
    enum Constants {
        static let inputViewInset: CGFloat = 0
    }

    func setupLayout() {
        addSubview(scrollView)
        scrollView.addSubview(pageStackView)

        addSubview(activityIndicator)
        addSubview(dimView)
        addSubview(chatInputView)

        scrollView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.top.equalTo(safeAreaLayoutGuide.snp.top)
            $0.bottom.equalTo(chatInputView.snp.top)
        }

        pageStackView.snp.makeConstraints {
            $0.edges.equalToSuperview()
            $0.height.equalToSuperview()
        }

        activityIndicator.snp.makeConstraints {
            $0.center.equalToSuperview()
        }

        chatInputView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.bottom.equalTo(keyboardLayoutGuide.snp.top).inset(Constants.inputViewInset)
        }

        dimView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }
}

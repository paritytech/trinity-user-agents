import UIKit

public protocol ChatProductNameProviding: AnyObject {
    var identifier: String { get }
    func provideName(_ completion: @escaping (String?) -> Void)
    func cancel()
}

public struct ChatProductLinkPreviewConfiguration: HashableContentConfiguration {
    public enum Style: Hashable {
        case inbox
        case outbox
    }

    public let domain: String
    public let style: Style
    public let nameProvider: ChatProductNameProviding?
    public let imageViewModel: ImageViewModelProtocol?
    public let tap: () -> Void

    public init(
        domain: String,
        style: Style,
        nameProvider: ChatProductNameProviding?,
        imageViewModel: ImageViewModelProtocol?,
        tap: @escaping () -> Void
    ) {
        self.domain = domain
        self.style = style
        self.nameProvider = nameProvider
        self.imageViewModel = imageViewModel
        self.tap = tap
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatProductLinkPreviewView(configuration: self)
    }

    public func updated(for _: UIConfigurationState) -> Self { self }

    public static func == (
        lhs: ChatProductLinkPreviewConfiguration,
        rhs: ChatProductLinkPreviewConfiguration
    ) -> Bool {
        lhs.domain == rhs.domain &&
            lhs.style == rhs.style &&
            lhs.nameProvider?.identifier == rhs.nameProvider?.identifier
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(domain)
        hasher.combine(style)
        hasher.combine(nameProvider?.identifier)
    }
}

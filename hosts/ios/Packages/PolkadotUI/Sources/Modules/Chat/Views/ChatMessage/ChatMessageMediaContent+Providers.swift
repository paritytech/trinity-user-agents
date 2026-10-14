import UIKit

// MARK: - Preview provider protocol

public protocol ChatMessageMediaPreviewProviding: AnyObject {
    @MainActor func providePreview(
        for imageView: UIImageView,
        size: CGSize?
    )
    // used to optimise loading
    var identifier: String { get }
}

// MARK: - Button configuration provider protocol

public protocol ChatMessageMediaButtonConfigurationProviding: AnyObject {
    func startUpdate(onUpdate: @escaping (ChatMessageMediaViewConfiguration.ButtonConfiguration?) -> Void)
    func stopUpdate()
}

// MARK: - Static button configuration provider

public final class StaticChatMessageMediaButtonConfigurationProvider: ChatMessageMediaButtonConfigurationProviding {
    private let buttonConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?

    public init(_ buttonConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?) {
        self.buttonConfiguration = buttonConfiguration
    }

    public func startUpdate(onUpdate: @escaping (ChatMessageMediaViewConfiguration.ButtonConfiguration?) -> Void) {
        onUpdate(buttonConfiguration)
    }

    public func stopUpdate() {}
}

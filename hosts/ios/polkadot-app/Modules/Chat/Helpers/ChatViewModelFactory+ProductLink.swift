import Foundation
import PolkadotUI
import Products

extension ChatViewModelFactory {
    /// Detects the first product universal link in the message text
    /// Returns empty text if text is equal to url
    func resolveProductLinkPreview(
        text: String,
        status: Chat.LocalMessage.Status,
        actions: ChatViewModelActions?
    ) -> (text: String, preview: ChatProductLinkPreviewConfiguration?) {
        guard let detected = ProductURLDetector.firstProductLink(in: text, hostProvider: flowState.hostProvider) else {
            return (text, nil)
        }

        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let strippedText: String = trimmed == detected.matchedURL ? "" : text

        let nameProvider = ProductNameProvider(
            host: detected.host,
            productRepository: productRepository,
            dotNsResolver: flowState.dotNsResolver,
            productResolver: flowState.productResolver,
            nameCache: productNameCache
        )

        let imageViewModel = flowState.iconViewModelFactory
            .createViewModel(for: detected.host.toDotDomain())

        let preview = ChatProductLinkPreviewConfiguration(
            domain: detected.host.toDotDomain(),
            style: previewStyle(for: status),
            nameProvider: nameProvider,
            imageViewModel: imageViewModel,
            tap: { [weak self] in
                guard
                    let self,
                    let url = URL(string: detected.matchedURL),
                    let productPage = flowState.hostProvider.page(url: url)
                else { return }

                actions?.openProduct(productPage)
            }
        )

        return (strippedText, preview)
    }

    private func previewStyle(
        for status: Chat.LocalMessage.Status
    ) -> ChatProductLinkPreviewConfiguration.Style {
        switch status {
        case .incoming: .inbox
        case .outgoing: .outbox
        }
    }
}

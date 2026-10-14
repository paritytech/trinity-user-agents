import Foundation
import PolkadotUI

extension ChatImageLoader {
    /// Loads a v1 product media source, an inline `data:` image or an `https` URL, through the resolver.
    static func resources(_ resolver: ChatResourceResolving = ChatResourceResolver.shared) -> ChatImageLoader {
        ChatImageLoader { source in
            guard let resource = ChatResource(source: source) else { return nil }
            return await resolver.image(for: resource)
        }
    }
}

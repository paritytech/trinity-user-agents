import Foundation
import KeyDerivation
import Products
import TrUAPIHost

/// Opens the HOP files of the chat product through `open_hop_chat_resource` of the shared runtime. The core claims
/// them from the nodes `AppPersonChatHostBridge` lists and checks every entry; this type opens no connection.
struct TrUAPIChatHopFiles: ChatHopFileOpening {
    private let runtime: @Sendable () throws -> TrUAPIHostRuntime
    private let tldProvider: DotNsTldProviding

    init(
        runtime: @escaping @Sendable () throws -> TrUAPIHostRuntime = TrUAPIProductAttachmentStaging.sharedRuntime,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.runtime = runtime
        self.tldProvider = tldProvider
    }

    func open(_ resource: ChatHopResource) async throws -> ChatHopFile {
        let file = try await runtime().openHopChatResource(
            productId: BuiltInProduct.chat(for: tldProvider.resolveTld()),
            resource: resource.wireResource
        )
        return ChatHopFile(bytes: file.bytes, entries: file.entries)
    }

    func ack(_ resource: ChatHopResource, entries: [Data]) async throws {
        try await runtime().ackHopChatResource(
            productId: BuiltInProduct.chat(for: tldProvider.resolveTld()),
            resource: resource.wireResource,
            entries: entries
        )
    }
}

extension ChatHopResource {
    /// The resource that the core and the product name.
    var wireResource: TrUAPIChatResource {
        .hop(identifier: identifier, ticket: ticket, node: node, size: size)
    }
}

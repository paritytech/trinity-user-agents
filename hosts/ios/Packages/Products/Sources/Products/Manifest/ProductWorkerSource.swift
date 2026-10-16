import Foundation

/// Where a product's chat worker is served from: its own subname when a worker manifest declares
/// one, the base name's archive and a conventional entry file otherwise.
public struct ProductWorkerSource: Hashable, Sendable {
    public let contentId: ProductId
    public let entryRelativePath: String

    public init(contentId: ProductId, entryRelativePath: String) {
        self.contentId = contentId
        self.entryRelativePath = entryRelativePath
    }
}

/// Which surface a worker is being started for. A worker declares the surfaces
/// it serves, and a host must not run it for one it did not declare.
public enum ProductWorkerModality: Hashable, Sendable {
    case chat
    case pocket
}

public extension ProductWorkerSource {
    /// The published worker for `resolved`, whatever it serves. A product has
    /// one worker and every modality is served from it, so what it declares is
    /// the holder's to check before it asks for one.
    static func published(for resolved: ResolvedProduct) -> ProductWorkerSource? {
        resolved.executables.worker.map {
            ProductWorkerSource(contentId: $0.identifier, entryRelativePath: $0.entrypoint)
        }
    }

    /// The published worker for `resolved`, when it declares `modality`.
    ///
    /// A product that published a worker without the modality serves none of
    /// it, and gets no fallback: handing its cards or its chat to a file the
    /// product never published would run code it did not declare.
    static func published(
        for resolved: ResolvedProduct,
        serving modality: ProductWorkerModality
    ) -> ProductWorkerSource? {
        guard let worker = resolved.executables.worker else { return nil }
        guard worker.serves(modality) else { return nil }

        return ProductWorkerSource(contentId: worker.identifier, entryRelativePath: worker.entrypoint)
    }

    /// The script installed by hand through debug settings, which stands in for
    /// a product that has published no worker at all.
    static func installedByHand(
        for resolved: ResolvedProduct,
        entryPath: (ProductId) -> String?
    ) -> ProductWorkerSource? {
        guard resolved.executables.worker == nil else { return nil }

        return entryPath(resolved.id).map {
            ProductWorkerSource(contentId: resolved.id, entryRelativePath: $0)
        }
    }
}

public extension ProductExecutable.Worker {
    /// Whether this worker declares `modality`.
    func serves(_ modality: ProductWorkerModality) -> Bool {
        modalities.contains { $0.kind == modality }
    }
}

import Foundation
import Kingfisher

enum LoadableAttachmentImageProviderError: Error {
    case attachmentNotFound
}

final class LoadableAttachmentImageProvider: @unchecked Sendable {
    private let service: AttachmentLoadProgressProvidable
    private let attachmentId: AttachmentId
    private let innerProvider: ImageDataProvider
    private let dataExistencePredicate: () -> Bool

    private let mutex = NSLock()
    private var task: Task<Void, Never>?

    init(
        attachmentId: AttachmentId,
        innerProvider: ImageDataProvider,
        service: AttachmentLoadProgressProvidable,
        dataExistencePredicate: @escaping () -> Bool
    ) {
        self.attachmentId = attachmentId
        self.innerProvider = innerProvider
        self.service = service
        self.dataExistencePredicate = dataExistencePredicate
    }

    deinit {
        task?.cancel()
    }
}

private extension LoadableAttachmentImageProvider {
    func provideDataOrError(for handler: @escaping @Sendable (Result<Data, any Error>) -> Void) {
        guard dataExistencePredicate() else {
            handler(.failure(LoadableAttachmentImageProviderError.attachmentNotFound))
            return
        }

        innerProvider.data(handler: handler)
    }
}

extension LoadableAttachmentImageProvider: ImageDataProvider {
    var cacheKey: String {
        attachmentId.stringValue
    }

    func data(handler: @escaping @Sendable (Result<Data, any Error>) -> Void) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard !dataExistencePredicate() else {
            innerProvider.data(handler: handler)
            return
        }

        task?.cancel()

        task = Task { [service, attachmentId, weak self] in
            do {
                let stream = await service.subscribeState(for: attachmentId)

                for try await event in stream {
                    switch event {
                    case .onComplete:
                        self?.provideDataOrError(for: handler)
                        return
                    case let .onFailure(error):
                        handler(.failure(error))
                        return
                    case .onProgress,
                         nil:
                        continue
                    }
                }
            } catch {
                if !Task.isCancelled {
                    handler(.failure(error))
                }
            }
        }
    }
}

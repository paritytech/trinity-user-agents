import Foundation
import PolkadotUI

final class AttachmentProgressViewModel: LoadingProgressViewModelProtocol {
    private let service: AttachmentLoadProgressProvidable
    private let attachmentId: AttachmentId

    private var task: Task<Void, Never>?

    let loadingDirection: LoadingDirection

    init(
        service: AttachmentLoadProgressProvidable,
        attachmentId: AttachmentId,
        loadingDirection: LoadingDirection
    ) {
        self.service = service
        self.attachmentId = attachmentId
        self.loadingDirection = loadingDirection
    }

    deinit {
        task?.cancel()
    }

    func startProgressUpdate(
        onProgress: @escaping (CGFloat) -> Void,
        onSuccess: @escaping () -> Void,
        onFailure: @escaping () -> Void
    ) {
        task?.cancel()

        task = Task(priority: .userInitiated) { [service, attachmentId] in
            do {
                let stream = await service.subscribeState(for: attachmentId)

                for try await event in stream {
                    guard !Task.isCancelled else { return }

                    switch event {
                    case let .onProgress(progress):
                        await MainActor.run {
                            if progress.total > 0 {
                                onProgress(CGFloat(progress.loaded) / CGFloat(progress.total))
                            } else {
                                onProgress(1)
                            }
                        }
                    case .onComplete:
                        await MainActor.run {
                            onSuccess()
                        }
                        return
                    case .onFailure:
                        await MainActor.run {
                            onFailure()
                        }
                        return
                    case nil:
                        continue
                    }
                }
            } catch {
                // we can appear here only on cancellation
            }
        }
    }

    func stopProgressUpdate() {
        task?.cancel()
        task = nil
    }
}

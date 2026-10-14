#if TESTNET_FEATURE
    import PolkadotUI
    import StructuredConcurrency
    import UIKit

    /// Bridges `StallBoard` snapshots onto the banner window. The reveal latch and the 1s tick
    /// live in `StallBoard`.
    @MainActor
    final class StallBannerPresenter {
        private let board: StallBoard
        private let viewModelFactory: StallBannerViewModelFactoryProtocol
        private let windowScene: UIWindowScene

        private var window: StallBannerWindow?
        private var subscriptionTask: Task<Void, Never>?

        init(
            board: StallBoard,
            viewModelFactory: StallBannerViewModelFactoryProtocol,
            windowScene: UIWindowScene
        ) {
            self.board = board
            self.viewModelFactory = viewModelFactory
            self.windowScene = windowScene
        }

        deinit {
            subscriptionTask?.cancel()
        }

        func setup() {
            guard subscriptionTask == nil else { return }

            subscriptionTask = Task { [board, weak self] in
                do {
                    for try await snapshot in board.snapshots {
                        self?.handle(snapshot: snapshot)
                    }
                } catch {
                    // Ignore stream errors (e.g., cancellation)
                }
            }
        }
    }

    private extension StallBannerPresenter {
        func handle(snapshot: StallReportSnapshot?) {
            guard let viewModel = viewModelFactory.createViewModel(from: snapshot) else {
                window?.hide()
                window = nil
                return
            }

            if window == nil {
                window = StallBannerWindow(windowScene: windowScene)
            }

            window?.show(viewModel: viewModel, onDismiss: { [board] id in
                Task { await board.dismiss(id: id) }
            })
        }
    }
#endif

import Foundation
import FoundationExt

public final class JSEngineMonitor: @unchecked Sendable {
    private let engine: JSEngineProtocol
    private let pauseEvent: ApplicationStateStreamFactory.Event
    private let resumeEvent: ApplicationStateStreamFactory.Event
    private let applicationStateStreamFactory = ApplicationStateStreamFactory()
    private var lifecycleTask: Task<Void, Never>?

    public init(
        engine: JSEngineProtocol,
        pauseEvent: ApplicationStateStreamFactory.Event = .didEnterBackground,
        resumeEvent: ApplicationStateStreamFactory.Event = .willEnterForeground
    ) {
        self.engine = engine
        self.pauseEvent = pauseEvent
        self.resumeEvent = resumeEvent
    }

    public func start() {
        guard lifecycleTask == nil else { return }

        let pauseStream = applicationStateStreamFactory.stream(for: pauseEvent)
        let resumeStream = applicationStateStreamFactory.stream(for: resumeEvent)

        lifecycleTask = Task { [weak self] in
            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    for await _ in pauseStream {
                        guard let self, await self.engine.getState() == .ready else { continue }
                        _ = try? await self.engine.evaluate("window.__pauseConnections__?.()")
                    }
                }
                group.addTask {
                    for await _ in resumeStream {
                        guard let self, await self.engine.getState() == .ready else { continue }
                        _ = try? await self.engine.evaluate("window.__resumeConnections__?.()")
                    }
                }
            }
        }
    }

    public func stop() {
        lifecycleTask?.cancel()
        lifecycleTask = nil
    }
}

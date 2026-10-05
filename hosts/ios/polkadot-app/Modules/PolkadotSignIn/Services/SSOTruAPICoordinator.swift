import Foundation
import CommonService
import UIKitExt
import TrUAPIHost

final class SSOTruAPICoordinator: @unchecked Sendable {
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let logger: LoggerProtocol
    private let sessions = Sessions()
    private var observer: NSObjectProtocol?

    init(runtimeProvider: TrUAPIHostRuntimeProviding, logger: LoggerProtocol = Logger.shared) {
        self.runtimeProvider = runtimeProvider
        self.logger = logger
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }
}

extension SSOTruAPICoordinator: MessageExchangeSignInHostCoordinating {
    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        runtimeProvider.setPresentationView(view)
    }

    func setup() async {
        observer = NotificationCenter.default.addObserver(forName: .truapiPairedHostsChanged, object: nil, queue: nil) { [weak self] _ in
            Task { await self?.resumeHosts() }
        }
        await resumeHosts()
    }

    func throttle() async {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
        await sessions.stop()
    }

    func disconnectHost(byAccountId accountId: Data) async throws {
        let runtime = try await runtimeProvider.sharedRuntime()
        guard let host = try await runtime.pairedHosts().first(where: { $0.peerStatement == accountId }) else { return }
        await sessions.stop(peer: host.peerEncryption)
        try await runtime.removePairedHost(peerStatement: host.peerStatement, peerEncryption: host.peerEncryption)
        NotificationCenter.default.post(name: .truapiPairedHostsChanged, object: nil)
    }

    private func resumeHosts() async {
        do {
            guard let runtime = try await runtimeProvider.activeRuntimeForRecords() else {
                await sessions.stop()
                return
            }
            let hosts = try await runtime.pairedHosts()
            await sessions.reconcile(hosts: hosts, runtime: runtime, logger: logger)
        } catch {
            logger.error("Rust SSO startup failed: \(error)")
        }
    }
}

private extension SSOTruAPICoordinator {
    actor Sessions {
        private struct SessionTask { let id: UUID; let task: Task<Void, Never> }
        private var tasks = [Data: SessionTask]()
        private var activeRuntime: TrUAPIHostRuntime?

        func reconcile(hosts: [PairedHostRecord], runtime: TrUAPIHostRuntime, logger: LoggerProtocol) {
            if activeRuntime !== runtime {
                stop()
                activeRuntime = runtime
            }
            let peers = Set(hosts.map(\.peerEncryption))
            for peer in tasks.keys where !peers.contains(peer) { stop(peer: peer) }
            for host in hosts { resume(host: host, runtime: runtime, logger: logger) }
        }

        func resume(host: PairedHostRecord, runtime: TrUAPIHostRuntime, logger: LoggerProtocol) {
            guard tasks[host.peerEncryption] == nil else { return }
            let identifier = UUID()
            let task = Task {
                defer { if tasks[host.peerEncryption]?.id == identifier { tasks[host.peerEncryption] = nil } }
                let peer = PairedSsoPeer(statementAccountId: host.peerStatement, encryptionPublicKey: host.peerEncryption)
                while !Task.isCancelled {
                    do {
                        let result = try await runtime.resumePairing(peer: peer)
                        if result == .peerDisconnected {
                            try await runtime.removePairedHost(peerStatement: host.peerStatement, peerEncryption: host.peerEncryption)
                            NotificationCenter.default.post(name: .truapiPairedHostsChanged, object: nil)
                            break
                        }
                        try await Task.sleep(for: .seconds(1))
                    } catch is CancellationError {
                        break
                    } catch {
                        logger.error("Rust SSO session failed: \(error)")
                        do { try await Task.sleep(for: .seconds(1)) } catch { break }
                    }
                }
            }
            tasks[host.peerEncryption] = SessionTask(id: identifier, task: task)
        }

        func stop(peer: Data) {
            tasks.removeValue(forKey: peer)?.task.cancel()
        }

        func stop() {
            activeRuntime = nil
            tasks.values.forEach { $0.task.cancel() }
            tasks.removeAll()
        }
    }
}

extension Notification.Name {
    static let truapiPairedHostsChanged = Notification.Name("truapiPairedHostsChanged")
}

import Foundation
import Combine
import AsyncExtensions
import AsyncAlgorithms
import KeyDerivation
import os
import CommonService
import EventCenter

protocol IdentityProfileServiceProtocol: AnyObject {
    func observe() -> AnyAsyncSequence<IdentityProfile>
}

final class IdentityProfileService {
    private struct UsernameState: Equatable {
        let username: Username?
        let isClaimed: Bool
    }

    private let usernameStorage: UsernameStoring
    private let identityService: IdentityServiceProtocol
    private let wallet: WalletManaging
    private let eventCenter: EventCenterProtocol
    private let logger: LoggerProtocol

    private let profileSubject: AsyncCurrentValueSubject<IdentityProfile>
    private let lock = OSAllocatedUnfairLock()

    private var usernameContinuation: AsyncStream<UsernameState>.Continuation?
    private var coordinatorTask: Task<Void, Never>?
    private var usernameTask: Task<Void, Never>?
    private var started = false

    init(
        usernameStorage: UsernameStoring,
        identityService: IdentityServiceProtocol,
        wallet: WalletManaging,
        eventCenter: EventCenterProtocol = EventCenter.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.usernameStorage = usernameStorage
        self.identityService = identityService
        self.wallet = wallet
        self.eventCenter = eventCenter
        self.logger = logger

        let initialProfile = IdentityProfile(
            username: usernameStorage.username,
            isClaimed: usernameStorage.usernameClaimed,
            rank: .basic
        )
        profileSubject = AsyncCurrentValueSubject<IdentityProfile>(initialProfile)
    }

    deinit {
        coordinatorTask?.cancel()
        usernameTask?.cancel()
        usernameContinuation?.finish()
    }
}

extension IdentityProfileService: IdentityProfileServiceProtocol {
    func observe() -> AnyAsyncSequence<IdentityProfile> {
        startIfNeeded()
        return profileSubject.removeDuplicates().eraseToAnyAsyncSequence()
    }
}

extension IdentityProfileService: AppEventVisiting {
    func processSelectedUsernameChanged(event _: SelectedUsernameChanged) {
        refreshUsername()
    }
}

private extension IdentityProfileService {
    func startIfNeeded() {
        lock.lock()
        let needsStart = !started
        started = true
        lock.unlock()
        guard needsStart else { return }

        eventCenter.add(observer: self)

        let (usernameStream, continuation) = AsyncStream.makeStream(of: UsernameState.self)
        usernameContinuation = continuation
        refreshUsername()

        coordinatorTask = Task { [profileSubject, weak self] in
            for await state in usernameStream {
                let profile = IdentityProfile(
                    username: state.username,
                    isClaimed: state.isClaimed,
                    rank: .basic
                )
                profileSubject.send(profile)
                self?.ensureOnChainSubscription(state: state)
            }
        }
    }
}

private extension IdentityProfileService {
    func refreshUsername() {
        let state = UsernameState(
            username: usernameStorage.username,
            isClaimed: usernameStorage.usernameClaimed
        )
        usernameContinuation?.yield(state)
    }

    private func ensureOnChainSubscription(state: UsernameState) {
        lock.lock()
        defer { lock.unlock() }

        if state.username != nil, !state.isClaimed {
            guard usernameTask == nil else { return }
            usernameTask = subscribeUsernameOnChain()
        } else {
            usernameTask?.cancel()
            usernameTask = nil
        }
    }

    func subscribeUsernameOnChain() -> Task<Void, Never>? {
        guard let accountId = try? wallet.getRawPublicKey() else {
            return nil
        }
        let publisher = identityService.subscribe(to: accountId)
        return Task { [logger, weak self] in
            do {
                for try await username in publisher.values.compactMap({ $0 }) {
                    self?.handleClaimed(username: username)
                    break
                }
            } catch is CancellationError {
                return
            } catch {
                logger.error("Username subscription failed: \(error)")
            }
        }
    }

    func handleClaimed(username: Username) {
        usernameStorage.username = username
        usernameStorage.usernameClaimed = true
        refreshUsername()
    }
}

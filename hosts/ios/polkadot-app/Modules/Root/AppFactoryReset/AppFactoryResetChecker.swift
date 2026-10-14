#if TESTNET_FEATURE
    import Combine
    import Foundation
    import KeyDerivation

    final class AppFactoryResetChecker {
        private let storage: UsernameStoring
        private let walletRepo: WalletManagerRepositoryProtocol
        private let identityService: IdentityServiceProtocol
        private var cancellable: AnyCancellable?

        init(
            storage: UsernameStoring,
            walletRepo: WalletManagerRepositoryProtocol,
            identityService: IdentityServiceProtocol
        ) {
            self.storage = storage
            self.walletRepo = walletRepo
            self.identityService = identityService
        }

        func checkIfResetNeeded(completion: @escaping (Bool) -> Void) {
            guard storage.usernameClaimed else {
                completion(false)
                return
            }

            guard let accountId = try? walletRepo.main().getRawPublicKey() else {
                completion(false)
                return
            }

            cancellable = identityService.username(for: accountId)
                .receive(on: DispatchQueue.main)
                .sink(
                    receiveCompletion: { result in
                        if case .failure = result {
                            completion(false)
                        }
                    },
                    receiveValue: { username in
                        completion(username == nil)
                    }
                )
        }
    }
#endif

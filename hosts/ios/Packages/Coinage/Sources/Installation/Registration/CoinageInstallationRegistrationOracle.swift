import DurableTransactions
import Foundation
import SDKLogger
import SubstrateSdk

/// Sound as a monotone oracle: the contract only ever appends, only this seed's data store account
/// writes its list, and the registrar keeps at most one attempt per target in flight — so a present
/// record is evidence about that attempt alone. The contract is read from the attempt's own group,
/// never from config that may have moved since.
public enum CoinageInstallationRegistrationOracle {
    public static func make(
        chainId: ChainId,
        dataStoreRepository: any AccountDataStoreRepositoryProtocol,
        logger: (any SDKLoggerProtocol)?
    ) -> MonotoneEffectOracle {
        MonotoneEffectOracle(chainId: chainId) { transactions, at in
            let targets = transactions.compactMap { transaction in
                transaction.groupId
                    .flatMap(InstallationRegistrationTarget.init(groupId:))
                    .map { (id: transaction.id, target: $0) }
            }

            var registeredByContract: [Data: Set<CoinageInstallationId>] = [:]
            for contract in Set(targets.map(\.target.contract)) {
                do {
                    registeredByContract[contract] = try await dataStoreRepository.fetchRegisteredInstallations(
                        contract: contract,
                        at: at.hash
                    )
                } catch {
                    logger?.warning("Installation registration oracle: read at \(at.number) failed: \(error)")
                }
            }

            return targets.reduce(into: [:]) { effects, entry in
                guard let registered = registeredByContract[entry.target.contract] else { return }
                effects[entry.id] = registered.contains(entry.target.installation)
            }
        }
    }
}

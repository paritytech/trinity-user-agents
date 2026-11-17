import struct TrUAPIHost.ProductId
import Foundation
import ChainRegistry
import Individuality
import KeyDerivation
import Products
import SubstrateSdk
import SubstrateSdkExt

enum BuiltInPersonhoodKeyError: Error, Equatable {
    case keyNotRegistered
    case keyNotInRing
    case peopleChainUnavailable
}

protocol BuiltInPersonhoodKeysProviding: Sendable {
    func entries(owner: ProductId) async throws -> [RegisteredRingVrfKey]
    func resolve(handle: RingVrfKeyHandle, ring: RingLocation) throws -> BandersnatchKeyManaging
}

enum BuiltInPersonhoodSlot: UInt32, CaseIterable {
    case fullPerson = 0
    case litePerson = 1

    var collectionId: MembersPallet.CollectionIdentifier {
        switch self {
        case .fullPerson: PeoplePallet.membersIdentifier
        case .litePerson: PeopleLitePallet.membersIdentifier
        }
    }
}

final class BuiltInPersonhoodKeys: @unchecked Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let peopleChainId: ChainModel.Id
    private let depsResolver: MembershipDepsResolving
    private let tldProvider: DotNsTldProviding
    private let keyManagerFactory: BuiltInPersonhoodKeyManagerMaking

    init(
        chainRegistry: ChainRegistryProtocol,
        peopleChainId: ChainModel.Id,
        depsResolver: MembershipDepsResolving,
        tldProvider: DotNsTldProviding,
        keyManagerFactory: BuiltInPersonhoodKeyManagerMaking
    ) {
        self.chainRegistry = chainRegistry
        self.peopleChainId = peopleChainId
        self.depsResolver = depsResolver
        self.tldProvider = tldProvider
        self.keyManagerFactory = keyManagerFactory
    }
}

extension BuiltInPersonhoodKeys: BuiltInPersonhoodKeysProviding {
    func entries(owner: ProductId) async throws -> [RegisteredRingVrfKey] {
        let tld = try tldProvider.currentTldOrError()
        let personhoodProductId = try BuiltInProduct.personhood(for: tld)

        guard owner == personhoodProductId else {
            return []
        }

        let genesisHash = try peopleGenesisHash()

        guard let palletIndex = try await depsResolver.membersPalletIndex(genesisHash: genesisHash) else {
            throw BuiltInPersonhoodKeyError.peopleChainUnavailable
        }

        let chainId = try genesisHash.fromHex()

        return BuiltInPersonhoodSlot.allCases.map { slot in
            RegisteredRingVrfKey(
                handle: RingVrfKeyHandle(productId: personhoodProductId, index: .index(slot.rawValue)),
                rings: [
                    RingLocation(
                        chainId: chainId,
                        junctions: [.palletInstance(palletIndex), .collectionId(slot.collectionId)]
                    )
                ],
                publicKey: nil
            )
        }
    }

    func resolve(handle: RingVrfKeyHandle, ring: RingLocation) throws -> BandersnatchKeyManaging {
        let tld = try tldProvider.currentTldOrError()

        guard
            try handle.productId == BuiltInProduct.personhood(for: tld),
            case let .index(index) = handle.index,
            let slot = BuiltInPersonhoodSlot(rawValue: index)
        else {
            throw BuiltInPersonhoodKeyError.keyNotRegistered
        }

        let genesisHash = try peopleGenesisHash()

        guard ring.chainId.toHex() == genesisHash, ring.collectionId == slot.collectionId else {
            throw BuiltInPersonhoodKeyError.keyNotInRing
        }

        return try keyManagerFactory.makeKeyManager(for: slot, tld: tld)
    }
}

private extension BuiltInPersonhoodKeys {
    func peopleGenesisHash() throws -> String {
        try chainRegistry.getChainOrError(for: peopleChainId).genesisHash.withoutHexPrefix().lowercased()
    }
}

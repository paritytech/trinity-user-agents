import Testing
@testable import Coinage

struct OnChainCoinReadTests {
    private let appInstance: CoinageInstanceId = 7

    @Test func coinOfAppInstanceReadsPresent() {
        let coin: CoinSyncResult.OnChainCoin? = makeCoin(instance: appInstance)

        let read = coin.read(in: appInstance)

        #expect(read == .present(makeCoin(instance: appInstance)))
        #expect(read.coin == makeCoin(instance: appInstance))
    }

    @Test func coinOfAnotherInstanceReadsForeignAndProjectsToAbsent() {
        let coin: CoinSyncResult.OnChainCoin? = makeCoin(instance: appInstance + 1)

        let read = coin.read(in: appInstance)

        #expect(read == .foreign(appInstance + 1))
        #expect(read.coin == nil)
    }

    @Test func missingValueReadsAbsent() {
        let coin: CoinSyncResult.OnChainCoin? = nil

        let read = coin.read(in: appInstance)

        #expect(read == .absent)
        #expect(read.coin == nil)
    }
}

private extension OnChainCoinReadTests {
    func makeCoin(instance: CoinageInstanceId) -> CoinSyncResult.OnChainCoin {
        CoinSyncResult.OnChainCoin(instanceId: instance, value: 3, age: 2)
    }
}

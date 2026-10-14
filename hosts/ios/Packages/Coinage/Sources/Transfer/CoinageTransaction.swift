import Foundation

/// The assets one durability entry consumes and mints, plus the coins handed to a peer.
///
/// `outputs` is the derivation-ref view used to register the entry; `outputCoins` is the same set
/// as full rows, used both to persist the projected mint and to build the on-chain call.
struct CoinageTransactionAssets {
    let inputs: [CoinageTxInput]
    let outputs: [OwnAsset]
    let outputCoins: [Coin]
    let handedOff: [Coin]
}

/// Collects what one coinage transaction consumes, mints, and hands off, so a strategy declares its
/// shape once ("consume this, mint that, hand these over") instead of assembling parallel input,
/// output, and handoff lists by hand and keeping them in step.
///
/// Minting itself is performed separately through a ``CoinMinting`` — this only records the minted
/// coins as outputs. It writes no status: the ledger records the transaction, and until it does
/// nothing here has been committed to. The output rows are persisted by the minter as they are minted.
struct CoinageTransaction {
    private var inputs: [CoinageTxInput] = []
    private var outputCoins: [Coin] = []
    private var handedOff: [Coin] = []

    /// Declare freshly minted coins as outputs of this transaction.
    mutating func mint(coins: [Coin]) {
        outputCoins += coins
    }

    /// Consume coins this wallet minted, addressed by derivation index and public key.
    mutating func consume(coins: [Coin]) {
        inputs += coins.map { .coin(.own($0.derivationIndex, $0.publicKey)) }
    }

    /// Record coins as given to a peer.
    mutating func handOff(coins: [Coin]) {
        handedOff += coins
    }

    func build() -> CoinageTransactionAssets {
        CoinageTransactionAssets(
            inputs: inputs,
            outputs: outputCoins.map { .coin($0.derivationIndex, $0.publicKey) },
            outputCoins: outputCoins,
            handedOff: handedOff
        )
    }
}

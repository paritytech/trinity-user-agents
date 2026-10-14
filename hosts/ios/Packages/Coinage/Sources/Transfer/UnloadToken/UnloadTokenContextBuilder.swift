import Foundation

enum UnloadTokenContextBuilder {
    /// Builds the unload token context for a given period and counter.
    /// Format: `"pop:polkadot.net/coinftk" ++ period(LE u32) ++ counter(LE u32)`
    static func freeUnloadTokenContext(period: UInt32, counter: UInt32) -> Data {
        var context = Data("pop:polkadot.net/coinftk".utf8)
        withUnsafeBytes(of: period.littleEndian) { context.append(contentsOf: $0) }
        withUnsafeBytes(of: counter.littleEndian) { context.append(contentsOf: $0) }
        return context
    }

    /// Context for recycler alias proofs.
    static var recyclerAliasContext: Data {
        Data("pop:polkadot.network/coinrecyclr".utf8)
    }
}

import Foundation
@testable import Coinage

/// A small, deterministic PRNG (SplitMix64), so a walk is a pure function of its seed and replays
/// identically. Its exact sequence need not match any other platform's — only be stable here.
struct SplitMix64: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) { state = seed }

    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}

/// What a walk can do.
enum FuzzAction: Equatable {
    case produceBlock
    case finalizeToBest
    case runPass
    /// A pass with one read failing throughout it, so every evidence path has an unknown to handle.
    case runPassWithFault(FuzzFault)
    case reorg(depth: Int)
    case registerSpend(coin: CoinageKeyIndex)
    /// One voucher in, one coin out.
    case registerUnload(voucher: CoinageKeyIndex)
    /// Several vouchers of one ring in, one coin out.
    case registerMultiUnload(first: CoinageKeyIndex, second: CoinageKeyIndex)
    /// One coin in, nothing trackable out.
    case registerOffboard(coin: CoinageKeyIndex)
    /// One coin in, two coins out.
    case registerSplit(coin: CoinageKeyIndex)
    case registerVoucherMint(coin: CoinageKeyIndex, voucher: CoinageKeyIndex)
    /// Nothing in, one voucher out.
    case registerExternalLoad(voucher: CoinageKeyIndex)
    case handOff(coin: CoinageKeyIndex)
    case peerSpends(coin: CoinageKeyIndex)
    case registerBatch(first: CoinageKeyIndex, second: CoinageKeyIndex)
    case crash
    case relaunch
    case archiveRecycler(voucher: CoinageKeyIndex)
    /// The queued voucher takes a place in a ring, which is what makes it unloadable.
    case placeVoucherInRing(voucher: CoinageKeyIndex, ring: Int)
    /// Identified by the entry's sequence rather than its id, so a recorded walk survives replay
    /// (the store mints a fresh random id each run, but sequence is deterministic).
    case includeTx(sequence: Int64, success: Bool)

    var kind: FuzzActionKind {
        switch self {
        case .produceBlock: .produceBlock
        case .finalizeToBest: .finalize
        case .runPass: .runPass
        case .runPassWithFault: .faultyPass
        case .reorg: .reorg
        case .registerSpend: .registerSpend
        case .registerUnload: .registerUnload
        case .registerMultiUnload: .registerMultiUnload
        case .registerOffboard: .registerOffboard
        case .registerSplit: .registerSplit
        case .registerVoucherMint: .mintVoucher
        case .registerExternalLoad: .externalLoad
        case .handOff: .handOff
        case .peerSpends: .peerSpends
        case .registerBatch: .registerBatch
        case .crash: .crash
        case .relaunch: .relaunch
        case .archiveRecycler: .archive
        case .placeVoucherInRing: .placeInRing
        case .includeTx: .includeTx
        }
    }
}

/// A stable, ordered set of action kinds. `Int` raw values give a deterministic iteration order —
/// essential, since `Dictionary` iteration in Swift is randomized and profile weighting must not be.
enum FuzzActionKind: Int, CaseIterable {
    case produceBlock
    case finalize
    case reorg
    case runPass
    case faultyPass
    case registerSpend
    case registerUnload
    case registerMultiUnload
    case registerOffboard
    case registerSplit
    case mintVoucher
    case externalLoad
    case archive
    case placeInRing
    case includeTx
    case handOff
    case peerSpends
    case registerBatch
    case crash
    case relaunch
}

/// How often a walk takes each direction, so one profile can hammer spending and another chain
/// instability. Weighting is per kind rather than per enabled action: at any moment there may be a
/// dozen coins to spend and one legal reorg depth, and weighting the flat list would let whichever
/// kind happens to have the most instances dominate. A kind is chosen first, then one of its actions
/// uniformly.
struct FuzzProfile {
    let name: String
    private let weights: [FuzzActionKind: Int]

    init(name: String, weights: [FuzzActionKind: Int]) {
        self.name = name
        self.weights = weights
    }

    func pick(_ rng: inout SplitMix64, from enabled: [FuzzAction]) -> FuzzAction? {
        let kinds = FuzzActionKind.allCases.filter { kind in
            weightOf(kind) > 0 && enabled.contains { $0.kind == kind }
        }
        guard !kinds.isEmpty else { return nil }

        let total = kinds.reduce(0) { $0 + weightOf($1) }
        var roll = Int.random(in: 0 ..< total, using: &rng)
        let kind = kinds.first { candidate in
            roll -= weightOf(candidate)
            return roll < 0
        } ?? kinds[kinds.count - 1]

        let choices = enabled.filter { $0.kind == kind }
        return choices[Int.random(in: 0 ..< choices.count, using: &rng)]
    }

    private func weightOf(_ kind: FuzzActionKind) -> Int { weights[kind] ?? 0 }
}

extension FuzzProfile {
    private static let ordinary = 10
    /// A faulty pass mostly withholds a verdict, so at `ordinary` it roughly halves how much a walk
    /// decides. Rare in the everyday mix; `flakyReads` is where it is the subject.
    private static let rareFaults = 3

    /// Weights are overrides on a base where every direction is `ordinary`, so a newly added kind is
    /// exercised everywhere by default rather than silently sitting at zero.
    private static func profile(_ name: String, _ overrides: [FuzzActionKind: Int]) -> FuzzProfile {
        var weights: [FuzzActionKind: Int] = [:]
        for kind in FuzzActionKind.allCases {
            weights[kind] = ordinary
        }
        weights[.faultyPass] = rareFaults
        for (kind, weight) in overrides {
            weights[kind] = weight
        }
        return FuzzProfile(name: name, weights: weights)
    }

    /// The everyday mix. Restarts and archival are deliberately rare: both reset or destroy a lot of
    /// state, and at ordinary weight they stop a walk building anything deep.
    static let balanced = profile("balanced", [.crash: 1, .relaunch: 1, .archive: 1])

    static let heavySpending = profile("heavy spending", [
        .registerSpend: 30, .includeTx: 30, .registerBatch: 20, .runPass: 15,
        .reorg: 2, .archive: 1, .crash: 1, .relaunch: 1,
    ])

    static let reorgStorm = profile("reorg storm", [
        .reorg: 30, .produceBlock: 30, .finalize: 3, .runPass: 20,
        .includeTx: 15, .archive: 1, .crash: 1, .relaunch: 1,
    ])

    static let voucherChurn = profile("voucher churn", [
        .registerUnload: 30, .mintVoucher: 25, .externalLoad: 25, .includeTx: 20,
        .registerSpend: 3, .archive: 5, .crash: 1, .relaunch: 1,
    ])

    static let archivalChurn = profile("archival churn", [
        .archive: 30, .registerUnload: 20, .externalLoad: 20, .includeTx: 15,
        .reorg: 15, .registerSpend: 3, .crash: 1, .relaunch: 1,
    ])

    static let restartChurn = profile("restart churn", [
        .crash: 25, .relaunch: 25, .runPass: 20, .registerSpend: 15,
        .includeTx: 15, .handOff: 10, .archive: 1,
    ])

    static let handoffChurn = profile("handoff churn", [
        .handOff: 30, .peerSpends: 25, .registerSpend: 20, .includeTx: 20,
        .runPass: 15, .archive: 1, .crash: 2, .relaunch: 2,
    ])

    static let flakyReads = profile("flaky reads", [
        .faultyPass: 30, .runPass: 15, .includeTx: 15, .registerSpend: 10,
        .archive: 1, .crash: 1, .relaunch: 1,
    ])

    static let all: [FuzzProfile] = [
        balanced, heavySpending, reorgStorm, voucherChurn, archivalChurn, restartChurn, handoffChurn, flakyReads,
    ]
}

/// A broken invariant, carrying the trace that reached it so the failure can be shrunk and printed.
struct FuzzViolation: Error {
    let message: String
    let trace: [FuzzAction]
}

/// A candidate the shrinker proposed that is not a legal history, so it says nothing about the
/// violation and the shrinker must keep the action it tried to drop.
struct ReplayDiverged: Error {}

extension [FuzzAction] {
    /// Prints a shrunk walk as harness calls, so a fuzz failure graduates into a named test.
    func asHarnessCalls() -> String {
        map { action in
            switch action {
            case .produceBlock: "advanceBlocks(1, finality: .inBest)"
            case .finalizeToBest: "finalizeToBest()"
            case .runPass: "await runPass()"
            case let .runPassWithFault(fault): "await runPass(withFault: .\(fault))"
            case let .reorg(depth): "reorgLastBlocks(\(depth))"
            case let .registerSpend(coin): "try await register(inputCoin: \(coin), outputCoin: ?)"
            case let .registerUnload(voucher): "try await registerVoucherUnload(vouchers: [\(voucher)], outputCoin: ?)"
            case let .registerMultiUnload(first, second):
                "try await registerVoucherUnload(vouchers: [\(first), \(second)], outputCoin: ?)"
            case let .registerOffboard(coin): "try await registerOffboard(inputCoin: \(coin))"
            case let .registerSplit(coin): "try await registerSplit(inputCoin: \(coin), outputCoins: ?)"
            case let .registerVoucherMint(coin, voucher):
                "try await registerVoucherMint(inputCoin: \(coin), voucher: \(voucher))"
            case let .registerExternalLoad(voucher): "try await registerExternalLoad(voucher: \(voucher))"
            case let .handOff(coin): "await handOff([coinOutput(\(coin))])"
            case let .peerSpends(coin): "consumeCoinOnChain(\(coin), finality: .inBest)"
            case let .registerBatch(first, second): "try await registerGroup(pairs: [(\(first), ?), (\(second), ?)])"
            case .crash: "crash()"
            case .relaunch: "try await relaunch()"
            case let .archiveRecycler(voucher): "archiveRecyclerOf(\(voucher), finality: .inBest)"
            case let .placeVoucherInRing(voucher, ring): "placeVoucherInRing(\(voucher), ring: \(ring), finality: .inBest)"
            case let .includeTx(sequence, success): "includeEntry(seq: \(sequence), success: \(success), finality: .inBest)"
            }
        }
        .map { "        \($0)" }
        .joined(separator: "\n")
    }
}

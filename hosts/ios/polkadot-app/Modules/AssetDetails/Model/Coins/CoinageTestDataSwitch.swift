#if DEBUG
    import DesignSystem
    import PolkadotUI
    import SwiftUI

    /// Which set of holdings the coins draw. Scaffolding for reviewing the depiction against
    /// wallets the app cannot easily be put into, shown with the other debug features only.
    enum CoinageTestDataMode: String, CaseIterable, Identifiable {
        case live
        case everyDenomination
        case everyStatus
        case randomFifty
        case randomFiveHundred

        var id: String { rawValue }

        var title: String {
            switch self {
            case .live: "Live"
            case .everyDenomination: "All values"
            case .everyStatus: "All states"
            case .randomFifty: "50"
            case .randomFiveHundred: "500"
            }
        }

        /// The breakdown with its coins swapped for the test set, or untouched for `live`. The
        /// figures above the coins stay the wallet's own; only the coins and their traces change.
        func applied(to breakdown: CoinageBalanceBreakdownViewModel) -> CoinageBalanceBreakdownViewModel {
            guard let coins = strip else { return breakdown }

            return CoinageBalanceBreakdownViewModel(
                totalBalance: breakdown.totalBalance,
                availableNowBalance: breakdown.availableNowBalance,
                gainingPrivacyBalance: breakdown.gainingPrivacyBalance,
                symbol: breakdown.symbol,
                strip: coins,
                traces: CoinageTestData.traces(for: coins)
            )
        }

        /// `nil` draws whatever the wallet actually holds. Ordered the way the live path orders,
        /// so the test sets exercise the same arrangement rather than a shape the app never
        /// produces.
        private var strip: [CoinageScene.Coin]? {
            let coins: [CoinageScene.Coin]? =
                switch self {
                case .live: nil
                case .everyDenomination: CoinageTestData.everyDenomination()
                case .everyStatus: CoinageTestData.everyStatus()
                case .randomFifty: CoinageTestData.random(count: 50, seed: 50)
                case .randomFiveHundred: CoinageTestData.random(count: 500, seed: 500)
                }

            return coins.map { CoinageBreakdownFactory.inDisplayOrder($0) }
        }
    }

    /// The radio row itself.
    struct CoinageTestDataSwitch: View {
        @Binding var mode: CoinageTestDataMode

        var body: some View {
            HStack(spacing: DSSpacings.small) {
                ForEach(CoinageTestDataMode.allCases) { option in
                    Button {
                        mode = option
                    } label: {
                        HStack(spacing: DSSpacings.extraTiny) {
                            Image(systemName: mode == option ? "largecircle.fill.circle" : "circle")
                                .font(.system(size: 11))

                            Text(option.title)
                                .typography(.bodySmall)
                        }
                        .foregroundStyle(mode == option ? Color.fgPrimary : Color.fgSecondary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// Plain or varied coins, side by side: plain ships, varied is kept to compare against.
    struct CoinageCoinSetSwitch: View {
        @Binding var set: CoinageScene.CoinSet

        var body: some View {
            HStack(spacing: DSSpacings.small) {
                Text(verbatim: "Coins")
                    .typography(.bodySmall)
                    .foregroundStyle(Color.fgPrimary)

                Picker(selection: $set) {
                    Text(verbatim: "Plain").tag(CoinageScene.CoinSet.plain)
                    Text(verbatim: "Varied").tag(CoinageScene.CoinSet.varied)
                } label: {
                    Text(verbatim: "Coins")
                }
                .pickerStyle(.segmented)
            }
        }
    }

    // MARK: - Generators

    enum CoinageTestData {
        private static let exponents: ClosedRange<Int16> = 0 ... CoinageCoinDesign.highestExponent

        /// One of every denomination, all as untraceable as a coin can be, so the switch shows the
        /// fifteen designs with nothing on top of them.
        static func everyDenomination() -> [CoinageScene.Coin] {
            exponents.map { coin(id: "test-value-\($0)", exponent: $0, level: CoinageWear.maximumLevel) }
        }

        /// Twenty holdings of one denomination across the whole wear ladder. A single value keeps
        /// metal and outline fixed, so the only thing changing is how traceable the holding is.
        static func everyStatus() -> [CoinageScene.Coin] {
            let exponent: Int16 = 4
            let count = 20

            return (0 ..< count).map { index in
                coin(
                    id: "test-state-\(index)",
                    exponent: exponent,
                    level: CoinageWear.maximumLevel - index * CoinageWear.maximumLevel / (count - 1),
                    isReady: index % 4 != 0,
                    hops: index % 5
                )
            }
        }

        /// A wallet of no particular shape, for judging how the coins read in bulk. Seeded, so a
        /// given size always draws the same wallet and a change to the depiction is the only thing
        /// that moves.
        static func random(count: Int, seed: Int) -> [CoinageScene.Coin] {
            var noise = CoinageNoise(seed: "test-random-\(seed)")

            return (0 ..< count).map { index in
                let exponent = min(Int16(noise.next() * CGFloat(exponents.count)), exponents.upperBound)
                let level = min(
                    Int(noise.next() * CGFloat(CoinageWear.maximumLevel + 1)),
                    CoinageWear.maximumLevel
                )

                return coin(
                    id: "test-random-\(seed)-\(index)",
                    exponent: exponent,
                    level: level,
                    isReady: noise.next() > 0.25,
                    hops: Int(noise.next() * 5)
                )
            }
        }

        /// A trace per test coin, so tapping one shows what the live card would: the value as the
        /// chain strikes it, the grade its wear level maps to, and one hop per pit.
        static func traces(for coins: [CoinageScene.Coin]) -> [String: CoinageCoinTrace] {
            Dictionary(uniqueKeysWithValues: coins.map { coin in
                (
                    coin.id,
                    CoinageCoinTrace(
                        value: String(format: "$%.2f", 0.01 * pow(2, Double(coin.exponent))),
                        partition: coin.partition,
                        hops: coin.hops,
                        quality: CoinageCoinTrace.Quality(score: UInt8(coin.level * 100 / CoinageWear.maximumLevel))
                    )
                )
            })
        }

        private static func coin(
            id: String,
            exponent: Int16,
            level: Int,
            isReady: Bool = true,
            hops: Int = 0
        ) -> CoinageScene.Coin {
            CoinageScene.Coin(
                id: id,
                exponent: exponent,
                wear: CoinageWear.amount(forLevel: level),
                partition: isReady ? .ready : .clearing,
                level: level,
                hops: hops
            )
        }
    }

    /// Deterministic scatter, so a given test set draws the same wallet every time.
    struct CoinageNoise {
        private var state: UInt64

        init(seed: String) {
            state = seed.unicodeScalars.reduce(UInt64(0x9E37_79B9_7F4A_7C15)) {
                ($0 &* 31) &+ UInt64($1.value)
            } | 1
        }

        mutating func next() -> CGFloat {
            state ^= state << 13
            state ^= state >> 7
            state ^= state << 17

            return CGFloat(state % 100_000) / 100_000
        }
    }

    /// A zeroed breakdown, so the coinage card still draws on a wallet that holds no coinage and
    /// the switch stays useful there.
    extension CoinageBalanceBreakdownViewModel {
        static let testDataPlaceholder = CoinageBalanceBreakdownViewModel(
            totalBalance: "0",
            availableNowBalance: "0",
            gainingPrivacyBalance: "0",
            symbol: "",
            strip: [],
            traces: [:]
        )
    }
#endif

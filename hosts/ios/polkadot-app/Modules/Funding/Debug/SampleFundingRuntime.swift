#if DEBUG
    import Foundation
    import TrUAPIHost

    /// A stand-in for the core's funding, so every funding screen can be walked
    /// through in the simulator without a provider product. Launch with
    /// `-FundingSample YES` to use it.
    ///
    /// Two providers: "sample-ramp.dot" serves card, bank and crypto, and
    /// "sample-cards.dot" serves card only at a slightly worse price. Sessions
    /// live in memory; a started one walks its progress steps on a timer.
    final class SampleFundingRuntime: FundingRuntime, @unchecked Sendable {
        static var isRequested: Bool {
            UserDefaults.standard.bool(forKey: "FundingSample")
        }

        /// The sample, once installed.
        static var installed: SampleFundingRuntime? {
            registry.lock()
            defer { registry.unlock() }
            return current
        }

        /// Installs the sample when the launch argument asks for it, and points
        /// the CASH card's lists at it.
        static func installIfRequested(environment: @escaping @MainActor () -> FundingOverlayEnvironment) {
            guard isRequested else { return }

            registry.lock()
            guard current == nil else {
                registry.unlock()
                return
            }
            let sample = SampleFundingRuntime(environment: environment)
            current = sample
            registry.unlock()

            Task { @MainActor in
                sample.seed(precision: FundingCash.current.precision)
                FundingActivityCenter.attach(runtime: sample)
            }
        }

        private static let registry = NSLock()
        private static var current: SampleFundingRuntime?

        fileprivate struct Entry {
            var session: FundingSession
            var progress: FundingProgress?
            var ask: FundingQuoteAsk?
            var quotes: [String: FundingQuote] = [:]
            var quoteRound = 0
        }

        fileprivate struct State {
            var entries: [String: Entry] = [:]
            var precision: Int16 = 6
        }

        private let lock = NSLock()
        private var state = State()
        private var overlay: AppFundingOverlay!

        private init(environment: @escaping @MainActor () -> FundingOverlayEnvironment) {
            overlay = AppFundingOverlay(runtime: self, environment: environment)
        }

        // MARK: - FundingRuntime

        func fundingSession(intent: String) -> FundingSession? {
            locked { $0.entries[intent]?.session }
        }

        func fundingProgress(intent: String) -> FundingProgress? {
            locked { $0.entries[intent]?.progress }
        }

        func fundingSessions() -> [FundingSession] {
            locked { $0.entries.values.map(\.session) }.sorted { $0.openedAtMs > $1.openedAtMs }
        }

        func fundingCandidates(intent _: String) -> [FundingCandidate] {
            let fiat = ["USD", "EUR", "GBP", "CHF"]
            let min = units(Self.minimum)
            let max = units(Self.maximum)

            let ramp = FundingCandidate(
                providerId: Self.rampId,
                routes: [
                    Self.route(.card, assets: fiat),
                    Self.route(.bank, assets: ["EUR", "GBP", "USD"], requiresAccount: true),
                    Self.route(.crypto, assets: ["USDT", "USDC"], networks: ["polkadot", "ethereum", "tron"])
                ],
                unsupported: [],
                limits: [
                    FundingLimit(rail: .card, asset: "USD", network: nil, min: min, max: max),
                    FundingLimit(rail: .bank, asset: "EUR", network: nil, min: min, max: max)
                ] + ["polkadot", "ethereum", "tron"].flatMap { network in
                    ["USDT", "USDC"]
                        .map { FundingLimit(rail: .crypto, asset: $0, network: network, min: min, max: max) }
                },
                backend: nil
            )
            let cards = FundingCandidate(
                providerId: Self.cardsId,
                routes: [Self.route(.card, assets: fiat)],
                unsupported: [],
                limits: [],
                backend: nil
            )
            return [ramp, cards]
        }

        func getFundingQuote(intent: String, ask: FundingQuoteAsk) {
            let providers = fundingCandidates(intent: intent)
                .filter { $0.serves(rail: ask.rail, direction: ask.direction) }
                .map(\.providerId)
            let generation: Int? = locked { state in
                guard state.entries[intent] != nil else { return nil }
                state.entries[intent]?.ask = ask
                state.entries[intent]?.quoteRound += 1
                return state.entries[intent]?.quoteRound
            }
            guard let generation else { return }

            for providerId in providers {
                overlay.fundingQuoteChanged(
                    intent: intent,
                    row: FundingQuoteRow(providerId: providerId, state: .pending)
                )
            }

            Task {
                try? await Task.sleep(for: .milliseconds(800))
                for providerId in providers {
                    let quoted = quoteState(providerId: providerId, ask: ask)
                    let current: Bool = locked { box in
                        guard box.entries[intent]?.quoteRound == generation else { return false }
                        if let quote = quoted.quote { box.entries[intent]?.quotes[providerId] = quote }
                        return true
                    }
                    guard current else { return }
                    overlay.fundingQuoteChanged(
                        intent: intent,
                        row: FundingQuoteRow(providerId: providerId, state: quoted)
                    )
                }
            }
        }

        func selectFundingProvider(intent: String, providerId: String, quoteId: String?) async throws -> Bool {
            let now = Self.nowMs
            let started: (FundingRail, FundingDirection)? = locked { state in
                guard var entry = state.entries[intent], entry.session.stage.isOpen, let ask = entry.ask,
                      let quote = entry.quotes[providerId],
                      quoteId == nil || quote.quoteId == quoteId else { return nil }

                entry.session.providerId = providerId
                entry.session.choice = FundingChoice(quote: quote, rail: ask.rail, asset: ask.asset)
                entry.progress = FundingProgress(
                    steps: Self.steps(direction: ask.direction).map {
                        FundingProgressStep(step: $0, reachedAtMs: $0 == .started ? now : nil)
                    },
                    failedAtMs: nil,
                    transactionId: nil,
                    reference: nil,
                    deposit: nil,
                    mismatch: nil,
                    retrying: false,
                    payout: nil
                )
                state.entries[intent] = entry
                return (ask.rail, ask.direction)
            }
            guard let started else { return false }

            notify(intent)
            Task { await walk(intent: intent, rail: started.0, direction: started.1) }
            return true
        }

        func cancelFunding(intent: String) async throws -> Bool {
            let cancelled: Bool = locked { state in
                guard var entry = state.entries[intent], entry.session.stage.isOpen else { return false }
                let now = Self.nowMs
                entry.session.cancelRequested = true
                entry.session.stage = .failed(reason: .cancelled, settledAtMs: now)
                entry.progress?.failedAtMs = now
                state.entries[intent] = entry
                return true
            }
            if cancelled { notify(intent) }
            return cancelled
        }

        func acknowledgeFundingSession(intent: String) async throws -> Bool {
            locked { $0.entries.removeValue(forKey: intent) != nil }
        }

        func openFunding(direction: FundingDirection, amount: U128?) async throws -> String? {
            let precision = await MainActor.run { FundingCash.current.precision }
            let now = Self.nowMs
            let intent = "sample-" + UUID().uuidString.prefix(8).lowercased()
            let session = FundingSession(
                intent: intent,
                ownerProductId: nil,
                direction: direction,
                amount: amount,
                stage: .open,
                openedAtMs: now,
                deadlineMs: now + 15 * 60 * 1_000,
                acknowledged: false,
                providerId: nil,
                cancelRequested: false,
                updates: [],
                choice: nil,
                saved: nil
            )
            locked { state in
                state.precision = precision
                state.entries[intent] = Entry(session: session)
            }

            let outcome = await overlay.presentFunding(
                productId: nil,
                intent: intent,
                direction: direction,
                amount: amount
            )
            guard outcome == .dismissed else { return intent }

            // Dismissed before a provider was chosen: the core drops the session.
            let dropped: Bool = locked { state in
                guard state.entries[intent]?.session.providerId == nil else { return false }
                state.entries[intent] = nil
                return true
            }
            return dropped ? nil : intent
        }
    }

    // MARK: - Progress

    private extension SampleFundingRuntime {
        static let rampId = "sample-ramp.dot"
        static let cardsId = "sample-cards.dot"
        static let minimum: Decimal = 10
        static let maximum: Decimal = 5_000
        static let stepPause: Duration = .seconds(3)

        static var nowMs: UInt64 {
            UInt64(Date().timeIntervalSince1970 * 1_000)
        }

        static func route(
            _ mode: FundingMode,
            assets: [String],
            networks: [String]? = nil,
            requiresAccount: Bool = false
        ) -> FundingRoute {
            FundingRoute(
                mode: mode,
                directions: [.in, .out],
                assets: assets,
                networks: networks,
                countries: nil,
                requiresAccount: requiresAccount
            )
        }

        static func steps(direction: FundingDirection) -> [FundingStep] {
            switch direction {
            case .in: [.started, .payment, .conversion, .added]
            case .out: [.started, .sent, .conversion]
            }
        }

        /// What a provider would report as the session moves: the deposit to pay
        /// first for bank and crypto in, then each step, then the end.
        func walk(intent: String, rail: FundingRail, direction: FundingDirection) async {
            if direction == .in, rail != .card {
                try? await Task.sleep(for: .seconds(1))
                guard update(intent, { entry in
                    entry.progress?.deposit = deposit(entry: entry, rail: rail)
                    if rail == .bank { entry.progress?.reference = Self.reference(intent: intent) }
                }) else { return }
                try? await Task.sleep(for: .seconds(10))
            }

            for step in Self.steps(direction: direction).dropFirst() {
                try? await Task.sleep(for: Self.stepPause)
                guard update(intent, { entry in
                    let now = Self.nowMs
                    entry.progress?.steps = entry.progress?.steps.map {
                        $0.step == step ? FundingProgressStep(step: step, reachedAtMs: now) : $0
                    } ?? []
                    if step == .payment || step == .sent {
                        entry.progress?.transactionId = "0x" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
                            .lowercased()
                    }
                }) else { return }
            }

            try? await Task.sleep(for: Self.stepPause)
            _ = update(intent) { entry in
                let now = Self.nowMs
                guard let quote = entry.session.choice?.quote else { return }
                switch direction {
                case .in:
                    entry.session.stage = .delivered(credited: quote.receiveAmount, settledAtMs: now)
                case .out:
                    entry.session.stage = .released(debited: quote.sendAmount, settledAtMs: now)
                    entry.progress?.payout = .paidOut
                }
            }
        }

        /// Applies `change` to an open session and tells the overlay. False once
        /// the session has ended or gone, which stops the walk.
        func update(_ intent: String, _ change: (inout Entry) -> Void) -> Bool {
            let applied: Bool = locked { state in
                guard var entry = state.entries[intent], entry.session.stage.isOpen else { return false }
                change(&entry)
                state.entries[intent] = entry
                return true
            }
            if applied { notify(intent) }
            return applied
        }

        func notify(_ intent: String) {
            guard let session = fundingSession(intent: intent) else { return }

            let status: HostFundingStatusSubscribeItem =
                switch session.stage {
                case .open: .inProgress(expiresAt: session.deadlineMs)
                case let .delivered(credited, _): .delivered(credited: credited)
                case let .released(debited, _): .released(debited: debited)
                case let .failed(reason, _): .failed(reason: reason, moved: "0")
                }
            overlay.fundingSessionChanged(intent: intent, status: status)
        }

        func deposit(entry: Entry, rail: FundingRail) -> FundingDeposit? {
            guard let ask = entry.ask, let quote = entry.session.choice?.quote else { return nil }

            let unit = FundingAssetUnit(code: ask.asset)
            let decimals = UInt8(clamping: unit.decimals)
            let expiresAt = Self.nowMs + 30 * 60 * 1_000

            switch rail {
            case .crypto:
                let network = ask.network ?? "polkadot"
                let address = Self.addresses[network] ?? Self.addresses["polkadot"]!
                let figure = NSDecimalNumber(decimal: unit.decimal(quote.sendAmount)).stringValue
                return .crypto(
                    address: address,
                    network: network,
                    asset: ask.asset,
                    amount: quote.sendAmount,
                    decimals: decimals,
                    exact: true,
                    uri: "\(network):\(address)?amount=\(figure)&asset=\(ask.asset)",
                    expiresAt: expiresAt
                )
            case .bank:
                return .bank(
                    amount: quote.sendAmount,
                    currency: ask.asset,
                    decimals: decimals,
                    beneficiary: "Sample Ramp Payments Ltd",
                    account: Self.ibans[ask.asset.uppercased()] ?? Self.ibans["EUR"],
                    bankCode: ask.asset.uppercased() == "GBP" ? "04-00-75" : "SMPLDEB1XXX",
                    reference: Self.reference(intent: entry.session.intent),
                    expiresAt: Self.nowMs + 3 * 24 * 60 * 60 * 1_000
                )
            case .card:
                return nil
            }
        }

        static func reference(intent: String) -> String {
            "SMPL-" + intent.suffix(6).uppercased()
        }

        static let addresses = [
            "polkadot": "14E5nqKAp3oAJcmzgZhUD2RcptBeUBScxKHgJKU4HPNcKVf3",
            "ethereum": "0x8F3cF7ad23Cd3CaDbD9735AFf958023239c6A063",
            "tron": "TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf"
        ]

        static let ibans = [
            "EUR": "DE89 3704 0044 0532 0130 00",
            "GBP": "GB33 BUKB 2020 1555 5555 55",
            "USD": "US64 SVBK US6S 3300 9673 8637"
        ]
    }

    // MARK: - Quotes

    private extension SampleFundingRuntime {
        /// Prices `ask` the way the provider would. Below 10 or above 5000 CASH is
        /// refused; the card-only provider charges a little more.
        func quoteState(providerId: String, ask: FundingQuoteAsk) -> FundingQuoteState {
            let cash = cashDecimal(ask.amount)
            if cash < Self.minimum {
                return .unavailable(reason: .refused(reason: .belowMinimum(min: units(Self.minimum))))
            }
            if cash > Self.maximum {
                return .unavailable(reason: .refused(reason: .aboveMaximum(max: units(Self.maximum))))
            }

            let providerFee = Self.providerFee(cash: cash, rail: ask.rail, worse: providerId == Self.cardsId)
            let network = Self.networkFees[ask.network ?? ""] ?? 0
            let rate = Self.rates[ask.asset.uppercased()] ?? 1
            let unit = FundingAssetUnit(code: ask.asset)

            let quote =
                switch ask.direction {
                case .in:
                    // Fees in what the user pays, on top of the CASH they receive.
                    FundingQuote(
                        quoteId: "q-" + UUID().uuidString.prefix(8).lowercased(),
                        sendAmount: Self.units((cash + providerFee + network) * rate, decimals: unit.decimals),
                        receiveAmount: ask.amount,
                        providerFee: Self.units(providerFee * rate, decimals: unit.decimals),
                        networkFee: Self.units(network * rate, decimals: unit.decimals),
                        etaSecs: Self.eta(ask.rail),
                        expiresAt: Self.nowMs + 60_000
                    )
                case .out:
                    // Fees in CASH, taken from what the user sends.
                    FundingQuote(
                        quoteId: "q-" + UUID().uuidString.prefix(8).lowercased(),
                        sendAmount: ask.amount,
                        receiveAmount: Self.units((cash - providerFee - network) * rate, decimals: unit.decimals),
                        providerFee: units(providerFee),
                        networkFee: units(network),
                        etaSecs: Self.eta(ask.rail),
                        expiresAt: Self.nowMs + 60_000
                    )
                }
            return .quoted(quote: quote)
        }

        static func providerFee(cash: Decimal, rail: FundingRail, worse: Bool) -> Decimal {
            switch rail {
            case .card: cash * (worse ? 0.029 : 0.019) + (worse ? 0.5 : 0.3)
            case .bank: cash * 0.005
            case .crypto: cash * 0.002
            }
        }

        static let networkFees: [String: Decimal] = ["ethereum": 2.4, "tron": 1, "polkadot": 0.05]

        static let rates: [String: Decimal] = ["USD": 1, "EUR": 0.92, "GBP": 0.79, "CHF": 0.88, "USDT": 1, "USDC": 1]

        static func eta(_ rail: FundingRail) -> UInt64 {
            switch rail {
            case .card: 120
            case .bank: 4 * 60 * 60
            case .crypto: 10 * 60
            }
        }

        func cashDecimal(_ units: U128) -> Decimal {
            (Decimal(string: units) ?? 0) / FundingCash.scale(locked { $0.precision })
        }

        func units(_ cash: Decimal) -> U128 {
            Self.units(cash, decimals: locked { $0.precision })
        }

        static func units(_ value: Decimal, decimals: Int16) -> U128 {
            var scaled = value * FundingCash.scale(decimals)
            var rounded = Decimal()
            NSDecimalRound(&rounded, &scaled, 0, .down)
            return NSDecimalNumber(decimal: max(rounded, 0)).stringValue
        }
    }

    // MARK: - State

    private extension SampleFundingRuntime {
        func locked<T>(_ body: (inout State) -> T) -> T {
            lock.lock()
            defer { lock.unlock() }

            return body(&state)
        }

        /// One ended session of each kind, so the CASH card's history has rows.
        func seed(precision: Int16) {
            let hour: UInt64 = 60 * 60 * 1_000
            let now = Self.nowMs
            locked { $0.precision = precision }

            func ended(
                _ name: String,
                _ direction: FundingDirection,
                _ rail: FundingRail,
                _ cash: Decimal,
                _ stage: (U128, UInt64) -> FundingStage,
                payout: FundingPayout? = nil,
                hoursAgo: UInt64
            ) -> Entry {
                let amount = units(cash)
                let settled = now - hoursAgo * hour
                let asset = rail == .crypto ? "USDT" : "EUR"
                let quote = FundingQuote(
                    quoteId: "q-seed-\(name)",
                    sendAmount: amount,
                    receiveAmount: amount,
                    providerFee: "0",
                    networkFee: "0",
                    etaSecs: Self.eta(rail),
                    expiresAt: nil
                )
                let session = FundingSession(
                    intent: "sample-seed-\(name)",
                    ownerProductId: nil,
                    direction: direction,
                    amount: amount,
                    stage: stage(amount, settled),
                    openedAtMs: settled - hour / 4,
                    deadlineMs: settled + hour,
                    acknowledged: false,
                    providerId: Self.rampId,
                    cancelRequested: false,
                    updates: [],
                    choice: FundingChoice(quote: quote, rail: rail, asset: asset),
                    saved: nil
                )
                let progress = FundingProgress(
                    steps: Self.steps(direction: direction).map { FundingProgressStep(step: $0, reachedAtMs: settled) },
                    failedAtMs: nil,
                    transactionId: "0xseed\(name)",
                    reference: "SMPL-\(name.prefix(6).uppercased())",
                    deposit: nil,
                    mismatch: nil,
                    retrying: false,
                    payout: payout
                )
                return Entry(session: session, progress: progress)
            }

            let seeds = [
                ended("topped-up", .in, .card, 50, { .delivered(credited: $0, settledAtMs: $1) }, hoursAgo: 2),
                ended(
                    "sent",
                    .out,
                    .bank,
                    120,
                    { .released(debited: $0, settledAtMs: $1) },
                    payout: .paidOut,
                    hoursAgo: 26
                ),
                ended("refunded", .in, .crypto, 75, { .failed(reason: .refunded, settledAtMs: $1) }, hoursAgo: 50),
                ended(
                    "payout-failed",
                    .out,
                    .card,
                    40,
                    { .released(debited: $0, settledAtMs: $1) },
                    payout: .failed(reason: "The card issuer declined the payout"),
                    hoursAgo: 74
                )
            ]
            locked { state in
                for seed in seeds {
                    state.entries[seed.session.intent] = seed
                }
            }
        }
    }
#endif

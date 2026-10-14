#!/usr/bin/env python3
"""Mutation sweep over the RFC-0006 top-up guards (iOS port).

Adapted from `feature/products/impl/tools/top_up_mutation_sweep.py` in the Android repo. Same spirit:
touch only the files that decide a top-up's fate and run only the top-up tests. Each mutant removes or
weakens exactly one rule the host call's contract states — idempotency, one-claim-per-source, terminality,
verdict immutability, group namespacing, and the translation from what coinage detected
into what a product is told.

Read a SURVIVED line as "no test distinguishes this rule's presence from its absence".

The sweep first runs the suite unmutated and refuses to continue unless it passes: a broken simulator or
a failing baseline would otherwise report every mutant as killed. Any run that ends without a clear
xcodebuild verdict aborts the sweep for the same reason.

iOS split note: on Android the coinage→product status translation lives in `ExecuteTopUpUseCase`, in scope
for these tests. On iOS the amount/shortfall arithmetic lives one layer down, in the claim services
(`ClaimCoinsService`/`ClaimAssetService`), which the top-up tests stub. The asset claim loop's exit guards
are covered by `ClaimAssetServiceTests` and swept here; the rest of the claim arithmetic is not.

Usage, from the repository root:
    python3 Packages/Coinage/Tools/top_up_mutation_sweep.py
    python3 Packages/Coinage/Tools/top_up_mutation_sweep.py --list
    python3 Packages/Coinage/Tools/top_up_mutation_sweep.py --check
    python3 Packages/Coinage/Tools/top_up_mutation_sweep.py --sim <simulator-udid>
    python3 Packages/Coinage/Tools/top_up_mutation_sweep.py --only "claim:" --only "window:"

Sources are restored on every exit path — normal exit, Ctrl-C, SIGTERM — and the restore is verified before
the script returns.
"""
import argparse
import os
import signal
import subprocess
import sys

COINAGE = "Packages/Coinage/Sources/IncomingPayment"
SERVICE = f"{COINAGE}/IncomingPaymentService.swift"
DESCRIPTOR = f"{COINAGE}/Model/IncomingPaymentSourceDescriptor.swift"
STATUS = f"{COINAGE}/Model/IncomingPaymentStatus.swift"
DETECT = f"{COINAGE}/Model/IncomingPaymentStatus+Detection.swift"
MODEL = f"{COINAGE}/Model/IncomingPayment.swift"
CONTEXT = f"{COINAGE}/IncomingPaymentContext.swift"
CLAIM_ASSET = "Packages/Coinage/Sources/Transfer/Claim/ClaimAssetService.swift"

DEFAULT_SIM_NAME = "iPhone 16"
ONLY_TESTING = [
    "CoinageTests/IncomingPaymentModelTests",
    "CoinageTests/IncomingPaymentSourceDescriptorTests",
    "CoinageTests/IncomingPaymentSourceResolverTests",
    "CoinageTests/IncomingPaymentContextTests",
    "CoinageTests/IncomingPaymentServiceTests",
    "CoinageTests/IncomingPaymentSweepTests",
    "CoinageTests/ClaimAssetServiceTests",
    "ProductsTests/PaymentTopUpRequestDtoTests",
]

# (label, file, exact source to replace, replacement). Each removes or weakens one rule.
MUTANTS = [
    # --- idempotency: an id names one operation, for good ---
    ("accept: a used id is handed out again", SERVICE,
     "        if try await store.fetch(groupId: groupId) != nil {",
     "        if false {"),

    ("accept: a top-up of nothing is accepted", SERVICE,
     "        guard amount > 0 else {",
     "        guard true else {"),

    # --- one live claim per source ---
    ("accept: a busy source is accepted", SERVICE,
     "            if descriptor.drawsOnSameFunds(as: otherDescriptor) {",
     "            if false {"),

    ("accept: a corrupted secret aborts the busy check", SERVICE,
     "secret corrupted; skipped in busy check\")\n                continue",
     "secret corrupted; skipped in busy check\")\n                throw IncomingPaymentError.sourceBusy"),

    ("busy: product-account paths are never the same source", DESCRIPTOR,
     "        case let (.productAccount(lhs), .productAccount(rhs)):\n            lhs == rhs",
     "        case (.productAccount, .productAccount):\n            false"),

    ("busy: overlapping coins are not the same source", DESCRIPTOR,
     "            !Set(lhs).isDisjoint(with: Set(rhs))",
     "            lhs == rhs"),

    # --- terminality & verdict immutability ---
    ("settle: a non-terminal status yields a verdict", STATUS,
     "        case .detecting,\n             .claiming:\n            nil",
     "        case .detecting,\n             .claiming:\n            .notClaimed"),

    ("settle: the source outlives the verdict", SERVICE,
     "        secretStore.remove(groupId: payment.groupId)",
     "        _ = payment.groupId"),

    ("settle: an unpersisted verdict still wipes the secret", SERVICE,
     "failed to persist verdict: \\(error)\")\n            return",
     "failed to persist verdict: \\(error)\")"),

    # --- a secret that cannot be read is not a secret that is gone ---
    ("drive: an unreadable secret settles the payment", SERVICE,
     "            return .unreadable",
     "            return .gone"),

    ("drive: a corrupted secret is retried forever", SERVICE,
     "secret corrupted; settling from durability group\")\n            return .gone",
     "secret corrupted; settling from durability group\")\n            return .unreadable"),

    ("durability: an unobservable group is settled as notClaimed", SERVICE,
     "group unobservable; left for next launch: \\(error)\")",
     "group unobservable; left for next launch: \\(error)\")\n"
     "            await settle(payment: payment, finalStatus: .notClaimed)"),

    ("durability: what the group finalized is ignored", SERVICE,
     "            let status = IncomingPaymentStatus(detection: detection)",
     "            let status = IncomingPaymentStatus.notClaimed"),

    ("subscribe: a recorded verdict is re-derived", SERVICE,
     "            return payment.outcome.map(IncomingPaymentStatus.init(outcome:)) ?? .detecting",
     "            return IncomingPaymentStatus.detecting"),

    ("subscribe: an unknown id is not reported as notFound", SERVICE,
     "                throw IncomingPaymentError.notFound(paymentId)",
     "                return .notClaimed"),

    ("isTerminal: an unfinalized claim is terminal", STATUS,
     "        case let .claimed(finalized):\n            finalized\n        case .claimedPartially,\n"
     "             .notClaimed:\n            true",
     "        case let .claimed(finalized):\n            true\n        case .claimedPartially,\n"
     "             .notClaimed:\n            true"),

    ("isTerminal: nothing is terminal", STATUS,
     "        case .claimedPartially,\n             .notClaimed:\n            true",
     "        case .claimedPartially,\n             .notClaimed:\n            false"),

    # --- the user is told of an unhappy verdict, and only of those ---
    ("acknowledge: a happy verdict prompts the user", SERVICE,
     "        case .claimed:\n            break\n        }\n    }",
     "        case .claimed:\n            await acknowledger.acknowledge(\n"
     "                productId: payment.productId, paymentId: payment.paymentId,\n"
     "                requestedAmount: payment.amount, outcome: outcome\n            )\n        }\n    }"),

    # --- scheduling: one runner per operation ---
    ("context: a running top-up is started a second time", CONTEXT,
     "        guard tasks[groupId] == nil, !pending.contains(where: { $0.groupId == groupId }) else {",
     "        guard true else {"),

    # --- the group a top-up's transactions land in ---
    ("group: two products share one top-up group", MODEL,
     '        "top up:\\(productId):\\(paymentId)"',
     '        "top up:\\(paymentId)"'),

    ("group: the \"top up:\" namespace is dropped", MODEL,
     '        "top up:\\(productId):\\(paymentId)"',
     '        "\\(productId):\\(paymentId)"'),

    # --- what the product is told ---
    ("status: a retry in progress is called partial", DETECT,
     "        case .claiming,\n             .claimingRest:\n            self = .claiming",
     "        case .claiming:\n            self = .claiming\n        case let .claimingRest(claimed):\n"
     "            self = .claimedPartially(actualClaimed: claimed)"),

    # --- the asset claim loop ends where the contract says ---
    ("claim: a closed window is still attempted", CLAIM_ASSET,
     "        if timing.now() >= run.retryUntil {",
     "        if false {"),

    ("claim: an unloadable remainder is attempted again", CLAIM_ASSET,
     "        if run.context.breakdown(amountInPlanks: remaining).isEmpty {",
     "        if false {"),

    ("claim: an unreadable voucher store is valued as zero", CLAIM_ASSET,
     "        } catch {\n            throw ClaimValuationError(underlying: error)\n        }",
     "        } catch {\n            return 0\n        }"),

    # --- the retry window belongs to the operation ---
    ("window: the retry window is not the one the operation opened with", SERVICE,
     "addingTimeInterval(CoinageConstants.topUpRetryWindow)",
     "addingTimeInterval(CoinageConstants.topUpRetryWindow * 2)"),
]


class SweepAborted(Exception):
    pass


def run_suite(destination):
    only = []
    for target in ONLY_TESTING:
        only += ["-only-testing:" + target]

    result = subprocess.run(
        ["xcodebuild", "test",
         "-project", "polkadot-app.xcodeproj",
         "-scheme", "polkadot-app",
         "-destination", destination] + only,
        capture_output=True, text=True,
    )
    out = result.stdout + result.stderr
    if "** TEST SUCCEEDED **" in out:
        return ("passed", None)
    if "** TEST FAILED **" in out:
        return ("failed", "test")
    if "** BUILD FAILED **" in out:
        return ("failed", "compile")
    # Neither verdict: the simulator is missing, xcodebuild crashed, or the destination is wrong. Counting
    # this as a kill would make a broken environment look like perfect coverage.
    raise SweepAborted(f"xcodebuild gave no verdict (exit={result.returncode}); last output:\n{out[-2000:]}")


def check_patterns(originals):
    """Every mutant must match its source exactly once, or the rule it names is no longer under test."""
    stale = []
    for label, path, old, _ in MUTANTS:
        count = originals[path].count(old)
        if count != 1:
            stale.append((label, count))
    return stale


def main():
    parser = argparse.ArgumentParser(description="Mutation sweep over the top-up guards")
    parser.add_argument("--list", action="store_true", help="print the mutants and exit without running")
    parser.add_argument("--check", action="store_true",
                        help="verify every mutant still matches its source exactly once, without running")
    parser.add_argument("--sim", default=None,
                        help=f"simulator UDID to run the tests on (default: the simulator named '{DEFAULT_SIM_NAME}')")
    parser.add_argument("--only", action="append", default=[], metavar="SUBSTRING",
                        help="run only mutants whose label contains SUBSTRING (repeatable); the rest are not counted")
    args = parser.parse_args()

    if not os.path.isfile("polkadot-app.xcodeproj/project.pbxproj") or not os.path.isfile(SERVICE):
        sys.exit("run this from the repository root")

    if args.list:
        for label, _, _, _ in MUTANTS:
            print(f"  {label}")
        return

    mutants = [m for m in MUTANTS if not args.only or any(needle in m[0] for needle in args.only)]
    if not mutants:
        sys.exit("no mutant label matches --only")

    originals = {path: open(path).read() for _, path, _, _ in MUTANTS}

    stale = check_patterns(originals)
    if args.check:
        for label, count in stale:
            print(f"STALE    {label}  (pattern matched {count}x)")
        print(f"{len(MUTANTS) - len(stale)}/{len(MUTANTS)} mutants apply cleanly")
        sys.exit(1 if stale else 0)

    destination = (
        f"platform=iOS Simulator,id={args.sim}" if args.sim
        else f"platform=iOS Simulator,name={DEFAULT_SIM_NAME}"
    )

    print("baseline (unmutated) ...", flush=True)
    try:
        verdict, reason = run_suite(destination)
    except SweepAborted as error:
        sys.exit(f"aborted: {error}")
    if verdict != "passed":
        sys.exit(f"aborted: the unmutated suite does not pass ({reason}); fix that before measuring mutants")
    print("baseline passed", flush=True)

    survived, killed, skipped = [], [], []

    # Turn a kill signal into an exception so the restore in `finally` still runs. Without this a sweep
    # stopped by a supervisor leaves a mutant sitting in the source, and whatever runs next measures it.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("terminated"))

    try:
        for label, path, old, new in mutants:
            original = originals[path]
            if original.count(old) != 1:
                skipped.append(label)
                print(f"SKIP     {label}  (pattern matched {original.count(old)}x — source moved under it)", flush=True)
                continue

            open(path, "w").write(original.replace(old, new, 1))
            try:
                verdict, reason = run_suite(destination)
            finally:
                open(path, "w").write(original)  # restore before the next mutant builds

            if verdict == "failed":
                killed.append(label)
                print(f"killed   {label}  ({reason})", flush=True)
            else:
                survived.append(label)
                print(f"SURVIVED {label}", flush=True)
    except SweepAborted as error:
        print(f"\naborted: {error}")
        sys.exit(2)
    finally:
        for path, text in originals.items():
            open(path, "w").write(text)
            assert open(path).read() == text, f"failed to restore {path}"

    if skipped:
        # Not a note: a pattern that no longer matches means the rule it names is gone or was rewritten,
        # and the sweep silently stopped testing it.
        print(f"\n{len(skipped)} mutant(s) could not be applied — the sweep is out of date with the source:")
        for label in skipped:
            print(f"  {label}")

    print(f"\nkilled {len(killed)}/{len(killed) + len(survived)} applied mutants")
    if survived:
        print("survivors:")
        for label in survived:
            print(f"  {label}")

    sys.exit(1 if survived or skipped else 0)


if __name__ == "__main__":
    main()

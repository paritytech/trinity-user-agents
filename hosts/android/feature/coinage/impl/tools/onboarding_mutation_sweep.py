#!/usr/bin/env python3
"""Mutation sweep over the durable onboarding loop.

A sibling of `coinage_rule_mutation_sweep.py` and narrow for the same reason: it touches only
`RealOnboardingUseCase.kt` and runs only `RealOnboardingUseCaseTest`, which keeps a sweep to seconds per
mutant rather than minutes. The mutants are hand-written, one per guard the loop's contract states — when a
denomination may be onboarded again, when the loop may stop, and what the caller is told meanwhile.

Read a SURVIVED line as "no test distinguishes this guard's presence from its absence".

Usage, from the repository root:
    python3 feature/coinage/impl/tools/onboarding_mutation_sweep.py
    python3 feature/coinage/impl/tools/onboarding_mutation_sweep.py --list

The source is restored on every exit path — normal exit, Ctrl-C and SIGTERM — and the restore is verified
before the script returns.
"""
import argparse
import glob
import os
import shutil
import signal
import subprocess
import sys
import xml.etree.ElementTree as ET

MODULE = "feature/coinage/impl"
ONBOARD = f"{MODULE}/src/main/java/io/paritytech/polkadotapp/feature_coinage_impl/domain/usecase/RealOnboardingUseCase.kt"
TEST_TASK = ":feature:coinage:impl:testDebugUnitTest"
TEST_FILTER = "*RealOnboardingUseCaseTest*"
RESULTS = f"{MODULE}/build/test-results/testDebugUnitTest"

# (label, exact source to replace, replacement). Each removes or weakens one guard.
MUTANTS = [
    # --- when the loop may stop ---
    ("stop on inclusion rather than finality",
     "            val outstanding = target.minusEach(settled.finalizedDenominations())",
     "            val outstanding = target.minusEach(settled.arrivedDenominations())"),

    ("the window never closes the loop",
     "            if (timeProvider.now() >= retryUntil) {",
     "            if (false) {"),

    ("a first attempt is made after the window closed",
     "            if (timeProvider.now() >= retryUntil) {\n                coinageLogW(\"Onboarding window closed group=${groupId.value} outstanding=${outstanding.size}\")\n                break\n            }",
     "            if (timeProvider.now() >= retryUntil && settled.isNotEmpty()) {\n                coinageLogW(\"Onboarding window closed group=${groupId.value} outstanding=${outstanding.size}\")\n                break\n            }"),

    # --- how much is still owed ---
    ("denominations are subtracted as a set, not one by one",
     "    val removable = other.toMutableList()\n\n    return filterNot { removable.remove(it) }",
     "    return this - other.toSet()"),

    # --- what may be submitted ---
    ("onboarding submits what the account cannot cover",
     "            if (price <= remaining) {",
     "            if (true) {"),

    ("the smallest denominations are funded first",
     "        sortedDescending().forEach { denomination ->",
     "        sorted().forEach { denomination ->"),

    ("the widest balance ever seen is onboarded against",
     "                affordable = outstanding.affordableWith(transferable)",
     "                affordable = (affordable + outstanding.affordableWith(transferable)).distinct()"),

    ("an empty affordable set is still submitted",
     "            if (affordable.isNotEmpty()) {",
     "            if (true) {"),

    # --- what the caller is told ---
    ("a payment merely in flight reports its running total",
     "            arrived.isNotEmpty() && any { it.status == CoinageTransactionStatus.FAILURE } ->",
     "            arrived.isNotEmpty() ->"),

    ("an unfinalized claim is reported as finalized",
     "        finalized = target.minusEach(finalizedDenominations()).isEmpty(),",
     "        finalized = true,"),

    ("a failed voucher's outputs are counted as minted",
     "    private suspend fun List<CoinageTransactionState>.arrivedDenominations(): List<ValueExponent> =\n        filter { it.status.isArrived }.mintedDenominations()",
     "    private suspend fun List<CoinageTransactionState>.arrivedDenominations(): List<ValueExponent> =\n        mintedDenominations()"),

    ("a shortfall is reported as fully onboarded",
     "            arrived.isNotEmpty() -> CoinageTransferDetection.ClaimedPartially(valueMintedBy(arrived))",
     "            arrived.isNotEmpty() -> claimed(target, arrived)"),

    ("an unplannable amount is not reported",
     "                send(CoinageTransferDetection.NotClaimed)\n                return@channelFlow",
     "                return@channelFlow"),
]


# `settled` holds only entries no longer live, and among those `isArrived` means exactly
# FINALIZED_SUCCESS — so the two readings coincide there and no scenario can tell them apart. The stricter
# call stays because the rule it states is finality, and the loop should say so where it stops.
EXPECTED_SURVIVORS = {
    "stop on inclusion rather than finality",
}


def failing_tests():
    failures = []
    for report in glob.glob(os.path.join(RESULTS, "*.xml")):
        try:
            root = ET.parse(report).getroot()
        except ET.ParseError:
            continue
        for case in root.iter("testcase"):
            if case.find("failure") is not None or case.find("error") is not None:
                failures.append(case.get("name"))
    return sorted(failures)


def run_suite():
    # Gradle overwrites reports but never deletes them, so a filtered run would otherwise read stale results.
    shutil.rmtree(RESULTS, ignore_errors=True)

    subprocess.run(
        ["./gradlew", TEST_TASK, "--tests", TEST_FILTER, "-q"],
        capture_output=True, text=True,
    )
    return failing_tests()


def main():
    parser = argparse.ArgumentParser(description="Mutation sweep over the durable onboarding loop")
    parser.add_argument("--list", action="store_true", help="print the mutants and exit without running")
    args = parser.parse_args()

    if not os.path.isfile("./gradlew") or not os.path.isfile(ONBOARD):
        sys.exit("run this from the repository root")

    if args.list:
        for label, _, _ in MUTANTS:
            expected = "  (expected to survive)" if label in EXPECTED_SURVIVORS else ""
            print(f"  {label}{expected}")
        return

    # A mutant is a temporary edit to real source. If anything else is editing that file — a commit, an IDE,
    # another sweep — the two interleave, and the mutant can end up committed.
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--", f"{MODULE}/src/main"],
        capture_output=True, text=True,
    ).stdout.strip()
    if dirty:
        sys.exit(f"refusing to run: main sources have uncommitted changes\n{dirty}")

    original = open(ONBOARD).read()
    survived, killed, skipped = [], [], []

    # Turn a kill into an exception so the restore below still runs. Without this a sweep stopped by a
    # supervisor leaves a mutant sitting in the source, and whatever anyone runs next measures the mutant.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("terminated"))

    try:
        for label, old, new in MUTANTS:
            if original.count(old) != 1:
                skipped.append(label)
                print(f"SKIP     {label}  (pattern matched {original.count(old)}x — source moved under it)")
                continue

            open(ONBOARD, "w").write(original.replace(old, new, 1))
            failures = run_suite()

            if failures:
                killed.append(label)
                print(f"killed   {label}  ({len(failures)} test(s), e.g. {failures[0]})")
            elif label in EXPECTED_SURVIVORS:
                print(f"survived {label}  (expected — equivalent mutant)")
            else:
                survived.append(label)
                print(f"SURVIVED {label}")
    finally:
        open(ONBOARD, "w").write(original)
        assert open(ONBOARD).read() == original, f"failed to restore {ONBOARD}"

    if skipped:
        # Not a note: a pattern that no longer matches means the guard it names is gone or was rewritten,
        # and the sweep silently stopped testing it.
        print(f"\n{len(skipped)} mutant(s) could not be applied — the sweep is out of date with the source:")
        for label in skipped:
            print(f"  {label}")

    applied = len(killed) + len(survived) + len(EXPECTED_SURVIVORS & set(l for l, _, _ in MUTANTS))
    print(f"\nkilled {len(killed)}/{applied} applied mutants ({len(EXPECTED_SURVIVORS)} expected survivor(s) excluded)")
    if survived:
        print("survivors:")
        for label in survived:
            print(f"  {label}")

    sys.exit(1 if survived or skipped else 0)


if __name__ == "__main__":
    main()

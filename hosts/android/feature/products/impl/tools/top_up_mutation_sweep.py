#!/usr/bin/env python3
"""Mutation sweep over the RFC-0006 top-up guards.

Scope is deliberately narrow, in the same spirit as `feature/coinage/impl/tools/coinage_rule_mutation_sweep.py`:
it touches only the three files that decide a top-up's fate and runs only the top-up tests. The mutants are
hand-written, one per rule the host call's contract states — idempotency, one-claim-per-source, terminality,
and the translation from what coinage detected into what a product is told.

Read a SURVIVED line as "no test distinguishes this rule's presence from its absence".

Usage, from the repository root:
    python3 feature/products/impl/tools/top_up_mutation_sweep.py
    python3 feature/products/impl/tools/top_up_mutation_sweep.py --list

The sources are restored on every exit path — normal exit, Ctrl-C and SIGTERM — and the restore is verified
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

MODULE = "feature/products/impl"
SRC = f"{MODULE}/src/main/java/io/paritytech/polkadotapp/feature_products_impl"
SERVICE = f"{SRC}/domain/topUpRequest/TopUpService.kt"
EXECUTE = f"{SRC}/domain/topUpRequest/ExecuteTopUpUseCase.kt"
STATUS = f"{SRC}/domain/topUpRequest/TopUpStatus.kt"
OPERATION = f"{SRC}/domain/topUpRequest/TopUpOperation.kt"
TEST_TASK = ":feature:products:impl:testDebugUnitTest"
TEST_FILTER = "*TopUp*"
RESULTS = f"{MODULE}/build/test-results/testDebugUnitTest"

# (label, file, exact source to replace, replacement). Each removes or weakens one rule.
MUTANTS = [
    # --- idempotency: an id names one operation, for good ---
    ("start: a used id is handed out again", SERVICE,
     "        if (repository.get(productId, id) != null) {",
     "        if (false) {"),

    # --- one live claim per source ---
    ("start: a busy source is accepted", SERVICE,
     "        claimantOf(productId, source)?.let { busyWith ->",
     "        (null as PaymentTopUpId?)?.let { busyWith ->"),

    ("busy: one product's account index blocks another's", SERVICE,
     "            sameProduct && index == other.index",
     "            index == other.index"),

    ("busy: overlapping coins are not the same source", SERVICE,
     "            secretKeys.any { it in other.secretKeys }",
     "            secretKeys == other.secretKeys"),

    # --- terminality ---
    ("settle: a non-terminal status is written as a verdict", SERVICE,
     "        if (!outcome.isTerminal) {",
     "        if (false) {"),

    ("settle: the source outlives the verdict", SERVICE,
     "        sourceStorage.remove(operation.groupId)",
     "        Unit"),

    ("attach: a recorded verdict is re-derived from the ledger", SERVICE,
     "        operation.outcome?.let { return MutableStateFlow(it) }",
     "        Unit"),

    ("isTerminal: an unfinalized claim is terminal", STATUS,
     "        is TopUpStatus.Claimed -> finalized",
     "        is TopUpStatus.Claimed -> true"),

    ("isTerminal: nothing is terminal", STATUS,
     "        is TopUpStatus.ClaimedPartially, is TopUpStatus.NotClaimed -> true",
     "        is TopUpStatus.ClaimedPartially, is TopUpStatus.NotClaimed -> false"),

    # --- resuming ---
    ("resume: a running top-up is started a second time", SERVICE,
     "                if (!running.containsKey(operation.groupId.value)) attach(operation)",
     "                attach(operation)"),

    # --- the group a top-up's transactions land in ---
    ("group: two products share one top-up group", OPERATION,
     '        get() = CoinageOperationGroupId("$TOP_UP_GROUP_PREFIX${productId.value}:${id.asHex()}")',
     '        get() = CoinageOperationGroupId("$TOP_UP_GROUP_PREFIX${id.asHex()}")'),

    # --- what the product is told ---
    ("status: a shortfall is reported as the full amount", EXECUTE,
     "        amount >= expected -> TopUpStatus.Claimed(finalized)",
     "        true -> TopUpStatus.Claimed(finalized)"),

    ("status: an unfinalized shortfall is called partial", EXECUTE,
     "        finalized -> TopUpStatus.ClaimedPartially(amount)\n        else -> TopUpStatus.Claiming",
     "        else -> TopUpStatus.ClaimedPartially(amount)"),

    ("status: a retry in progress is called partial", EXECUTE,
     "    is CoinageTransferDetection.ClaimingRest -> TopUpStatus.Claiming",
     "    is CoinageTransferDetection.ClaimingRest -> TopUpStatus.ClaimedPartially(claimed)"),

    ("status: repeats are reported to the product", EXECUTE,
     "            .distinctUntilChanged()",
     "            "),

    # --- the retry window belongs to the operation ---
    ("window: the retry window is not the one the operation opened with", EXECUTE,
     "        val retryUntil = operation.startedAt + TOP_UP_RETRY_WINDOW",
     "        val retryUntil = operation.startedAt + TOP_UP_RETRY_WINDOW + TOP_UP_RETRY_WINDOW"),
]


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
    parser = argparse.ArgumentParser(description="Mutation sweep over the top-up guards")
    parser.add_argument("--list", action="store_true", help="print the mutants and exit without running")
    args = parser.parse_args()

    if not os.path.isfile("./gradlew") or not os.path.isfile(SERVICE):
        sys.exit("run this from the repository root")

    if args.list:
        for label, _, _, _ in MUTANTS:
            print(f"  {label}")
        return

    # A mutant is a temporary edit to real source. If anything else is editing those files — a commit, an
    # IDE, another sweep — the two interleave, and the mutant can end up committed.
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--", f"{MODULE}/src/main"],
        capture_output=True, text=True,
    ).stdout.strip()
    if dirty:
        sys.exit(f"refusing to run: main sources have uncommitted changes\n{dirty}")

    originals = {path: open(path).read() for _, path, _, _ in MUTANTS}
    survived, killed, skipped = [], [], []

    # Turn a kill into an exception so the restore below still runs. Without this a sweep stopped by a
    # supervisor leaves a mutant sitting in the source, and whatever anyone runs next measures the mutant.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("terminated"))

    try:
        for label, path, old, new in MUTANTS:
            original = originals[path]
            if original.count(old) != 1:
                skipped.append(label)
                print(f"SKIP     {label}  (pattern matched {original.count(old)}x — source moved under it)")
                continue

            open(path, "w").write(original.replace(old, new, 1))
            failures = run_suite()

            if failures:
                killed.append(label)
                print(f"killed   {label}  ({len(failures)} test(s), e.g. {failures[0]})")
            else:
                survived.append(label)
                print(f"SURVIVED {label}")
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

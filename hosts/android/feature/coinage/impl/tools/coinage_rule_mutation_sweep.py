#!/usr/bin/env python3
"""Mutation sweep over the coinage recovery rule ladder.

Scope is deliberately narrow. This touches exactly one source file — CoinageRules.kt — and runs exactly one
Gradle task — :feature:coinage:impl:testDebugUnitTest. It is not a general-purpose mutation tool and is not
meant to grow into one: the mutants below are hand-written to mirror the predicates in
`.claude/scratch/coinage-durability-spec.md § Predicates`, one per guard the spec states. A generic tool
mutates bytecode conditionals instead, which on Kotlin yields mostly equivalent mutants from `getOrElse`
lambdas, `?:` chains and suspend state machines.

Read a SURVIVED line as "no test distinguishes this guard's presence from its absence". That is not the same
as "this guard is untested": a predicate the spec defines as total is correct independently of where it is
called, so scenarios cannot reach the state that would kill it. Only the ladder-level guards (Rules 3-7)
should be read as coverage gaps.

Usage, from the repository root:
    python3 feature/coinage/impl/tools/coinage_rule_mutation_sweep.py
    python3 feature/coinage/impl/tools/coinage_rule_mutation_sweep.py --list
    python3 feature/coinage/impl/tools/coinage_rule_mutation_sweep.py --only-fuzz

The source file is restored on every exit path — normal exit, Ctrl-C and SIGTERM — and the restore is
verified before the script returns.
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
SRC = f"{MODULE}/src/main/java/io/paritytech/polkadotapp/feature_coinage_impl"
RULES = f"{SRC}/domain/transaction/recovery/CoinageRules.kt"
ASSETS = f"{SRC}/domain/usecase/RealCoinageAssetsUseCase.kt"
CLAIM = f"{SRC}/domain/usecase/RealClaimReceivedCoinsUseCase.kt"
SUBMIT = f"{SRC}/domain/usecase/RealCoinageTransferSubmissionUseCase.kt"
UNLOAD = f"{SRC}/domain/externalPayment/usecase/UnloadRecyclerIntoExternalAssetUseCase.kt"
TEST_TASK = ":feature:coinage:impl:testDebugUnitTest"
RESULTS = f"{MODULE}/build/test-results/testDebugUnitTest"

# (label, exact source to replace, replacement). Each removes or weakens one guard.
MUTANTS = [
    ("R3 drop windowClosed",
     "    if (windowClosed &&\n        entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = true) }",
     "    if (\n        entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = true) }"),

    ("R3 drop noPotentialConsumers",
     "entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = true) }",
     "entry.outputs.any { evidence.absent(it, atFinalized = true) }"),

    ("R3 read best head not finalized",
     "entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = true) }",
     "entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = false) }"),

    ("R4 drop windowClosed",
     "    if (windowClosed && entry.inputs.any { evidence.available(it, atFinalized = true) }) {",
     "    if (entry.inputs.any { evidence.available(it, atFinalized = true) }) {"),

    ("R4 read best head not finalized",
     "    if (windowClosed && entry.inputs.any { evidence.available(it, atFinalized = true) }) {",
     "    if (windowClosed && entry.inputs.any { evidence.available(it, atFinalized = false) }) {"),

    ("R3b drop !windowClosed",
     "    if (!windowClosed &&\n        entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = false) }",
     "    if (\n        entry.outputs.any { it.noPotentialConsumers(dag, evidence) && evidence.absent(it, atFinalized = false) }"),

    ("R4b drop !windowClosed",
     "    if (!windowClosed && entry.inputs.any { evidence.available(it, atFinalized = false) }) {",
     "    if (entry.inputs.any { evidence.available(it, atFinalized = false) }) {"),

    ("R5 drop provenOwnCoins",
     "    if (provenOwnCoins && entry.inputs.all { evidence.absent(it, atFinalized = true) }) {",
     "    if (entry.inputs.all { evidence.absent(it, atFinalized = true) }) {"),

    ("R6 drop provenOwnCoins",
     "    if (provenOwnCoins &&\n        entry.inputs.any { evidence.exists(it, atFinalized = true) } &&",
     "    if (\n        entry.inputs.any { evidence.exists(it, atFinalized = true) } &&"),

    ("noPotentialConsumers drop handoff guard",
     "    if (dag.isHandedOff(publicKey)) return false\n    if (spent(dag, evidence)) return false",
     "    if (spent(dag, evidence)) return false"),

    ("noPotentialConsumers drop spent guard",
     "    if (dag.isHandedOff(publicKey)) return false\n    if (spent(dag, evidence)) return false",
     "    if (dag.isHandedOff(publicKey)) return false"),

    ("noPotentialConsumers drop live-consumer guard",
     "    return dag.consumers(publicKey).none { it.status != CoinageTransactionStatus.FAILURE }",
     "    return true"),

    ("spentByAbsence drop isCoin guard",
     "    if (!isCoin) return false\n    val minter = dag.minter(publicKey) ?: return false",
     "    val minter = dag.minter(publicKey) ?: return false"),

    ("spentByAbsence drop windowClosed",
     "    return minter.status == CoinageTransactionStatus.FINALIZED_SUCCESS &&\n        evidence.absent(this, atFinalized = true) &&\n        evidence.windowClosed(minter)",
     "    return minter.status == CoinageTransactionStatus.FINALIZED_SUCCESS &&\n        evidence.absent(this, atFinalized = true)"),

    ("spentByAbsence always false",
     "private fun LedgerAsset.spentByAbsence(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {\n    if (!isCoin) return false",
     "private fun LedgerAsset.spentByAbsence(dag: CoinageEntryDag, evidence: ChainEvidence): Boolean {\n    if (true) return false\n    if (!isCoin) return false"),

    ("provenConsumedOnChain drop isVoucher",
     "    asset.isVoucher && alias(atFinalized)[asset.publicKey] == AliasRead.UNLOADED",
     "    alias(atFinalized)[asset.publicKey] == AliasRead.UNLOADED"),

    ("provenNotUnloaded drop isVoucher",
     "    asset.isVoucher && alias(atFinalized)[asset.publicKey] == AliasRead.NOT_UNLOADED",
     "    alias(atFinalized)[asset.publicKey] == AliasRead.NOT_UNLOADED"),

    ("search drop wholeRangeRead",
     "            if (search.wholeRangeRead && windowClosed) {",
     "            if (windowClosed) {"),

    ("search drop windowClosed",
     "            if (search.wholeRangeRead && windowClosed) {",
     "            if (search.wholeRangeRead) {"),

    ("R0 record gone drops the finalized arm",
     "            evidence.executed(entry, atFinalized = true) ->\n"
     "                evidence.decided(entry, \"Rule 0 record gone, executed at F\", CoinageTransactionStatus.FINALIZED_SUCCESS, evidence.finalized)",
     "            false ->\n"
     "                evidence.decided(entry, \"Rule 0 record gone, executed at F\", CoinageTransactionStatus.FINALIZED_SUCCESS, evidence.finalized)"),

    ("R1 before R2 ordering removed",
     '    if (evidence.executed(entry, atFinalized = true)) {\n        return evidence.decided(entry, "Rule 1 executed at F", CoinageTransactionStatus.FINALIZED_SUCCESS, entry.successDetectedAt)',
     '    if (false) {\n        return evidence.decided(entry, "Rule 1 executed at F", CoinageTransactionStatus.FINALIZED_SUCCESS, entry.successDetectedAt)'),
]


# (label, file, exact source to replace, replacement) — the guards the use-case suites claim to hold.
USE_CASE_MUTANTS = [
    ("assets: voucher joined by coin key", ASSETS,
     "vouchers.map { TrackedVoucher(it, states.stateOf(OwnAsset.Voucher(it.ringVrfKeyIndex))) }",
     "vouchers.map { TrackedVoucher(it, states.stateOf(OwnAsset.Coin(it.ringVrfKeyIndex))) }"),

    ("assets: unknown asset is not free", ASSETS,
     "this[asset] ?: CoinageAssetState.UNTRACKED",
     "this[asset] ?: CoinageAssetState(handedOff = true, minterStatus = null, consumerStatus = null)"),

    ("assets: selectable ignores age and presence", ASSETS,
     "val selectable = coins.filter { it.isSelectable(recyclingAge) }",
     "val selectable = coins.filter { it.state.isFree }"),

    ("claim: resubmits an already-claimed group", CLAIM,
     "if (alreadySubmitted.isNotEmpty()) {",
     "if (false) {"),

    ("claim: only finality counts as arrived", CLAIM,
     "        private val ARRIVED_STATUSES = setOf(\n"
     "            CoinageTransactionStatus.PENDING_SUCCESS,\n"
     "            CoinageTransactionStatus.FINALIZED_SUCCESS,\n"
     "        )",
     "        private val ARRIVED_STATUSES = setOf(\n"
     "            CoinageTransactionStatus.FINALIZED_SUCCESS,\n"
     "        )"),

    ("claim: stops reporting while still live", CLAIM,
     "                states.any { it.status.isLive }",
     "                false"),

    ("claim: a wholly failed group is not an error", CLAIM,
     "            all { it.status == CoinageTransactionStatus.FAILURE } -> CoinageTransferDetection.Error.Transfer",
     "            none { it.status == CoinageTransactionStatus.FAILURE } -> CoinageTransferDetection.Error.Transfer"),

    ("submit: a key with no coin fails the claim", SUBMIT,
     "                Result.success(Unit)\n            }",
     "                Result.failure(IllegalStateException(\"no coin on chain\"))\n            }"),

    ("submit: claims registered outside their group", SUBMIT,
     "                groupId = groupId,",
     "                groupId = null,"),

    ("unload: an empty unload is allowed", UNLOAD,
     'vouchers.isEmpty() -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.emptyVouchers")',
     'false -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.emptyVouchers")'),

    ("unload: a voucher outside a recycler is allowed", UNLOAD,
     'vouchers.any { !it.isInRecycler() } -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.missingRecyclerInfo")',
     'false -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.missingRecyclerInfo")'),

    ("unload: an unregistered group is not submitted", UNLOAD,
     "            any { it.status.isLive } || isEmpty() -> ExternalUnloadStatus.Submitted",
     "            any { it.status.isLive } -> ExternalUnloadStatus.Submitted"),

    ("unload: partial success reported as success", UNLOAD,
     "            executed > 0 -> ExternalUnloadStatus.PartialSuccess(executed = executed, total = size)",
     "            executed > 0 -> ExternalUnloadStatus.Success"),

    ("unload: a group claims no vouchers", UNLOAD,
     "                inputs = group.vouchers.map { CoinageInput.Voucher(it.ringVrfKeyIndex) },",
     "                inputs = emptyList(),"),

    ("unload: resubmits an already-submitted group", UNLOAD,
     "        if (alreadySubmitted.isNotEmpty()) {",
     "        if (false) {"),
]

# Guards the spec defines as total, so no scenario can reach the state that kills them. Surviving here is
# the expected result, not a gap; see the module note above.
EXPECTED_SURVIVORS = {
    "noPotentialConsumers drop spent guard",
    "spentByAbsence drop isCoin guard",
    "spentByAbsence drop windowClosed",
    "spentByAbsence always false",
    "provenConsumedOnChain drop isVoucher",
    "provenNotUnloaded drop isVoucher",
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


def run_suite(only_fuzz=False):
    # Gradle overwrites reports but never deletes them, so a filtered run would otherwise read stale results.
    shutil.rmtree(RESULTS, ignore_errors=True)

    command = ["./gradlew", TEST_TASK, "-q"]
    if only_fuzz:
        command += ["--tests", "*CoinageFuzzTest*"]

    subprocess.run(command, capture_output=True, text=True)
    return failing_tests()


def main():
    parser = argparse.ArgumentParser(description=f"Mutation sweep over {os.path.basename(RULES)}")
    parser.add_argument("--list", action="store_true", help="print the mutants and exit without running")
    parser.add_argument(
        "--only-fuzz",
        action="store_true",
        help="run only CoinageFuzzTest, measuring what the fuzzer's invariants catch without the "
             "hand-written suite. Expect fewer kills: the fuzzer asserts invariants, not verdicts, so a "
             "mutant that only makes a correct verdict arrive by the wrong route is invisible to it.",
    )
    args = parser.parse_args()

    if not os.path.isfile("./gradlew") or not os.path.isfile(RULES):
        sys.exit("run this from the repository root")

    if args.list:
        for label, _, _, _ in [(l, RULES, o, n) for l, o, n in MUTANTS] + USE_CASE_MUTANTS:
            expected = "  (expected to survive)" if label in EXPECTED_SURVIVORS else ""
            print(f"  {label}{expected}")
        return

    # A mutant is a temporary edit to real source. If anything else is editing those files — a commit, an
    # IDE, another sweep — the two interleave, and the mutant can end up committed. That happened once.
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--", f"{MODULE}/src/main"],
        capture_output=True, text=True,
    ).stdout.strip()
    if dirty:
        sys.exit(f"refusing to run: main sources have uncommitted changes\n{dirty}")

    targets = [(label, RULES, old, new) for label, old, new in MUTANTS] + USE_CASE_MUTANTS
    originals = {path: open(path).read() for _, path, _, _ in targets}
    survived, killed, skipped = [], [], []

    # Turn a kill into an exception so the restore below still runs. Without this a sweep stopped by a
    # supervisor leaves a mutant sitting in the source, and whatever anyone runs next measures the mutant.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("terminated"))

    try:
        for label, path, old, new in targets:
            original = originals[path]
            if original.count(old) != 1:
                skipped.append(label)
                print(f"SKIP     {label}  (pattern matched {original.count(old)}x — source moved under it)")
                continue

            open(path, "w").write(original.replace(old, new, 1))
            failures = run_suite(only_fuzz=args.only_fuzz)

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
        # Not a note: a pattern that no longer matches means the guard it names is gone or was rewritten,
        # and the sweep silently stopped testing it.
        print("\nMutants that no longer match their source — these tested nothing:")
        for label in skipped:
            print(f"  - {label}")

    unexpected = [s for s in survived if s not in EXPECTED_SURVIVORS]
    print(f"\n{len(killed)} killed, {len(survived)} survived "
          f"({len(survived) - len(unexpected)} of them expected)")

    if unexpected:
        print("\nGuards no test distinguishes:")
        for label in unexpected:
            print(f"  - {label}")

    sys.exit(1 if unexpected or skipped else 0)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Mutation sweep over the coinage recovery rule ladder (iOS).

Scope is deliberately narrow. This touches exactly one source file — RuleEvaluator.swift — and runs
exactly one test target — CoinageTests. It is not a general-purpose mutation tool and is not meant to
grow into one: the mutants below are hand-written to mirror the durability spec's predicates, one per
guard the spec states. A generic tool mutates bytecode/AST conditionals instead, which on Swift yields
mostly equivalent mutants from optional chaining, `??` chains and async state machines.

Read a SURVIVED line as "no test distinguishes this guard's presence from its absence". That is not the
same as "this guard is untested": a predicate the spec defines as total is correct independently of where
it is called, so scenarios cannot reach the state that would kill it. Only the ladder-level guards should
be read as coverage gaps. Guards known to be total are listed in EXPECTED_SURVIVORS — surviving there is
the expected result, not a gap.

Usage, from the repository root:
    python3 Packages/Coinage/Tools/coinage_rule_mutation_sweep.py
    python3 Packages/Coinage/Tools/coinage_rule_mutation_sweep.py --list
    python3 Packages/Coinage/Tools/coinage_rule_mutation_sweep.py --only-fuzz

The source file is restored on every exit path — normal exit, Ctrl-C and SIGTERM — and the restore is
verified before the script returns. A mutant that fails to compile is reported as an error, never counted
as a kill.
"""
import argparse
import os
import re
import signal
import subprocess
import sys

MODULE = "Packages/Coinage"
RULES = f"{MODULE}/Sources/CoinageTx/Engine/RuleEvaluator.swift"

PROJECT = "polkadot-app.xcodeproj"
SCHEME = "polkadot-app"
DESTINATION = "platform=iOS Simulator,id=7C44CC0A-DFC3-4334-BCC1-8EE42558B79F"
TEST_TARGET = "CoinageTests"

# (label, exact source to replace, replacement). Each removes or weakens exactly one guard, and each
# replacement must still compile — a non-compiling mutant tests nothing.
MUTANTS = [
    ("R3 drop windowClosed",
     "        if windowClosed,\n"
     "           entry.outputs.contains(where: {\n"
     "               noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: true)",
     "        if true,\n"
     "           entry.outputs.contains(where: {\n"
     "               noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: true)"),

    ("R3 drop noPotentialConsumers",
     "noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: true)",
     "evidence.absent($0.publicKey, atFinalized: true)"),

    ("R3 read best head not finalized",
     "noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: true)",
     "noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: false)"),

    ("R4 drop windowClosed",
     "        if windowClosed, entry.inputs.contains(where: { available($0, evidence, atFinalized: true) }) {",
     "        if true, entry.inputs.contains(where: { available($0, evidence, atFinalized: true) }) {"),

    ("R4 read best head not finalized",
     "        if windowClosed, entry.inputs.contains(where: { available($0, evidence, atFinalized: true) }) {",
     "        if windowClosed, entry.inputs.contains(where: { available($0, evidence, atFinalized: false) }) {"),

    ("R3b drop !windowClosed",
     "        if !windowClosed,\n"
     "           entry.outputs.contains(where: {\n"
     "               noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: false)",
     "        if true,\n"
     "           entry.outputs.contains(where: {\n"
     "               noPotentialConsumers($0, dag, evidence) && evidence.absent($0.publicKey, atFinalized: false)"),

    ("R4b drop !windowClosed",
     "        if !windowClosed, entry.inputs.contains(where: { available($0, evidence, atFinalized: false) }) {",
     "        if true, entry.inputs.contains(where: { available($0, evidence, atFinalized: false) }) {"),

    ("R5 drop provenOwnCoins",
     "        if provenOwnCoins, entry.inputs.allSatisfy({ evidence.absent($0.publicKey, atFinalized: true) }) {",
     "        if true, entry.inputs.allSatisfy({ evidence.absent($0.publicKey, atFinalized: true) }) {"),

    ("R6 drop provenOwnCoins",
     "        if provenOwnCoins,\n"
     "           entry.inputs.contains(where: { evidence.exists($0.publicKey, atFinalized: true) }),",
     "        if true,\n"
     "           entry.inputs.contains(where: { evidence.exists($0.publicKey, atFinalized: true) }),"),

    ("noPotentialConsumers drop handoff guard",
     "        if dag.isHandedOff(output.publicKey) { return false }\n"
     "        if spent(output, dag, evidence) { return false }",
     "        if spent(output, dag, evidence) { return false }"),

    ("noPotentialConsumers drop spent guard",
     "        if dag.isHandedOff(output.publicKey) { return false }\n"
     "        if spent(output, dag, evidence) { return false }",
     "        if dag.isHandedOff(output.publicKey) { return false }"),

    ("noPotentialConsumers drop live-consumer guard",
     "        return dag.consumers(output.publicKey).allSatisfy { $0.status == .failure }",
     "        return true"),

    ("spentByAbsence drop isCoin guard",
     "        guard output.isCoin, let minter = dag.minter(output.publicKey) else { return false }",
     "        guard let minter = dag.minter(output.publicKey) else { return false }"),

    ("spentByAbsence drop windowClosed",
     "        return minter.status == .finalizedSuccess\n"
     "            && evidence.absent(output.publicKey, atFinalized: true)\n"
     "            && evidence.windowClosed(minter)",
     "        return minter.status == .finalizedSuccess\n"
     "            && evidence.absent(output.publicKey, atFinalized: true)"),

    ("spentByAbsence always false",
     "        guard output.isCoin, let minter = dag.minter(output.publicKey) else { return false }",
     "        guard false, output.isCoin, let minter = dag.minter(output.publicKey) else { return false }"),

    ("provenConsumed drop isVoucher",
     "        let provenConsumed = !output.isCoin && evidence.isUnloaded(output.publicKey, atFinalized: true)",
     "        let provenConsumed = evidence.isUnloaded(output.publicKey, atFinalized: true)"),

    ("provenNotUnloaded drop isVoucher (coins gated by alias too)",
     "        if input.isCoin {\n"
     "            return evidence.exists(input.publicKey, atFinalized: atFinalized)\n"
     "        }",
     "        if input.isCoin {\n"
     "            return evidence.exists(input.publicKey, atFinalized: atFinalized)\n"
     "                && evidence.isNotUnloaded(input.publicKey, atFinalized: atFinalized)\n"
     "        }"),

    ("R7 search drop wholeRangeRead",
     "        case .incomplete:\n"
     "            return .decided(Verdict(status: .pending, successDetectedAt: nil))",
     "        case .incomplete:\n"
     "            return .decided(Verdict(status: windowClosed ? .failure : .pending, successDetectedAt: nil))"),

    ("R7 search drop windowClosed",
     "        case .notFoundWindowComplete:\n"
     "            return .decided(Verdict(status: windowClosed ? .failure : .pending, successDetectedAt: nil))",
     "        case .notFoundWindowComplete:\n"
     "            return .decided(Verdict(status: .failure, successDetectedAt: nil))"),

    ("R0 record gone drops the finalized arm",
     "            if evidence.executed(entry, atFinalized: true) {\n"
     "                return .decided(Verdict(status: .finalizedSuccess, successDetectedAt: evidence.finalized))\n"
     "            }",
     "            if false, evidence.executed(entry, atFinalized: true) {\n"
     "                return .decided(Verdict(status: .finalizedSuccess, successDetectedAt: evidence.finalized))\n"
     "            }"),

    ("R1 before R2 ordering removed",
     "        if evidence.executed(entry, atFinalized: true) {\n"
     "            return .decided(Verdict(status: .finalizedSuccess, successDetectedAt: entry.successDetectedAt))\n"
     "        }",
     "        if false, evidence.executed(entry, atFinalized: true) {\n"
     "            return .decided(Verdict(status: .finalizedSuccess, successDetectedAt: entry.successDetectedAt))\n"
     "        }"),
]

# Guards the spec defines as total, so no scenario can reach the state that kills them. Surviving here is
# the expected result, not a gap; see the module note above.
EXPECTED_SURVIVORS = {
    "noPotentialConsumers drop spent guard",
    "spentByAbsence drop isCoin guard",
    "spentByAbsence drop windowClosed",
    "spentByAbsence always false",
    "provenConsumed drop isVoucher",
    # `provenNotUnloaded drop isVoucher` is *not* here: the rule suite builds evidence where an absent
    # coin has no alias reading, so gating coins by the alias flips a verdict and a test kills it.
}


def base_command():
    return [
        "xcodebuild",
        "-project", PROJECT,
        "-scheme", SCHEME,
        "-configuration", "Debug",
        "-destination", DESTINATION,
    ]


def build_for_testing():
    """Compiles the test bundle with the current mutant. Returns (compiled, output)."""
    result = subprocess.run(
        base_command() + ["build-for-testing"],
        capture_output=True, text=True,
    )
    out = result.stdout + result.stderr
    compiled = result.returncode == 0 and not re.search(r"\.swift:\d+:\d+: error:", out)
    return compiled, out


def run_tests(only_fuzz):
    """Runs the pre-built bundle. Returns (passed, failing_test_names)."""
    only = f"{TEST_TARGET}/CoinageFuzzTest" if only_fuzz else TEST_TARGET
    result = subprocess.run(
        base_command() + ["test-without-building", "-only-testing:" + only],
        capture_output=True, text=True,
    )
    out = result.stdout + result.stderr
    passed = result.returncode == 0
    fails = re.findall(r"Test [Cc]ase '(.+?)' failed", out)
    fails += [m.strip() for m in re.findall(r'✘.*?"(.+?)"', out)]
    return passed, sorted(set(fails))


def main():
    parser = argparse.ArgumentParser(description=f"Mutation sweep over {os.path.basename(RULES)}")
    parser.add_argument("--list", action="store_true", help="print the mutants and exit without running")
    parser.add_argument(
        "--match", default=None,
        help="run only mutants whose label contains any of these comma-separated substrings",
    )
    parser.add_argument(
        "--only-fuzz",
        action="store_true",
        help="run only CoinageFuzzTest, measuring what the fuzzer's invariants catch without the "
             "hand-written suite. Expect fewer kills: the fuzzer asserts invariants, not verdicts, so a "
             "mutant that only makes a correct verdict arrive by the wrong route is invisible to it.",
    )
    args = parser.parse_args()

    if not os.path.isfile(PROJECT.split("/")[0] + ".pbxproj") and not os.path.isdir(PROJECT):
        pass  # project presence checked below via RULES

    if not os.path.isdir(PROJECT) or not os.path.isfile(RULES):
        sys.exit("run this from the repository root")

    if args.list:
        for label, _, _ in MUTANTS:
            expected = "  (expected to survive)" if label in EXPECTED_SURVIVORS else ""
            print(f"  {label}{expected}")
        return

    # A mutant is a temporary edit to real source. If anything else is editing this file — a commit, an
    # IDE, another sweep — the two interleave, and the mutant can end up committed. Scope the guard to the
    # one file the sweep touches, so unrelated uncommitted work elsewhere does not block it.
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--", RULES],
        capture_output=True, text=True,
    ).stdout.strip()
    if dirty:
        sys.exit(f"refusing to run: {RULES} has uncommitted changes\n{dirty}")

    original = open(RULES).read()
    survived, killed, skipped, errored = [], [], [], []

    # Turn a supervisor kill into an exception so the restore below still runs. Without this a sweep
    # stopped by SIGTERM leaves a mutant sitting in the source, and whatever anyone runs next measures it.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit("terminated"))

    matchers = [m.strip() for m in args.match.split(",")] if args.match else None

    try:
        for label, old, new in MUTANTS:
            if matchers and not any(m in label for m in matchers):
                continue
            if original.count(old) != 1:
                skipped.append(label)
                print(f"SKIP     {label}  (pattern matched {original.count(old)}x — source moved under it)")
                continue

            open(RULES, "w").write(original.replace(old, new, 1))

            compiled, _ = build_for_testing()
            if not compiled:
                errored.append(label)
                print(f"ERROR    {label}  (mutant did not compile — fix the mutant string)")
                continue

            passed, fails = run_tests(args.only_fuzz)
            if passed:
                survived.append(label)
                print(f"SURVIVED {label}")
            else:
                killed.append(label)
                example = f", e.g. {fails[0]}" if fails else ""
                print(f"killed   {label}  ({len(fails)} test(s){example})")
    finally:
        open(RULES, "w").write(original)
        assert open(RULES).read() == original, f"failed to restore {RULES}"

    if skipped:
        print("\nMutants that no longer match their source — these tested nothing:")
        for label in skipped:
            print(f"  - {label}")
    if errored:
        print("\nMutants that did not compile — fix the mutant string:")
        for label in errored:
            print(f"  - {label}")

    unexpected = [s for s in survived if s not in EXPECTED_SURVIVORS]
    expected_survived = len(survived) - len(unexpected)
    print(f"\n{len(killed)} killed, {len(survived)} survived ({expected_survived} of them expected)")

    if unexpected:
        print("\nGuards no test distinguishes:")
        for label in unexpected:
            print(f"  - {label}")

    sys.exit(1 if unexpected or skipped or errored else 0)


if __name__ == "__main__":
    main()

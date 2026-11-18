//! Random walks over coins, entries and reorganisations that check the
//! invariants no reachable state may break, rather than expected verdicts.
//!
//! A failure is shrunk to the shortest walk that still breaks the invariant
//! and printed as scenario code. `COINAGE_FUZZ_SEEDS` and `COINAGE_FUZZ_STEPS`
//! set the seeds per profile and the actions per walk for a longer run; the
//! defaults keep a debug build to a few seconds, because every pass derives
//! each coin's key from the seed.

mod driver;

use rand::{SeedableRng, rngs::StdRng};

use driver::{Action, Driver, Fault, Kind, Profile, Stop, Violation};

const SEEDS_PER_PROFILE: u64 = 1;
const STEPS: usize = 60;
/// Each attempt replays a whole walk.
const MAX_SHRINK_ATTEMPTS: usize = 400;

/// The everyday mix and seven that each lean one way, so a walk reaches
/// the corners a uniform one rarely does.
const PROFILES: [Profile; 8] = [
	Profile { name: "balanced", overrides: &[(Kind::Crash, 1), (Kind::Relaunch, 1), (Kind::Archive, 1)] },
	Profile {
		name: "heavy spending",
		overrides: &[
			(Kind::Spend, 30),
			(Kind::Include, 30),
			(Kind::Batch, 20),
			(Kind::RunPass, 15),
			(Kind::Reorg, 2),
			(Kind::Archive, 1),
			(Kind::Crash, 1),
			(Kind::Relaunch, 1),
		],
	},
	Profile {
		name: "reorg storm",
		overrides: &[
			(Kind::Reorg, 30),
			(Kind::ProduceBlock, 30),
			(Kind::Finalize, 3),
			(Kind::RunPass, 20),
			(Kind::Include, 15),
			(Kind::Archive, 1),
			(Kind::Crash, 1),
			(Kind::Relaunch, 1),
		],
	},
	Profile {
		name: "entry churn",
		overrides: &[
			(Kind::Unload, 30),
			(Kind::Recycle, 25),
			(Kind::Load, 25),
			(Kind::Include, 20),
			(Kind::Spend, 3),
			(Kind::Archive, 5),
			(Kind::Crash, 1),
			(Kind::Relaunch, 1),
		],
	},
	Profile {
		name: "archival churn",
		overrides: &[
			(Kind::Archive, 30),
			(Kind::Unload, 20),
			(Kind::Load, 20),
			(Kind::Include, 15),
			(Kind::Reorg, 15),
			(Kind::Spend, 3),
			(Kind::Crash, 1),
			(Kind::Relaunch, 1),
		],
	},
	Profile {
		name: "restart churn",
		overrides: &[
			(Kind::Crash, 25),
			(Kind::Relaunch, 25),
			(Kind::RunPass, 20),
			(Kind::Spend, 15),
			(Kind::Include, 15),
			(Kind::HandOff, 10),
			(Kind::Archive, 1),
		],
	},
	Profile {
		name: "disclosure churn",
		overrides: &[
			(Kind::HandOff, 30),
			(Kind::PeerSpends, 25),
			(Kind::Spend, 20),
			(Kind::Include, 20),
			(Kind::RunPass, 15),
			(Kind::Archive, 1),
			(Kind::Crash, 2),
			(Kind::Relaunch, 2),
		],
	},
	Profile {
		name: "flaky reads",
		overrides: &[
			(Kind::FaultyPass, 30),
			(Kind::RunPass, 15),
			(Kind::Include, 15),
			(Kind::Spend, 10),
			(Kind::Archive, 1),
			(Kind::Crash, 1),
			(Kind::Relaunch, 1),
		],
	},
];

/// Profiles round robin, so a run cut short has sampled every one.
#[test]
fn random_walks_over_coins_entries_and_reorgs_keep_every_durability_invariant() {
	let seeds = setting("COINAGE_FUZZ_SEEDS").unwrap_or(SEEDS_PER_PROFILE);
	let steps = setting("COINAGE_FUZZ_STEPS").map_or(STEPS, |steps| usize::try_from(steps).expect("a step count"));
	for seed in 0..seeds {
		for profile in &PROFILES {
			let Err((violation, trace)) = Driver::new().walk(&mut StdRng::seed_from_u64(seed), steps, profile) else {
				continue;
			};
			let minimal = shrink(trace);
			panic!(
				"profile '{}' seed {seed} broke an invariant: {}\nshrunk to {} actions:\n{}",
				profile.name,
				violation.0,
				minimal.len(),
				as_rust(&minimal)
			);
		}
	}
}

fn setting(name: &str) -> Option<u64> {
	std::env::var(name).ok().map(|value| value.parse().expect("a number"))
}

/// Whether replaying `actions` on a fresh world breaks an invariant. A
/// candidate that is no longer a legal history says nothing either way.
fn violates(actions: &[Action]) -> bool {
	matches!(Driver::new().replay(actions), Err(Stop::Violated(Violation(_))))
}

/// Delta debugging, coarse to fine: drops runs of actions while the rest
/// still breaks an invariant, halving the run when nothing more can go.
fn shrink(trace: Vec<Action>) -> Vec<Action> {
	let mut best = trace;
	let mut chunk = (best.len() / 2).max(1);
	let mut attempts = 0;
	while chunk >= 1 && attempts < MAX_SHRINK_ATTEMPTS {
		let mut index = 0;
		let mut improved = false;
		while index + chunk <= best.len() && attempts < MAX_SHRINK_ATTEMPTS {
			let candidate: Vec<Action> = best[..index].iter().chain(&best[index + chunk..]).copied().collect();
			attempts += 1;
			if violates(&candidate) {
				best = candidate;
				improved = true;
			} else {
				index += chunk;
			}
		}
		if !improved {
			chunk /= 2;
		}
	}
	best
}

/// The walk as scenario code over a [`Driver`]'s seeded world, so a failure
/// becomes a named test.
fn as_rust(actions: &[Action]) -> String {
	actions
		.iter()
		.map(|action| match action {
			Action::ProduceBlock => "\tworld.advance(1, Finality::InBest);".to_owned(),
			Action::Finalize => "\tworld.finalize();".to_owned(),
			Action::RunPass => "\tworld.run_pass();".to_owned(),
			Action::FaultyPass(fault) => format!("\t// {}\n\tworld.run_pass();", fault_comment(*fault)),
			Action::Reorg(depth) => format!("\tworld.reorg({depth});"),
			Action::Spend(n) => format!("\tworld.register(&[world.coin({n})], &[world.coin(?)]);"),
			Action::Offboard(n) => format!("\tworld.register(&[world.coin({n})], &[]);"),
			Action::Split(n) => format!("\tworld.register(&[world.coin({n})], &[world.coin(?), world.coin(?)]);"),
			Action::Unload(n) => format!("\tworld.register(&[world.entry({n})], &[world.coin(?)]);"),
			Action::MultiUnload(first, second) => {
				format!("\tworld.register(&[world.entry({first}), world.entry({second})], &[world.coin(?)]);")
			},
			Action::Recycle(n, entry) => format!("\tworld.register(&[world.coin({n})], &[world.entry({entry})]);"),
			Action::Load(entry) => format!("\tworld.register(&[], &[world.entry({entry})]);"),
			Action::HandOff(n) => format!("\tworld.disclose(world.coin({n}), true);"),
			Action::PeerSpends(n) => format!("\tworld.consume(world.coin({n}), Finality::InBest);"),
			Action::Batch(first, second) => format!("\t// one operation spending coins {first} and {second}"),
			Action::Crash => "\tworld.crash();".to_owned(),
			Action::Relaunch => "\tworld.relaunch();".to_owned(),
			Action::Archive(n) => format!("\tworld.archive(world.entry({n}), Finality::InBest);"),
			Action::Place(n, ring) => format!("\tworld.place(world.entry({n}), {ring}, Finality::InBest);"),
			Action::Include(id, true) => format!("\tworld.execute(DurableTxId({id}), Finality::InBest);"),
			Action::Include(id, false) => {
				format!("\tworld.include(DurableTxId({id}), DispatchOutcome::Failed(?), Finality::InBest);")
			},
		})
		.collect::<Vec<_>>()
		.join("\n")
}

fn fault_comment(fault: Fault) -> &'static str {
	match fault {
		Fault::Coins => "coin reads fail at both heads for this pass",
		Fault::Aliases => "alias reads fail for this pass",
		Fault::Memberships => "membership reads fail for this pass",
		Fault::Positions => "ring position reads fail for this pass",
		Fault::Blocks => "canonical hashes cannot be read for this pass",
		Fault::Outcomes => "dispatch outcomes cannot be read for this pass",
		Fault::Pin => "the heads cannot be read for this pass",
	}
}

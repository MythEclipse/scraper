// Semver bump driven by the commits about to be pushed.
//
// Ported from 9router's `scripts/bump-version.mjs`, adapted for a Rust
// workspace that has three versioned manifests instead of a tree of
// package.json files.
//
// Rules (in precedence order):
//   1. any `feat:` commit in the push range          -> major
//   2. otherwise >= 5 changed files                  -> minor
//   3. otherwise                                     -> patch
//
// Run manually (`node scripts/bump-version.mjs`) and by the lefthook pre-push
// hook. Safe to re-run: when HEAD is already the bump commit for this range,
// the script exits 0 instead of bumping a second time.
//
// # Why every manifest moves together
//
// The service reports its version from `env!("CARGO_PKG_VERSION")` in `/health`,
// the dashboard, and the status store's meta key. Those all read the *workspace*
// version, so bumping `package.json` alone would leave the running service
// advertising a number that no release ever published.
//
// # Why Cargo.lock is regenerated here
//
// CI and the Dockerfile build with `cargo build --locked`, which refuses to run
// when Cargo.lock disagrees with a manifest. Editing `Cargo.toml` alone
// therefore turns a version bump into a failed build — verified by bumping the
// version and watching `cargo metadata --locked` exit 101. Regenerating the lock
// offline-able keeps the two in step, and it is written into the bump commit so
// the next push cannot ship a stale lock.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The commit message the bumped version must be committed under. */
const BUMP_SUBJECT = "chore: adjust package.json version (bump)";
const BLOCK_PUSH_AFTER_BUMP = process.argv.includes("--block-push-after-bump");

/** Run git, returning stdout, or "" when the command fails. */
function git(args) {
	try {
		return execFileSync("git", args, {
			cwd: projectRoot,
			encoding: "utf8",
			stdio: ["ignore", "pipe", "ignore"],
		}).trim();
	} catch {
		return "";
	}
}

/** Resolve the remote-tracking ref this branch pushes to. */
function upstreamRef(branch) {
	const configured = git([
		"rev-parse",
		"--abbrev-ref",
		"--symbolic-full-name",
		`${branch}@{upstream}`,
	]);
	if (configured) return configured;
	return `origin/${branch}`;
}

/** Subject lines of the commits in the push range, oldest first. */
function pendingSubjects(base, branch) {
	const range = base ? `${base}...${branch}` : branch;
	return git(["log", "--format=%s", range])
		.split("\n")
		.map((line) => line.trim())
		.filter(Boolean);
}

/** Repo-relative paths touched in the push range. */
function pendingFiles(base, branch) {
	const range = base ? `${base}...${branch}` : branch;
	return git(["diff", "--name-only", range])
		.split("\n")
		.map((line) => line.trim())
		.filter(Boolean);
}

/**
 * The version in the workspace `[workspace.package]` block.
 *
 * Read with a regex rather than a TOML parser: this script runs from a git hook
 * on machines that may have no Node dependencies installed, and the field is a
 * single line at the top of the file.
 */
function readWorkspaceVersion() {
	const toml = readFileSync(resolve(projectRoot, "Cargo.toml"), "utf8");
	const block = toml.split("[workspace.package]")[1];
	if (!block) throw new Error("Cargo.toml has no [workspace.package] section");
	const match = /^version\s*=\s*"([^"]+)"/m.exec(block);
	if (!match) throw new Error("Cargo.toml has no version in [workspace.package]");
	return match[1];
}

/** The version in package.json. */
function readPackageVersion() {
	return JSON.parse(readFileSync(resolve(projectRoot, "package.json"), "utf8")).version;
}

/**
 * Every tracked manifest carrying a `version` field, root first.
 *
 * `git ls-files` is the source rather than a glob: it lists tracked files only,
 * so `target/` can never enter the sweep.
 */
function versionedManifests() {
	const files = git(["ls-files"])
		.split("\n")
		.filter((file) => file === "package.json" || file === "Cargo.toml")
		.filter(Boolean);
	return files;
}

/** Write `version` into one manifest, without reformatting the rest of it. */
function writeVersion(file, version) {
	const path = resolve(projectRoot, file);
	const original = readFileSync(path, "utf8");
	let next;

	if (file === "package.json") {
		const pkg = JSON.parse(original);
		if (pkg.version === version) return false;
		// A bump commit that also reformats a manifest hides the one number it was
		// supposed to change. Refuse rather than produce that diff.
		if (`${JSON.stringify(pkg, null, 2)}\n` !== original) {
			throw new Error(`${file} is not canonical 2-space JSON — format it before bumping`);
		}
		pkg.version = version;
		next = `${JSON.stringify(pkg, null, 2)}\n`;
	} else {
		// Only the [workspace.package] version is rewritten. Member crates inherit
		// it through `version.workspace = true`, so editing them would either
		// duplicate the number or break the inheritance.
		const block = original.split("[workspace.package]")[1];
		if (!block) return false;
		const pattern = /(^version\s*=\s*")([^"]+)(")/m;
		const match = pattern.exec(block);
		if (!match) return false;
		if (match[2] === version) return false;
		const replaced = block.replace(pattern, `$1${version}$3`);
		next = original.replace(block, replaced);
	}

	writeFileSync(path, next);
	return true;
}

/**
 * Regenerate Cargo.lock so `--locked` builds keep working.
 *
 * `cargo metadata` is the cheapest command that rewrites the lock file: it
 * resolves the whole workspace without compiling anything. It is allowed to fail
 * (a machine without cargo can still bump package.json) but a failure is
 * reported, because shipping a manifest and a stale lock is what breaks CI.
 */
function refreshLockfile() {
	try {
		execFileSync("cargo", ["metadata", "--format-version", "1"], {
			cwd: projectRoot,
			stdio: ["ignore", "ignore", "pipe"],
		});
		return true;
	} catch {
		console.error(
			"[bump] WARNING: could not regenerate Cargo.lock (is cargo installed?). " +
				"The bump commit may fail `cargo build --locked`.",
		);
		return false;
	}
}

function bumpKind(subjects, files) {
	if (subjects.some((subject) => /^feat(\(|:)/.test(subject))) return "major";
	return files.length >= 5 ? "minor" : "patch";
}

function nextVersion(current, kind) {
	const [major, minor, patch] = current.split(".").map(Number);
	if ([major, minor, patch].some((part) => Number.isNaN(part))) {
		throw new Error(`Unparsable version in Cargo.toml: ${current}`);
	}
	if (kind === "major") return `${major + 1}.0.0`;
	if (kind === "minor") return `${major}.${minor + 1}.0`;
	return `${major}.${minor}.${patch + 1}`;
}

/**
 * True when HEAD is the bump commit for this very range — same message, and the
 * only files it touched are the manifests the bump owns.
 *
 * This cannot compare against `target`: that value is derived from the current
 * version, so it is never equal to it.
 */
function bumpAlreadyApplied(branch, manifests) {
	if (git(["log", "-1", "--format=%s", branch]) !== BUMP_SUBJECT) return false;
	return git(["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD", branch])
		.split("\n")
		.filter(Boolean)
		.every((file) => manifests.includes(file) || file === "Cargo.lock");
}

function main() {
	const manifests = versionedManifests();
	const cargoVersion = readWorkspaceVersion();
	const packageVersion = readPackageVersion();

	// A drifted pair means one manifest was bumped without the other, so the
	// running service advertises a version no release published — /health reads
	// CARGO_PKG_VERSION while the release pipeline publishes package.json. Bumping
	// on top of that would keep the drift and pick an arbitrary base, so stop and
	// name the fix instead.
	if (cargoVersion !== packageVersion) {
		console.error(
			`[bump] Cargo.toml is ${cargoVersion} but package.json is ${packageVersion}.`,
		);
		console.error(
			"[bump] Align them before pushing, or the service will report a version that was never released.",
		);
		console.error(`[bump]   sed -i 's/^version = ".*"/version = "${packageVersion}"/' Cargo.toml && cargo metadata --format-version 1 >/dev/null`);
		process.exit(1);
	}

	const current = packageVersion;

	const branch = git(["rev-parse", "--abbrev-ref", "HEAD"]);
	if (!branch || branch === "HEAD") {
		console.error("[bump] Detached HEAD — cannot determine what is being pushed. Skipping.");
		process.exit(0);
	}

	const base = upstreamRef(branch);
	if (!git(["rev-parse", "--verify", "--quiet", base])) {
		// The branch has never been pushed. There is no range to compare against, so
		// any bump here would be guessed from the entire history — and since that
		// history holds `feat:` commits, the guess would be a major bump for a first
		// push. Nothing to do.
		console.log(`[bump] No upstream ${base} yet - first push of this branch. Skipping.`);
		return;
	}

	const subjects = pendingSubjects(base, branch);
	const files = pendingFiles(base, branch);

	// An empty range means this push carries nothing to version — the branch is
	// already level with its upstream. Bumping here would invent a release for
	// a push that changes no code, and the next real push would then bump again
	// from a number nobody asked for.
	if (subjects.length === 0) {
		console.log(`[bump] Nothing to push vs ${base} - no bump needed.`);
		return;
	}

	const kind = bumpKind(subjects, files);

	// Re-bumping an already-bumped range would skip a release number, and the
	// pre-push hook always runs after the bump commit was made.
	if (bumpAlreadyApplied(branch, manifests)) {
		console.log(
			`[bump] HEAD is already the bump commit for this range (${subjects.length} pending commit(s), ${files.length} changed file(s) vs ${base}). No change.`,
		);
		return;
	}

	const target = nextVersion(current, kind);
	const written = manifests.filter((file) => writeVersion(file, target));
	if (written.some((file) => file === "Cargo.toml")) refreshLockfile();

	console.log(`[bump] ${current} -> ${target} (${kind}) — ${written.length}/${manifests.length} manifest(s)`);
	for (const file of written) console.log(`[bump]   ${file}`);
	console.log(`[bump] ${subjects.length} pending commit(s), ${files.length} changed file(s) vs ${base}`);
	for (const subject of subjects) console.log(`[bump]   - ${subject}`);

	// Commit it here rather than printing an instruction. The hook runs on pre-push:
	// the push is already in flight, so a version left in the working tree ships as
	// the OLD number and stays behind as dirt that the next push bumps again from
	// the stale base. An unstaged bump is an unshipped bump.
	const staged = [...manifests, "Cargo.lock"].filter((file) =>
		git(["ls-files", "--error-unmatch", file]),
	);
	execFileSync("git", ["add", ...staged], {
		cwd: projectRoot,
		stdio: ["ignore", "ignore", "inherit"],
	});
	execFileSync("git", ["commit", "-m", BUMP_SUBJECT, "--no-verify"], {
		cwd: projectRoot,
		stdio: ["ignore", "ignore", "inherit"],
	});
	console.log(`[bump] Committed ${target} as "${BUMP_SUBJECT}".`);
	if (BLOCK_PUSH_AFTER_BUMP) {
		console.error(
			"[bump] Version bump committed. Push stopped once so the bump commit is included on the next push.",
		);
		process.exitCode = 1;
	}
}

main();

# scraper

## Homepage and public status dashboard

`GET /` serves a server-rendered homepage: service overview, public statistics,
and a monitor for every documented endpoint. `GET /api/status` returns the same
snapshot as JSON, which the page polls every five seconds (only while the tab is
visible) and which any external uptime checker can read directly.

- The endpoint catalog is **derived from the OpenAPI document**, so a route
  added to `openapi_modules.rs` appears on the homepage immediately — no second
  list to keep in sync.
- Counters live in-process (`observability::telemetry`), keyed by the matched
  route *template* (`GET /api/anime/detail/{slug}`), so memory does not grow
  with traffic. The request path never talks to Redis.
- They are **persisted to Redis** by a write-behind loop
  (`observability::status_store`) once a second, plus a final flush on shutdown,
  and read back on boot. Totals therefore survive restarts and deploys, and the
  dashboard shows which store is in use (`statistik tersimpan di Redis` vs
  `in-process`). The flush writes absolute values, so a retry or a double flush
  cannot inflate a count.
- Health per endpoint: `ok` (last call 2xx), `warn` (last call 4xx — alive but
  rejecting the request), `error` (last call 5xx, or ≥50% of calls did), `idle`
  (never called), `stale` (last observation older than the staleness window, so
  no live verdict is asserted).
- Staleness matters because the counters outlive the process: an endpoint that
  failed days ago and has not been called since must not page anyone forever.
  Default window is 1 hour, override with `STATUS_STALE_AFTER_SECS`.
- The dashboard's own routes (`/`, `/api/status`, `/docs*`, `/favicon.*`) are
  excluded from the counters, so polling cannot drown out real traffic.
- `GET /health` stays a monitored public endpoint.
- Redis is optional. If it is unreachable the service still boots, the dashboard
  falls back to in-process numbers, and the flush warns once instead of every
  tick. Drop the stored history with
  `redis-cli --scan --pattern 'scraper:status:*' | xargs redis-cli DEL`.

## Releases and container images

The version is decided **on your machine, before the push**, by a git hook — not
by CI. This is the same mechanism `9router` uses, and it exists because deciding
the version in CI meant pushing two interdependent refs (a bump commit and a
tag). When the tag was attempted first the server rejected it with
`missing necessary objects` and the release was lost; that happened on three
consecutive releases here.

### How it works

`lefthook` (installed by `npm run prepare`) runs `scripts/bump-version.mjs` on
pre-push. The script reads the commits you are about to push and picks a bump:

| Push range contains | Bump |
|---|---|
| any `feat(...)` / `feat:` commit | major |
| 5 or more changed files | minor |
| anything else | patch |

It then writes that version into `Cargo.toml` (`[workspace.package]`) **and**
`package.json`, regenerates `Cargo.lock` (CI builds with `--locked`, so a stale
lock is a failed build), and commits all three as
`chore: adjust package.json version (bump)`.

The hook then **stops the push once, on purpose**. The bump commit was created
during the push, so the in-flight ref cannot contain it — pushing anyway would
publish the *old* version. Push again and the bump commit goes with it.

```bash
git push            # hook bumps, commits, and stops
git push            # ships the code plus the bump commit
```

Re-running the script on an already-bumped range is a no-op, so a third push
does not bump twice. Bumping with nothing pending is also a no-op.

### Tagging a release

Pushing commits does not publish anything. A release is a tag:

```bash
git tag v1.4.0
git push origin v1.4.0
```

The tag must match both manifests, and CI enforces that — a tag that disagrees
with `Cargo.toml` or `package.json` fails the `Validate release tag` job instead
of publishing an image whose `/health` reports a different version. The tag also
has to be pushed by you: a tag pushed with `GITHUB_TOKEN` does not trigger
workflows at all, which is the whole reason the old design had to do everything
in one run.

That tag then builds and pushes one container image per service to GHCR at
exactly that version, so an image tag always matches a git tag:

This repo publishes 1 image(s):

| Package | Image |
|---|---|
| `scraper` | `ghcr.io/asepharyana/scraper:scraper:<version>` |


Pull one without touching the registry UI:

```bash
docker pull ghcr.io/asepharyana/<repo>:<service>:<version>
```

### Adding or removing an image

Do not edit `.github/workflows/release.yml` to change which images are built.
The list lives in `.github/docker-images.json`:

```json
{ "images": [
  { "package": "<service>", "context": ".", "file": "Dockerfile" }
] }
```

- `package` becomes the GHCR path component, so it must be lowercase
  `[a-z0-9._-]`. It does not have to match the repo name.
- `context` is the build context directory, `file` the Dockerfile within it.
- `build_args` is an object of `KEY=value` pairs; the pipeline flattens it before
  the matrix, so both the object and a pre-joined `"A=1\nB=2"` string work.
- `"enabled": false` keeps an entry documented but skips building it. **An empty
  or fully disabled list fails the run** rather than publishing nothing.

Every enabled entry must have a `Dockerfile` **and** a `.dockerignore` in its
context, and both must be tracked by git — CI cannot see an untracked file, and a
missing `.dockerignore` silently ships `node_modules/` and `.git` inside the image.

`latest` tracks the newest stable release. Prereleases (`1.2.0-rc.1`) are
tagged but never promote `latest`.

### If a release fails

Check the run:

```bash
gh run list --workflow=release.yml --limit 5
gh run view <run-id> --log-failed
```

If the tag and the manifests disagree, bump, commit, push, and re-tag:

```bash
node scripts/bump-version.mjs   # or: npm run bump
git push
git tag -f v1.4.0 && git push -f origin v1.4.0
```

`git push` on its own never publishes an image: no tag, no release. That is
intentional, and it is why a normal commit push shows no release run at all.
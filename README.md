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

Every push to `main` is analysed by [semantic-release](https://semantic-release.gitbook.io/).
The commit prefix decides the version bump — nobody types a version number:

| Commit | Result |
|---|---|
| `feat(scope): …` | minor bump |
| `fix(scope): …` | patch bump |
| `BREAKING CHANGE:` in the footer | major bump |
| `chore`, `ci`, `docs`, `refactor`, `test`, `build` | patch bump, or no release at all |

A release publishes a tag, a GitHub Release and the changelog. The same workflow
then builds and pushes one container image per service to GHCR at exactly that
version, so an image tag always matches a git tag:

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

Check the run, then fix forward with a normal commit — do not re-tag by hand:

```bash
gh run list --workflow=release.yml --limit 5
gh run view <run-id> --log-failed
```

A run that only built images and published no tag is normal for a `chore` push:
there was nothing to release, so no version was derived and no image was tagged.
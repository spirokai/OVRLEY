# Strava Activity Provider Integration Specification

Status: Ready for implementation planning  
Last updated: 2026-07-16

## 1. Summary

OVRLEY will support importing activities directly from Strava. The integration is read-only: users can browse their own activities by month and import the complete documented Strava activity streams into OVRLEY. It will not upload, edit, delete, comment on, or otherwise mutate Strava data.

Strava is the first implementation of a reusable activity-provider architecture. Provider-neutral infrastructure must be shared by future integrations wherever the underlying behavior is genuinely common. OAuth details, remote API contracts, and provider-response adapters remain provider-specific.

The security boundary is:

```text
React presentation and TanStack Query
        |
        | narrow Tauri IPC; no credentials
        v
Rust provider orchestration
        |
        | HTTPS with opaque OVRLEY session credential
        v
Cloudflare Worker at api.ovrley.cc
        |
        | Strava access token held only by the Worker
        v
Strava API
```

The Cloudflare Worker is a permanent, purpose-built API proxy. The Strava client secret and all Strava access and refresh tokens remain server-side. React never performs provider network requests and never receives any credential.

## 2. Goals

- Connect one Strava account to the current OS user profile using system-browser OAuth.
- Persist the connection safely in the operating system credential store.
- Browse the authenticated athlete's activities one activity-local calendar month at a time.
- Display the activity name, distance, local date, elapsed duration, and sport type.
- Include private and `Only You` activities through the read-only `activity:read_all` scope.
- Show manual, summary-only activities in the list while marking them unavailable for import.
- Download every currently documented Strava activity stream at high resolution when an activity is selected.
- Cache downloaded stream responses locally for 30 days while always deriving list membership from a fresh or in-memory-cached Strava list response.
- Convert provider data once through a strict Strava adapter into OVRLEY's canonical `RawActivity`, then use the existing shared finalizer and activation path.
- Replace the active activity atomically so failures never clear or partially replace a working activity.
- Establish provider-neutral frontend, Rust orchestration, cache, error, source-identity, and presentation contracts for future activity providers.

## 3. Non-goals

- Uploading activities or files to Strava.
- Editing, deleting, creating, or otherwise mutating Strava activities.
- Comments, kudos, clubs, segments, routes, athlete statistics, webhooks, or social features.
- Fetching Strava activity-detail responses in addition to streams.
- Persisting activity-list responses across OVRLEY processes.
- Restoring the previously active Strava activity after OVRLEY restarts.
- Worker-side persistence or edge caching of activity lists or streams.
- Supporting multiple simultaneously connected Strava accounts in one OS user profile.
- Defining final visual styling. This specification defines states and behavior only.

## 4. Architectural Principles

### 4.1 Provider-neutral architecture

Shared code must use canonical provider contracts rather than Strava-shaped data. A provider identifier is part of every query key, cache path, source descriptor, and provider operation.

The reusable layer owns:

- Canonical connection status and provider activity-summary contracts.
- TanStack Query keys, monthly-query orchestration, invalidation, and mutation lifecycle.
- Month navigation and presentational activity-list behavior.
- Credential-store abstractions and connection lifecycle orchestration in Rust.
- HTTP timeout, retry, size-limit, and typed-error infrastructure in Rust.
- Versioned cache envelopes, TTL checks, startup cleanup, and atomic file replacement.
- Canonical activity source identity.
- Atomic conversion/finalization/activation flow.

The provider-specific layer owns:

- Authorization protocol details and requested scopes.
- Worker endpoints and upstream API calls.
- Validation of the provider's list and stream response contracts.
- Mapping provider list responses into canonical provider activity summaries.
- Mapping provider stream responses into canonical `RawActivity` data.
- Provider-specific processing options required by the shared finalizer.

Shared consumers must not contain Strava field names, aliases, response-shape fallbacks, or Strava-specific conditionals. The Strava adapter is the one external-system boundary where Strava names and shapes are translated.

Reuse must be based on these real shared behaviors. Future providers must not be forced into a Strava-specific remote API shape merely to satisfy an abstraction.

### 4.2 Ownership boundaries

- React components are presentational.
- TanStack Query and thin provider-neutral feature hooks own remote-operation state in the frontend.
- Zustand continues to own active editor/media state, not provider query results.
- Rust owns provider credentials, Worker networking, local disk caching, ingress validation, provider adaptation, and finalization.
- `ovrley_core` owns pure provider-response validation/adaptation and canonical activity finalization where no Tauri or OS dependency is required.
- The Tauri shell owns keyring access, HTTP transport, managed authorization attempts, app paths, and IPC commands.
- The Cloudflare Worker owns Strava OAuth secrets, Strava token lifecycle, D1 sessions, and the fixed read-only Strava API surface.

### 4.3 Strict contracts

- Required Worker, Strava, cache-envelope, and canonical activity data must be validated at ingress.
- Malformed present data must fail loudly and must not be repaired with defaults or aliases.
- Optional Strava streams may be absent because not every activity records every metric.
- A non-empty, valid time stream is required for import.
- No partial monthly list or partial activity is accepted.
- Unknown future provider fields do not automatically become OVRLEY metrics. Provider support is added deliberately at the provider adapter.

## 5. Canonical Shared Contracts

The exact Rust and JavaScript representation may follow repository conventions, but the domain contract must contain the following concepts.

### 5.1 Provider identifier

Provider IDs are canonical, allowlisted values owned by OVRLEY. The first value is `strava`. They are not arbitrary URLs or dynamically loaded provider names.

### 5.2 Connection status

A connected provider exposes only the account information needed by the UI:

```text
providerId
connectionId
athleteId
displayName
status
```

`status` distinguishes at least disconnected, connecting, connected, temporarily unavailable, and credential-store unavailable. Network failure must not be interpreted as revocation.

### 5.3 Provider activity summary

The Worker maps Strava's summary response to a versioned canonical list item containing at least:

```text
providerId
providerActivityId       string, even when the upstream ID is numeric
name
distanceMeters
startedAt                absolute UTC instant for synchronization
localStartedAt           validated activity-local date/time for presentation/month membership
elapsedSeconds
sportType                sourced from Strava sport_type
manual
```

The local desktop augments this result with transient state such as cache availability and import progress. Local cache files never create list items and never determine whether an activity remains visible.

### 5.4 Activity source descriptor

The current file-centric activity identity must be replaced by one canonical source descriptor. Provider activities must not fabricate `.fit` or `.gpx` filenames or pretend to be local files.

Conceptual variants are:

```text
LocalFileSource
  kind: local-file
  displayName
  format

ProviderSource
  kind: provider
  providerId
  providerActivityId
  displayName

```

All activation, summary, rendering, diagnostics, and display consumers use this source descriptor. There must not be parallel Strava-specific active-activity state.

### 5.5 Provider errors

Worker and Rust failures are translated into a canonical typed error contract. It must distinguish at least:

- Credential store unavailable or locked.
- Not connected.
- Authorization denied.
- Authorization expired.
- Required scope rejected.
- Session revoked.
- Network unavailable.
- Worker or Strava temporarily unavailable.
- Rate limited, including `retryAt` when known.
- Invalid Worker response.
- Invalid provider response.
- Manual or streamless activity unavailable.
- Stream payload too large.
- Corrupt cache.
- Activity finalization failure.

Components render these states but do not infer them from error-message text.

## 6. Cloudflare Worker

### 6.1 Deployment and source ownership

- Worker source, Wrangler configuration, D1 migrations, and deployment automation live in the `ovrley-website` repository.
- Production uses a dedicated API origin under `https://api.ovrley.cc`.
- The versioned Strava API base is `https://api.ovrley.cc/strava/v1`.
- Desktop builds always use this production origin. There is no runtime endpoint override.
- Tests use injected/mocked transports rather than redirecting release binaries to arbitrary origins.
- The Worker does not enable a generic CORS API for WebView clients; the native Rust client is the API consumer.

### 6.2 Purpose-built API

The Worker exposes only purpose-built operations. It is not a generic Strava path proxy.

The v1 API must support the following logical operations:

- Create a browser authorization attempt.
- Observe and exchange a browser authorization attempt.
- Validate an authenticated OVRLEY session and return minimal account identity.
- Disconnect the current OS-profile session.
- Return one complete activity-local calendar month of canonical activity summaries.
- Return all requested streams for one activity ID.

Routes should use fixed path names. Activity IDs should be supplied in request bodies where practical so default URL logs do not retain them. The Worker may trust the authenticated client's requested activity ID and rely on Strava's authorization checks; signed activity capabilities and activity-detail ownership calls are not required.

### 6.3 OAuth flow

OVRLEY is a public desktop client. No shared Worker API key or other recoverable secret is embedded in JavaScript or the Rust binary.

Authentication uses the system browser and short-lived polling:

1. Rust obtains or creates a random stable identifier for the current OS user profile and creates a high-entropy one-time proof verifier/challenge.
2. Rust requests a new authorization attempt from the Worker. The Worker creates a high-entropy, one-time OAuth state and stores only the data required to finish the attempt.
3. The Worker returns an opaque attempt ID and authorization URL. Rust opens that URL in the system browser.
4. Strava redirects to the HTTPS Worker callback. No custom protocol, deep link, localhost listener, Strava token, or OVRLEY session token appears in the browser return URL.
5. The Worker validates exact state, exchanges the one-time code with the server-held Strava client secret, and validates the granted scope.
6. Rust polls the attempt approximately every two seconds. Completion requires proof of the verifier created by Rust.
7. On success, the Worker issues a random 256-bit opaque OVRLEY session token. Rust stores it in the OS credential store before reporting the connection as complete.
8. The attempt is invalidated immediately after successful exchange, explicit cancellation, or expiry.

Authorization attempts expire after five minutes. Closing OVRLEY leaves no persistent authorization attempt; the Worker expires it normally.

The requested scope is exactly the read-only scope required to access all of the athlete's activities, including private and `Only You` activities: `activity:read_all`. No write scope is requested. Strava may let the athlete uncheck scopes; the Worker must reject incomplete authorization and must not replace an existing working session when the required scope was not granted.

### 6.4 Session model

- A portable OVRLEY executable shares one Strava connection with all other OVRLEY copies running under the same OS user profile.
- A random stable profile identifier associates the Worker session with that OS-profile boundary.
- At most one active Strava session exists for that profile identifier.
- A successful new login atomically replaces and revokes the previous session for that profile.
- The previous session remains valid while the replacement browser flow is pending or fails.
- Sessions on other computers or OS user profiles are independent.
- An active session normally persists until local disconnect, replacement, Strava revocation, or unrecoverable token failure.
- An inactivity ceiling of one year removes abandoned Worker sessions and encrypted Strava tokens. A scheduled Worker cleanup performs this retention policy.

Rust authenticates Worker requests with the opaque OVRLEY session token over HTTPS. D1 stores only a keyed hash of that token, never the token itself. Long-lived JWTs and embedded desktop secrets are not used.

### 6.5 D1 storage and encryption

D1 stores only authorization/account state:

- Short-lived OAuth attempt records.
- Hashed session credentials and hashed/stable profile associations.
- Minimal athlete identity: Strava athlete ID and display name.
- Granted scopes.
- Access-token expiry and refresh coordination state.
- Application-encrypted Strava access and refresh tokens.
- Session creation, last-used, and retention timestamps.

Strava tokens are encrypted using authenticated encryption through the Worker Web Crypto API. The encryption key is a Worker secret, not a D1 value. Stored ciphertext includes a key identifier and unique nonce so encryption keys can be rotated without placing plaintext tokens in migrations or logs.

D1 also coordinates token refresh so concurrent requests cannot use and invalidate the same rotating refresh token. A short conditional refresh lease or equivalent transaction-safe mechanism ensures one refresh owner. Successful refresh responses atomically replace both the access token and the latest returned refresh token.

D1 does not store activity summaries, activity IDs for authorization, or stream payloads.

### 6.6 Strava token lifecycle

- The Worker refreshes access tokens automatically before they expire.
- An unexpected Strava 401 triggers at most one coordinated refresh and one retry.
- Every successful refresh stores the latest refresh token because Strava invalidates the previous refresh token immediately when it rotates.
- A definitive refresh/revocation failure invalidates the OVRLEY session.
- Local Disconnect uses Strava's token-specific `POST /oauth/revoke` flow, introduced in June 2026, to revoke the current token chain without intentionally deauthorizing all independently connected devices.
- After the revocation attempt, the Worker removes the local session and token material and Rust removes the OS credential.
- The one-year orphan cleanup applies the same token-specific revocation on a best-effort basis before deletion.
- The legacy global Strava deauthorization endpoint is not part of the normal Disconnect operation.

### 6.7 Activity-list operation

The Worker accepts a validated `YYYY-MM` month and returns one complete month. It:

1. Computes a safely widened UTC query range that covers all valid activity-local timezone offsets.
2. Calls Strava's authenticated-athlete activities endpoint with the maximum supported page size.
3. Continues pagination until the month is complete.
4. Validates every required summary field.
5. Filters membership by validated Strava `start_date_local`, not UTC or the desktop's timezone.
6. Maps only the canonical summary fields required by OVRLEY.
7. Returns no partial result if a page fails or is malformed.

Elapsed time, not moving time, is the canonical list and import duration because it aligns with the activity stream and OVRLEY's synchronization timeline.

Manual activities remain in the response with `manual: true`; they are not sent to the streams endpoint by the normal UI.

### 6.8 Stream operation

For an importable activity, the Worker makes one Strava stream request and does not make an activity-detail request. It explicitly requests every currently documented activity stream:

- `time`
- `distance`
- `latlng`
- `altitude`
- `velocity_smooth`
- `heartrate`
- `cadence`
- `watts`
- `temp`
- `moving`
- `grade_smooth`

The request uses high resolution, time-based series, and keyed stream objects. The successful stream JSON is passed through unchanged to Rust so the provider-specific local validator/adapter and raw cache share one source representation.

The Worker must stream or buffer responses without logging bodies and must return `Cache-Control: private, no-store` or stricter equivalent headers. Cloudflare edge caching is disabled for all authenticated operations.

### 6.9 Retries and rate limits

- A logical operation receives no more than one retry at the layer that owns the failure.
- Rust retries a Worker transport failure or retryable Worker 5xx once with a short bounded backoff.
- The Worker retries a Strava transport failure or retryable Strava 5xx once. If that retry fails, it returns a typed upstream failure that Rust does not retry again.
- TanStack Query does not add another retry around native provider operations.
- Rate-limit responses are never automatically retried.
- Strava rate-limit headers are translated into a typed error with `retryAt` when the reset can be determined.
- The Worker forwards enough non-sensitive quota state for operational diagnosis but not raw upstream bodies.

Cloudflare Workers Rate Limiting bindings provide approximate abuse controls without D1 request counters. Initial development values are intentionally generous and configuration-only so they can be tightened after Strava approval and production telemetry:

| Operation             | Key            | Initial limit |
| --------------------- | -------------- | ------------- |
| Start OAuth           | Client IP      | 10/minute     |
| Poll OAuth status     | Attempt ID     | 40/minute     |
| Monthly activity list | OVRLEY session | 30/minute     |
| Stream download       | OVRLEY session | 15/minute     |

Strava's application-wide read quota remains authoritative. A Strava 429 is handled through the typed rate-limit behavior above.

### 6.10 Worker observability

Worker logs may contain only redacted operational metadata:

- OVRLEY request ID.
- Fixed route name.
- Status and coarse typed error code.
- Latency.
- Strava rate-limit limit/usage headers.

Logs must not contain authorization headers, OAuth state, codes, proof verifiers, access/refresh/session tokens, athlete names, raw athlete IDs, activity IDs, activity names, list bodies, stream bodies, coordinates, or health metrics.

## 7. Desktop Rust Architecture

### 7.1 Network and credential boundary

- All Worker requests originate in Rust through a direct native HTTP client.
- React and the WebView do not call the Worker or Strava.
- The Strava client secret and Strava tokens never enter the desktop process.
- The opaque OVRLEY session credential never enters JavaScript.
- The release Worker origin is compiled into the native client.
- Standard platform TLS validation is required.
- Network, credential store, clock, and cache filesystem operations are injected behind testable Rust interfaces.

### 7.2 Credential storage

Rust uses the `keyring` crate directly:

- Windows: Credential Manager.
- macOS: Keychain.
- Linux: Secret Service.

The credential entry contains the stable local profile identity and current opaque Worker session credential. A locked, missing, or unsupported credential service produces `credential-store-unavailable`; the feature must not silently fall back to Tauri Store, app preferences, plaintext files, environment variables, or in-memory login.

The first release supports all currently released desktop platforms. Linux systems without a functioning Secret Service cannot connect Strava and receive the explicit unavailable state.

### 7.3 Provider orchestration

The native provider layer exposes narrow provider operations through Tauri IPC. The conceptual operations are:

- Get provider connection status.
- Start, poll, and cancel provider authorization.
- Disconnect a provider.
- List one provider month.
- Inspect local cache status for returned activity IDs.
- Import one provider activity, with an explicit force-download option.

Provider IDs are validated against a native registry/enum. No IPC command accepts arbitrary API origins, paths, headers, scopes, or credentials.

Managed native state may retain the current short-lived authorization attempt and shared HTTP client. Durable credentials remain in the keyring, not managed memory alone.

### 7.4 Strava ingress and adapter

The Strava response validator accepts exactly the documented requested stream types. A valid import requires:

- A non-empty time stream.
- Finite, non-negative, monotonically ordered elapsed values.
- Structurally valid metadata for every returned stream.
- Sample alignment required by the keyed high-resolution time-series response.
- Valid latitude/longitude pairs when `latlng` is present.
- Valid element types and finite numeric values for every present numeric stream.

Documented stream absence is optional and does not create a fallback value. Malformed present streams fail the import.

The Strava adapter maps once into canonical OVRLEY names and units:

| Strava stream     | Canonical OVRLEY data                                                            |
| ----------------- | -------------------------------------------------------------------------------- |
| `time`            | `elapsed_seconds` and absolute timestamps derived from the summary start instant |
| `distance`        | `distance` in meters                                                             |
| `latlng`          | `latitude`, `longitude`                                                          |
| `altitude`        | `elevation` in meters                                                            |
| `velocity_smooth` | `speed` in meters per second                                                     |
| `heartrate`       | `heartrate` in beats per minute                                                  |
| `cadence`         | `cadence`                                                                        |
| `watts`           | `power` in watts                                                                 |
| `temp`            | `temperature` in Celsius                                                         |
| `grade_smooth`    | `gradient`                                                                       |
| `moving`          | Canonical aligned boolean moving series                                          |

The canonical activity schema must be extended to retain the aligned `moving` series even though no current widget renders it. It must not be encoded as an undocumented numeric alias.

The adapter combines stream samples with the selected canonical summary and creates a provider source descriptor. It requests shared idle-gap processing explicitly rather than reimplementing derivation, distance, smoothing, coverage, or final assembly. Those remain owned by the existing Rust finalizer.

### 7.5 Atomic activation

Import follows this order:

1. Validate the selected canonical provider summary.
2. Reject a known manual activity without a stream request.
3. Resolve a fresh cache hit or download the streams.
4. Validate the stream response.
5. Make a best-effort cache write according to the cache rules.
6. Map the validated response to canonical `RawActivity`.
7. Run the shared Rust finalizer to produce `ParsedActivity`.
8. Atomically activate the finalized activity and its canonical source descriptor.
9. Close the provider drawer after successful activation.

Only one import mutation may run at a time. Additional activity selections are unavailable until it settles.

Any download, cache-read validation, adaptation, or finalization failure leaves the existing active activity, summary, source identity, editor timing, and scene configuration unchanged. Partial activity state is never activated.

The raw provider response remains a valid cache entry if provider validation succeeds but downstream OVRLEY finalization fails. A cache write failure does not block activation and is not surfaced in the initial UI; the activity simply remains uncached.

Disconnecting Strava or replacing its account does not clear an already active activity from the editor.

## 8. Local Stream Cache

### 8.1 Location and privacy

Strava stream caches are plaintext JSON under:

```text
<OVRLEY documents directory>/activities/strava/
```

The current runtime-path owner determines the platform-specific `<OVRLEY documents directory>`. Cache files are protected only by normal OS user permissions. They are deliberately not encrypted.

The provider-neutral cache manager uses:

```text
<OVRLEY documents directory>/activities/<provider-id>/<provider-activity-id>.json
```

Provider activity IDs must be validated before path construction and must never be accepted as path fragments.

### 8.2 Envelope

Each file is one versioned JSON envelope:

```json
{
  "cacheSchemaVersion": 1,
  "providerId": "strava",
  "providerActivityId": "123456789",
  "fetchedAt": "2026-07-15T12:00:00Z",
  "response": {}
}
```

`response` contains the Strava stream JSON without field remapping. The writer may normalize insignificant JSON whitespace as part of producing the containing envelope, but it must not transform provider fields or values.

Writes use a temporary file in the destination directory followed by atomic replacement. A failed or interrupted write must not destroy a previous valid entry.

### 8.3 TTL and cleanup

- An entry is fresh for exactly 30 days from `fetchedAt`.
- Reading a cache entry does not extend its lifetime.
- Forced re-download resets `fetchedAt` only after a successful validated response replaces the entry.
- Expired entries are deleted during every OVRLEY startup, even if the Strava drawer is never opened.
- Expired entries are never used for import.
- There is no sliding expiration or persistent activity-list index.
- Stream caches remain until expiry after Disconnect, session revocation, or account replacement.

### 8.4 Cache and list independence

The monthly activity list is always based exclusively on the Worker's current or TanStack Query-cached Strava list response. OVRLEY never enumerates stream cache files to create activities in the drawer.

After receiving a monthly list, Rust checks cache envelopes for those returned activity IDs and reports transient cache status. Successful download updates that status locally without requiring a list refetch.

Old cache files from another connected account remain invisible unless the current Strava response independently contains the same globally identified activity.

### 8.5 Corruption

A present cache file with malformed JSON, a mismatched provider/activity ID, invalid envelope data, invalid provider response data, or an unsupported cache schema fails the current import with a typed cache-corruption error. OVRLEY then deletes that file. It does not silently convert the same import attempt into a network request. The next explicit import or force-download may fetch the streams again.

Optional absence and expiry are different from corruption: a missing or expired entry causes a normal download.

### 8.6 Forced re-download

Force-download bypasses a fresh cache entry. It downloads and validates into a temporary/in-memory representation and replaces the old cache atomically only after the new provider response is valid.

If the forced request fails, the old valid cache remains intact, the current editor activity remains active, and the requested failure is reported. OVRLEY does not silently load the old cache as if the forced refresh had succeeded.

### 8.7 Resource limits

- A raw stream response is limited to 64 MiB.
- The limit applies before unbounded JSON allocation or cache installation.
- Oversized responses fail with a typed error and do not replace an existing cache.
- Cache write failure is non-fatal after a valid response has been obtained; import may continue from memory.

## 9. Frontend Architecture

### 9.1 TanStack Query

Add TanStack Query as the provider server-state owner. Do not create a second custom caching implementation and do not put provider lists in Zustand.

Provider query keys are created centrally. The conceptual keys are:

```text
['activity-provider', providerId, 'connection']
['activity-provider', providerId, connectionId, 'activities', yyyyMm]
```

Monthly list behavior:

- `staleTime` is 10 minutes.
- `gcTime` is infinite for the current process so inactive monthly responses remain in memory unless explicitly cleared.
- TanStack Query retries are disabled because the native/Worker layers own the single agreed retry budget.
- Window-focus refetch, reconnect refetch, and polling are disabled for monthly lists.
- No query persistence plugin is used.
- No activity-list response survives application exit.
- The current month is fetched when the provider drawer first opens.
- Other months are fetched only when navigated to; adjacent months are not prefetched.
- A manual refresh invalidates and refetches only the visible month.
- Query keys prevent late results for one connection/month from replacing another.
- Duplicate requests for the same month are coalesced by TanStack Query.
- A failed stale refresh retains and displays the previous successful response with an explicit refresh error. With no previous response, only the error state is shown.

Connection status is checked lazily when the provider drawer first opens, not during normal OVRLEY startup. Once loaded, connection state is retained for the process and invalidated by login, replacement, disconnect, or definitive server revocation.

If the Worker definitively reports that a session is invalid, Rust deletes the local credential and the frontend removes that provider connection and all monthly queries. Stream cache files remain until normal expiry. Ordinary network failures retain the credential and report temporary unavailability.

Activity import and forced re-download use TanStack Query mutations. The mutation delegates all cache/network/adaptation/finalization behavior to Rust; TanStack Query does not hold stream payloads.

### 9.2 Reusable presentation

Provider presentation is split into reusable prop-driven components for:

- Connection state and connection actions.
- Month navigation.
- Activity summary lists.
- Unavailable/manual activity state.
- Local stream-cache status.
- List loading, stale-refresh error, and empty-month states.
- Single-activity import progress and failure.

Provider-specific containers supply the provider label, connection commands, query functions, and canonical summaries. Reusable components do not inspect Strava response fields.

Visual treatment, typography, spacing, icon choice, and final drawer styling are deferred. Functional UI requirements are:

- Manual activities remain visible but cannot be selected for import.
- Freshly cached stream entries expose a distinguishable cache state.
- A cached activity selection loads without a stream API request.
- A force-download action is available for cached activities.
- The drawer closes only after successful activation and remains open on failure.
- Further activity selections are unavailable while one import mutation is pending.

### 9.3 Drawer layout state

Widgets and activity providers share one mutually exclusive left-drawer state. Replace independent booleans with one canonical active-left-drawer value, conceptually:

```text
none | widgets | activity-provider
```

If multiple providers later share one provider drawer, provider selection is separate from whether that drawer is open.

### 9.4 Restart behavior

- Provider credentials persist in the OS credential store.
- Stream caches persist according to their 30-day TTL.
- Monthly list queries do not persist.
- The active imported activity does not persist or restore.
- Opening the provider drawer after restart validates the session and fetches the current month.
- Selecting a listed activity may then reuse its fresh disk cache.

## 10. Read-only and Security Requirements

- The Strava client secret exists only as a Cloudflare Worker secret.
- Strava access and refresh tokens exist only transiently in Worker memory and encrypted in D1.
- The desktop holds only an opaque OVRLEY session credential in the OS credential store.
- JavaScript receives no credential, authorization code, proof verifier, Strava token, or arbitrary Worker URL.
- The Worker requests no write scopes and exposes no mutating Strava endpoints.
- Browser URLs contain only short-lived attempt/state values, never access, refresh, or session tokens.
- Worker request bodies and responses containing private activity data are not logged or cached at Cloudflare.
- Authenticated responses use no-store cache headers.
- IPC commands validate provider IDs, activity IDs, month values, force flags, and canonical DTOs.
- Cache paths are constructed from validated canonical IDs and never from raw user paths.
- Activity-list and stream payloads are bounded and validated before use.
- Desktop debug builds retain the existing activity-finalizer diagnostic behavior, including possible writes under `debug/activities`; this is an explicit accepted exception to the managed cache location.

## 11. Failure Semantics

| Failure                                  | Required behavior                                                                     |
| ---------------------------------------- | ------------------------------------------------------------------------------------- |
| OS credential store unavailable          | Block Strava connection; do not downgrade storage                                     |
| OAuth denied, expired, or missing scope  | Preserve any existing session; report typed authorization state                       |
| Worker network failure                   | Keep credential; report temporary unavailability                                      |
| Worker reports session revoked           | Delete credential, clear connection/list queries, retain stream caches                |
| Monthly refresh fails with prior data    | Keep stale response and report refresh failure                                        |
| Monthly request fails without prior data | Show no list and report failure                                                       |
| Any monthly page is malformed/fails      | Reject the complete new month; never show a partial response                          |
| Manual activity selected                 | Do not call streams; report unavailable                                               |
| Streams absent or malformed              | Do not cache or activate; keep current activity                                       |
| Valid streams but finalization fails     | Keep valid raw cache; keep current activity; report finalization failure              |
| Cache missing or expired                 | Download normally                                                                     |
| Cache corrupt                            | Fail current import, report, delete corrupt entry; fetch only on next explicit action |
| Cache write fails                        | Continue finalization/activation silently; cache status remains uncached              |
| Forced download fails                    | Preserve prior valid cache and active activity; report failure                        |
| Strava/Worker network or 5xx failure     | Retry once, then report typed transient failure                                       |
| Strava rate limit                        | Do not retry; report reset/retry time when known                                      |
| Activity import fails at any stage       | Preserve the complete existing active editor state                                    |

## 12. Testing Requirements

### 12.1 Worker

- OAuth state and proof-verifier validation, one-time use, cancellation, and five-minute expiry.
- Required-scope rejection without replacing an existing session.
- D1 token encryption/decryption and key identifiers.
- Session-token hashing and one-session-per-profile replacement.
- Concurrent refresh serialization and rotated refresh-token persistence.
- One-year inactive-session cleanup.
- Token-specific local revocation.
- Month-range widening, activity-local filtering, pagination, canonical mapping, and no partial results.
- Fixed stream key/resolution parameters and no detail request.
- Retry, Strava 401 refresh, 429 translation, and no-store headers.
- Cloudflare rate-limit keys and route-specific bindings.
- Log redaction tests that ensure credentials and activity data are absent.

### 12.2 Rust/Tauri

- Mock credential-store success, locked/unavailable state, replacement, deletion, and no plaintext fallback.
- Mock HTTP client request origin/path allowlisting, retry, timeout, and 64 MiB limit.
- Provider registry rejects unknown IDs.
- Cache envelope round-trip, TTL, startup cleanup, atomic replacement, forced-refresh failure, and corruption deletion-after-reporting.
- Disconnect/session-revocation behavior retains stream files and active activity.
- Strava fixtures covering every documented stream, optional stream absence, manual/empty streams, non-finite values, misalignment, invalid coordinates, and malformed metadata.
- Exact Strava-to-`RawActivity` unit and field mapping.
- Canonical boolean `moving` alignment and synthetic idle behavior.
- Finalization through the existing shared pipeline.
- Atomic activation tests proving every pre-activation failure preserves prior editor state.
- Canonical source descriptors for local files, provider activities, and video telemetry.

### 12.3 Frontend

- Provider query-key isolation by provider, connection, and month.
- Ten-minute stale behavior, process-only cache, on-demand month loading, and no adjacent prefetch.
- Manual visible-month refresh.
- Stale list retained on refresh failure.
- Definitive session revocation clears provider queries; transient network errors do not.
- Cache status augments only Worker-returned list items.
- Manual activities remain visible and unavailable.
- Import mutation prevents concurrent selection.
- Successful import closes the drawer; failed import leaves it open and preserves active state.
- Widgets and provider drawer are mutually exclusive.
- Reusable presentation tests contain no Strava response-shape assumptions.

## 13. Acceptance Criteria

1. A user with an available OS credential store can authorize OVRLEY in the system browser without any Strava secret or token entering the desktop frontend.
2. Authorization persists across OVRLEY restarts under the same OS user profile and is shared by portable executable copies in that profile.
3. Opening the provider drawer lazily validates the connection and loads the current activity-local month.
4. Navigating to a month issues at most one coalesced query for that provider/connection/month while fresh, with a 10-minute stale time and no cross-process persistence.
5. The list shows every returned activity with the required summary fields; manual activities remain visible but unavailable.
6. Selecting an uncached activity makes one purpose-built stream operation, requests every documented high-resolution stream, validates it, caches the raw response, finalizes it, and activates it atomically.
7. Selecting a fresh cached activity makes no stream API request and finalizes from the validated cached response.
8. Forced re-download bypasses a fresh cache and cannot destroy the old cache or active activity when it fails.
9. Cache files expire 30 days after download and are cleaned on app startup; list membership never comes from those files.
10. Disconnect revokes the current token chain and clears credentials/lists without deleting stream caches or the currently active editor activity.
11. No Worker operation persists or logs activity summaries, routes, or metrics.
12. No failure path activates partial telemetry or clears the previous working activity.
13. Shared frontend and native infrastructure refers to canonical providers, not Strava response fields, and a future provider can supply its own Worker/API and `RawActivity` adapter without duplicating query, cache, drawer, source, or activation infrastructure.

## 14. External Prerequisites

- Register and configure the OVRLEY Strava API application with the Worker callback domain.
- Store the Strava client ID, client secret, and token-encryption key in Cloudflare Worker secrets.
- Provision D1 and apply the Worker migrations.
- Configure the dedicated `api.ovrley.cc` Worker route.
- Configure the four Cloudflare Rate Limiting bindings.
- Complete Strava's application review before broad production availability. New applications begin in Single Player Mode and have limited athlete capacity and shared read quotas.
- Satisfy Strava attribution and brand-guideline requirements when final UI styling is implemented; visual details are outside this specification.

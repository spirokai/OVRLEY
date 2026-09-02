Status: ready-for-agent

# Strava Activity Provider Integration

## Problem Statement

OVRLEY users currently have to export an activity from Strava, locate the resulting file, and import it manually before they can build or render an overlay. This interrupts the editing workflow, requires duplicate local files, and makes it harder to find older activities.

Users need a secure, read-only way to connect Strava, browse their own activity history, and import recorded telemetry directly into the same OVRLEY activity pipeline used by local files. The connection must remain convenient across application restarts without exposing Strava credentials to the desktop frontend or embedding a recoverable Strava client secret in the portable application.

The implementation also needs to establish a reusable activity-provider foundation. Strava is the first remote provider, but future providers should be able to reuse connection state, monthly browsing, query lifecycle, local stream caching, source identity, atomic activation, and presentation instead of creating parallel integration stacks.

## Solution

Add a read-only activity-provider drawer to OVRLEY. A user connects Strava in their system browser, after which OVRLEY lists activities one activity-local calendar month at a time. Selecting an importable activity downloads every documented high-resolution Strava stream, caches the raw response locally for 30 days, converts it into OVRLEY's canonical activity model, and activates it in the editor.

A purpose-built Cloudflare Worker remains between OVRLEY and Strava. It owns the Strava client secret, encrypted Strava access and refresh tokens, OAuth state, session lifecycle, read-only endpoint enforcement, and upstream token refresh. The desktop stores only an opaque OVRLEY session credential in the operating system credential store. React receives no credential and performs no provider network requests.

TanStack Query owns provider connection and monthly-list server state in the frontend. Rust owns native networking, secure credential access, local stream caching, strict provider validation, adaptation to `RawActivity`, and atomic finalization. The active editor state continues to use one canonical activity model and is extended with a provider-neutral source descriptor rather than Strava-specific state or fabricated filenames.

## User Stories

1. As an OVRLEY user, I want to connect my Strava account from the desktop application, so that I can import activities without exporting files manually.
2. As an OVRLEY user, I want authorization to open in my normal system browser, so that I can use my existing Strava login and password-manager workflow safely.
3. As a Strava user who signs in through Google, I want authorization to avoid an embedded WebView, so that the Strava login flow works reliably.
4. As a privacy-conscious user, I want OVRLEY to request only read access to activities, so that the integration cannot upload, edit, or delete my Strava data.
5. As a user with private activities, I want OVRLEY to request full read-only activity access, so that activities marked private or `Only You` are available to me.
6. As a privacy-conscious user, I want login to fail when the required read-only scope is not granted, so that OVRLEY never presents a misleading partially connected state.
7. As a returning user, I want my Strava connection to persist across OVRLEY restarts, so that I do not have to authorize every session.
8. As a portable OVRLEY user, I want every OVRLEY executable under my OS user profile to share one Strava connection, so that copying or replacing the executable does not create duplicate local sessions.
9. As a user of multiple computers, I want each computer's OS profile to have an independent Strava session, so that connecting or replacing one profile does not log out another computer.
10. As a connected user, I want a new successful login to replace my previous session for the current OS profile, so that at most one account is active there.
11. As a connected user replacing an account, I want the old session to remain usable until the new authorization succeeds, so that a cancelled or failed login does not leave me disconnected.
12. As a connected user, I want to see the Strava athlete ID and display name associated with the connection, so that I can confirm which account is active.
13. As a connected user, I want to disconnect only the current OS profile, so that independently connected computers remain usable.
14. As a privacy-conscious user, I want Disconnect to revoke the current Strava token chain, so that discarded Worker credentials cannot continue accessing Strava.
15. As an occasional user, I want my connection to remain active without frequent reauthorization, so that infrequent OVRLEY use remains convenient.
16. As a privacy-conscious user, I want abandoned sessions removed after one year of inactivity, so that lost machines or deleted credential entries do not leave indefinite Worker access.
17. As a user whose operating system credential store is unavailable, I want OVRLEY to block Strava connection clearly, so that my session is never downgraded to unsafe plaintext storage.
18. As a Windows user, I want the session stored in Credential Manager, so that it follows native credential-security behavior.
19. As a macOS user, I want the session stored in Keychain, so that it follows native credential-security behavior.
20. As a Linux desktop user, I want the session stored through Secret Service when available, so that Linux receives the same security boundary as other platforms.
21. As a Linux user without Secret Service, I want an explicit unavailable state, so that OVRLEY does not pretend persistence is secure.
22. As an OVRLEY user who does not use Strava during a session, I want the app to avoid connection checks at startup, so that the existing local workflow remains unaffected by network availability.
23. As a connected user, I want connection validation to happen when I open the activity-provider drawer, so that Strava data loads only when requested.
24. As a connected user, I want the current calendar month to load when the drawer opens, so that my newest activities are immediately available.
25. As a connected user, I want to navigate one month at a time, so that I can find older activities predictably.
26. As a traveling athlete, I want activities grouped by the local date where each activity occurred, so that late-night activities appear in the expected month.
27. As an athlete with many activities, I want OVRLEY to load every Strava page required for the selected month, so that no activities disappear because of API pagination.
28. As an athlete, I want each activity to show its name, distance, local date, elapsed duration, and sport type, so that I can identify the correct recording.
29. As an athlete, I want duration to use elapsed time rather than moving time, so that the displayed duration matches the timeline OVRLEY will import and synchronize.
30. As an athlete with private activities, I want those activities listed alongside my other activities, so that visibility settings do not make my own recordings unavailable to me.
31. As an athlete with manually entered summary activities, I want those activities to remain visible but unavailable for import, so that the list reflects Strava without implying telemetry exists.
32. As an athlete whose non-manual activity unexpectedly has no streams, I want a clear import failure, so that OVRLEY does not create a fabricated or partial timeline.
33. As a connected user, I want each month fetched only when I navigate to it, so that browsing does not waste Strava's shared API quota.
34. As a connected user, I want a fetched month to remain fresh for 10 minutes, so that repeated navigation is responsive without hiding changes indefinitely.
35. As a connected user, I want monthly list responses retained in memory for the current process, so that revisiting a month does not discard useful data unnecessarily.
36. As a privacy-conscious user, I want monthly activity lists forgotten when OVRLEY exits, so that the application does not build a persistent local index of my Strava history.
37. As a connected user, I want to refresh the visible month explicitly, so that newly uploaded, renamed, or removed Strava activities can appear immediately.
38. As a connected user, I want a failed refresh to keep showing the last successful month with an error, so that a temporary outage does not erase useful list context.
39. As a connected user with no prior month response, I want a failed request to show an error instead of a partial list, so that malformed or incomplete data is never presented as complete.
40. As a connected user, I want late results for another month or account ignored, so that rapid navigation and account replacement cannot show the wrong activity list.
41. As a connected user, I want duplicate month requests coalesced, so that repeated rendering or navigation does not consume extra Strava quota.
42. As an OVRLEY user, I want the Widgets drawer and activity-provider drawer to be mutually exclusive, so that left-side controls do not overlap or compete.
43. As an athlete, I want selecting an activity to import its recorded telemetry directly into OVRLEY, so that I can start editing an overlay without an exported file.
44. As an athlete, I want OVRLEY to request every documented Strava activity stream, so that all available route, timing, motion, environmental, and sensor data is retained.
45. As an athlete, I want streams requested at high time-based resolution, so that overlay values and video synchronization use the best telemetry Strava exposes.
46. As an athlete, I want time, distance, GPS, altitude, speed, heart rate, cadence, power, temperature, grade, and moving state mapped into canonical OVRLEY data, so that existing processing and widgets can consume them consistently.
47. As an athlete whose recording lacks a particular sensor, I want missing optional streams accepted, so that an activity without power or heart rate remains importable.
48. As an athlete, I want malformed present streams rejected rather than repaired, so that misaligned or invalid telemetry cannot silently produce a misleading overlay.
49. As an OVRLEY user, I want only one activity import to run at a time, so that rapid selection cannot race downloads or activate the wrong activity.
50. As an OVRLEY user, I want the provider drawer to close after successful activation, so that I can immediately inspect the imported activity in the editor.
51. As an OVRLEY user, I want the drawer to remain open when import fails, so that I can understand the failure or select another activity.
52. As an OVRLEY user with an activity already loaded, I want every provider import to finish successfully before replacing it, so that a failure never clears my working editor state.
53. As an OVRLEY user, I want imported provider activities to use the same finalization, synchronization, preview, and rendering pipeline as local activity files, so that provider origin does not change overlay behavior.
54. As an OVRLEY user, I want the active activity to retain its real provider identity and display name, so that Strava data is not disguised as a fake FIT or GPX file.
55. As an OVRLEY user, I want disconnecting or replacing Strava to leave the currently active editor activity intact, so that account management does not destroy current work.
56. As a returning user, I do not want OVRLEY to restore the prior active Strava activity automatically, so that restart behavior remains consistent with the current session-only editor model.
57. As an OVRLEY user, I want downloaded stream responses cached for 30 days, so that repeatedly using the same activity does not consume Strava quota or require another download.
58. As an OVRLEY user, I want a cached activity marked in the current Strava-derived list, so that I know which imports are available locally.
59. As an OVRLEY user, I want selecting a fresh cached activity to avoid the stream API entirely, so that loading is faster and works without spending upstream quota.
60. As an OVRLEY user, I want cache use not to extend the 30-day lifetime, so that frequently used telemetry is eventually refreshed.
61. As an OVRLEY user, I want expired stream caches removed at application startup, so that files do not remain indefinitely merely because I stop opening the provider drawer.
62. As an OVRLEY user, I want cached streams retained until expiry after disconnect or account replacement, so that the agreed local retention policy remains independent of account state.
63. As a privacy-conscious user, I want local stream caches stored only under OVRLEY's activity/provider directory, so that I can locate and manage the plaintext telemetry files.
64. As an OVRLEY user, I want local cache contents never to create entries in the activity list, so that the list reflects only the current Strava response.
65. As an OVRLEY user, I want a valid provider response retained in cache even if the current OVRLEY finalizer fails, so that retrying a local bug does not waste another Strava request.
66. As an OVRLEY user, I want a cache write failure not to block a valid import, so that disk-cache availability is not a prerequisite for using downloaded telemetry.
67. As an OVRLEY user, I want corrupt cache data reported and removed, so that malformed present data is not silently treated as a cache miss.
68. As an OVRLEY user, I want the next explicit action after a corrupt-cache error to download the activity again, so that recovery remains deliberate and understandable.
69. As an OVRLEY user, I want to force a stream re-download for a cached activity, so that I can replace stale or changed telemetry before the normal expiry.
70. As an OVRLEY user, I want a failed forced re-download to preserve the previous valid cache, so that a transient outage does not destroy usable data.
71. As an OVRLEY user, I want a successful forced re-download to replace the cache atomically, so that interrupted writes cannot leave a partial JSON file.
72. As an OVRLEY user, I want oversized stream responses rejected at a bounded limit, so that malformed or compromised responses cannot exhaust application memory or disk.
73. As a connected user, I want temporary Worker or Strava failures retried once, so that brief network interruptions can recover without repeated quota consumption.
74. As a connected user, I want rate-limit failures to show when retry may be possible, so that I do not repeatedly submit requests that Strava will reject.
75. As a connected user, I want OVRLEY not to retry rate-limit failures automatically, so that it does not consume the next quota window without my action.
76. As a connected user, I want definitive session revocation to clear the unusable local credential and in-memory lists, so that OVRLEY returns to a truthful disconnected state.
77. As a connected user, I want ordinary network failure to retain my credential, so that an outage is not mistaken for account revocation.
78. As a privacy-conscious user, I want Strava client and token secrets to remain on the Worker, so that extracting the portable desktop binary does not reveal them.
79. As a privacy-conscious user, I want the Worker to avoid persisting or edge-caching activity summaries and streams, so that my route and health data are not retained in Cloudflare storage.
80. As a privacy-conscious user, I want Worker logs redacted, so that credentials, account identity, activity IDs, names, coordinates, and health metrics are not captured by observability.
81. As an OVRLEY maintainer, I want the Worker API limited to purpose-built read operations, so that the desktop cannot turn it into a generic Strava proxy.
82. As an OVRLEY maintainer, I want Worker abuse limits based on sessions and short-lived attempts, so that one client cannot consume unrestricted service capacity.
83. As an OVRLEY maintainer, I want Strava token refresh serialized and rotated tokens stored atomically, so that concurrent reads cannot invalidate a working connection.
84. As an OVRLEY maintainer, I want provider credentials and network calls owned by Rust, so that browser code and DevTools cannot inspect them.
85. As an OVRLEY maintainer, I want provider query state isolated from Zustand editor state, so that remote-data lifecycle does not complicate media and scene mutations.
86. As an OVRLEY maintainer, I want all remote and cache boundaries to return typed errors, so that UI behavior does not depend on parsing error strings.
87. As an OVRLEY maintainer, I want provider activity IDs validated before path construction, so that remote identifiers cannot become arbitrary filesystem paths.
88. As an OVRLEY maintainer, I want provider imports to converge on `RawActivity` and the shared Rust finalizer, so that telemetry derivation and rendering remain canonical.
89. As an OVRLEY maintainer, I want the source model to represent local files, providers, and video telemetry explicitly, so that consumers do not rely on filename aliases or parallel source fields.
90. As a future provider implementer, I want reusable credential, HTTP, cache, query, drawer, and activation infrastructure, so that a new provider requires only genuine provider-specific behavior.
91. As a future provider implementer, I want provider-specific API responses normalized once at their adapter boundary, so that shared consumers remain independent of external naming schemes.
92. As a future provider implementer, I want query keys and cache paths namespaced by provider identity, so that two providers cannot overwrite or display each other's data.
93. As a future provider implementer, I want reusable presentational components to consume canonical activity summaries, so that new integrations do not duplicate month navigation and list states.
94. As a future provider implementer, I want the provider abstraction to remain narrower than Strava's API model, so that a different provider is not forced into accidental Strava semantics.

## Implementation Decisions

### Module boundaries

- The implementation is divided into five major modules: the Cloudflare Worker service, the native provider platform, the pure Strava adapter, the reusable frontend provider feature, and canonical activity-source/layout integration.
- The Cloudflare Worker service lives with the existing website infrastructure and owns deployment configuration, D1 migrations, secrets, OAuth, Worker sessions, Strava token lifecycle, monthly-list aggregation, stream proxying, rate limits, and redacted observability.
- The native provider platform owns OS credential access, the pinned Worker HTTP client, authorization-attempt orchestration, typed provider operations, local stream-cache mechanics, resource limits, and narrow Tauri IPC.
- The pure Strava adapter owns strict stream-response validation and conversion into canonical `RawActivity`; it remains independent of Tauri, keyrings, Cloudflare, and React.
- The reusable frontend provider feature owns TanStack Query keys and lifecycle, connection and month orchestration, import mutations, and prop-driven provider presentation.
- Canonical activity-source/layout integration replaces file-centric source identity and independent drawer booleans with one source descriptor and one mutually exclusive active-left-drawer state.

### Deep modules

- OAuth and Worker session lifecycle form one deep module with a narrow API for starting, observing, exchanging, validating, replacing, and disconnecting sessions. It hides OAuth state, proof verification, D1 encryption, refresh rotation, and token revocation.
- Native stream-cache management forms one deep module with operations for status, fresh reads, validated writes, forced replacement, corruption handling, and startup expiry cleanup. Provider-specific validators are supplied at the boundary; filesystem mechanics remain shared.
- Strava stream adaptation forms one pure deep module: validated summary plus validated stream response in, canonical `RawActivity` out.
- Provider query orchestration forms one frontend deep module that owns canonical keys, stale/retention policy, invalidation, connection replacement, and mutation concurrency while exposing presentation-ready states.

### Provider-neutral contracts

- Provider identifiers are canonical allowlisted values. `strava` is the first value; arbitrary endpoints and dynamically supplied provider names are rejected.
- The canonical connection contract includes provider identity, a connection identity, athlete ID, display name, and an explicit connection state.
- The canonical provider activity summary includes provider/activity identity, name, distance in meters, absolute UTC start, activity-local start, elapsed seconds, sport type, and whether the activity is manual.
- Provider activity IDs are represented as strings after ingress even when an upstream API uses numeric identifiers.
- Shared frontend and Rust consumers never inspect Strava response fields.
- Provider-specific adapters are allowed only at the external provider boundary and normalize once into canonical OVRLEY contracts.
- A typed provider error vocabulary distinguishes credential, authorization, session, network, upstream, rate-limit, provider-data, cache, size-limit, and finalization failures.

### Canonical activity source and activation

- The active activity uses one canonical source descriptor with explicit variants for local files, remote providers, and video telemetry.
- A provider source carries provider ID, provider activity ID, and display name. It does not fabricate a local filename or format.
- Existing activity-summary, preview, synchronization, geometry, and rendering consumers migrate to the canonical source descriptor rather than receiving Strava aliases.
- Provider streams map into `RawActivity` and pass through the existing Rust finalizer; providers do not construct `ParsedActivity` independently.
- Strava's `moving` stream becomes a canonical aligned boolean activity series even though no current widget renders it.
- Import is an atomic replacement. The complete response is resolved, validated, adapted, and finalized before one store transition activates it.
- Every failure before activation preserves the prior active activity, source, summary, editor timing, and scene configuration.
- Disconnect and account replacement do not clear an activity already active in the editor.
- Active activity restoration after application restart remains out of scope.

### Worker security and API

- The production API is versioned under the dedicated `api.ovrley.cc` origin. Desktop builds use only the compiled production origin; runtime endpoint overrides are not supported.
- OVRLEY is treated as a public desktop client. No shared Worker API secret is embedded in the application.
- The Worker remains in the data path for all Strava operations and exposes only purpose-built authorization, session, monthly-list, and stream operations.
- The Worker requests only `activity:read_all`. Missing required scope rejects the new authorization and preserves an existing session.
- The Worker makes no upload, edit, delete, comment, kudos, route, club, segment, webhook, or generic proxy operation available.
- Activity streams are fetched by one stream request; no activity-detail request is added.
- The Worker may trust the authenticated session's requested activity ID and rely on Strava's access control. Signed activity capabilities are not required.
- Activity data receives no-store response headers and is not persisted or edge-cached by Cloudflare.
- Worker application logs retain only request ID, fixed route, status, coarse error code, latency, and Strava quota headers. Credentials, OAuth values, account identity, activity identity, and payloads are excluded.

### OAuth and session lifecycle

- Authentication opens the system browser and completes through short-lived polling. Deep links and desktop loopback listeners are not used.
- Rust creates a high-entropy proof verifier/challenge and a stable random OS-profile identity. The Worker creates exact-match one-time OAuth state.
- Browser callback URLs never carry a Strava token or OVRLEY session token.
- Authorization attempts expire after five minutes and are invalidated after exchange, cancellation, or expiry.
- The Worker exchanges the Strava code with its server-held client secret and validates the accepted scope before issuing an OVRLEY session.
- The desktop receives one random 256-bit opaque OVRLEY session credential over HTTPS and stores it in the OS credential store. JavaScript never receives it.
- D1 stores only a keyed hash of the OVRLEY session credential.
- One active session is allowed per stable OS-profile identity. A successful replacement supersedes the old session; other OS profiles remain independent.
- Sessions persist until disconnect, replacement, provider revocation, unrecoverable token failure, or one year of inactivity.
- Disconnect uses Strava's token-specific revoke operation rather than global application deauthorization.
- Abandoned one-year sessions are revoked on a best-effort basis and deleted by scheduled cleanup.

### Worker persistence and token refresh

- D1 stores short-lived authorization attempts, hashed session/profile associations, minimal athlete ID/display name, granted scopes, encrypted Strava tokens, token expiry/refresh coordination, and session timestamps.
- D1 does not store activity summaries, activity authorization lists, or stream responses.
- Strava access and refresh tokens use authenticated application-level encryption with a Worker-secret key, per-record nonces, and key identifiers for rotation.
- Token refresh is serialized with a transaction-safe D1 lease or equivalent mechanism.
- The latest refresh token returned by Strava always replaces the prior token atomically.
- Access tokens refresh automatically before expiry. An unexpected Strava 401 permits one coordinated refresh and one retry.
- Definitive refresh or revocation failure invalidates the OVRLEY session.

### Monthly list contract

- The Worker accepts a validated calendar month and returns a complete canonical response for that activity-local month.
- The Worker widens the UTC query interval enough to cover valid timezone offsets, paginates the authenticated-athlete endpoint fully, filters by validated activity-local start, and rejects partial pages.
- Month membership uses the activity's local date, not UTC or the desktop's current timezone.
- Elapsed time is the canonical displayed/import duration; moving time is not used as a replacement.
- Manual activities remain in the list and are explicitly unavailable for import.
- The list response determines all visible activities. Local stream-cache enumeration never adds an item.

### Stream contract and adaptation

- The Worker explicitly requests every documented Strava activity stream: time, distance, latitude/longitude, altitude, smoothed velocity, heart rate, cadence, watts, temperature, moving state, and smoothed grade.
- Requests use high resolution, time-based series, and keyed stream objects.
- The successful provider response reaches Rust without field remapping and is the representation stored inside the local cache envelope.
- A non-empty, finite, non-negative, monotonically ordered time stream is required.
- Present streams require valid metadata, element types, finite values, valid coordinates, and compatible sample alignment.
- Documented stream absence is valid optionality. Malformed present streams are rejected rather than repaired.
- The adapter maps provider units and names once into canonical OVRLEY samples and supplies explicit source-processing options to the shared finalizer.

### Native credential boundary

- Rust performs all Worker networking and all OS credential access. React calls only narrow Tauri IPC operations and receives canonical non-secret results.
- The native credential implementation uses operating-system keyrings through Rust: Credential Manager on Windows, Keychain on macOS, and Secret Service on Linux.
- Missing, locked, or unsupported credential storage blocks provider connection. There is no plaintext, Tauri Store, environment-variable, or memory-only fallback.
- All released desktop platforms are included. Linux systems without Secret Service receive the explicit unavailable state.
- IPC validates provider IDs, month values, activity IDs, force flags, and canonical payloads. It cannot accept arbitrary origins, URLs, paths, headers, scopes, or credentials.
- Network, credential, clock, and cache-filesystem dependencies are injectable so the native platform can be tested without real keyrings or Strava access.

### Local stream cache

- Provider stream caches use the existing OVRLEY documents root, namespaced by provider and validated provider activity ID.
- Strava cache files are plaintext JSON protected by normal OS user permissions; local encryption is not required.
- Each cache file is one versioned JSON envelope containing schema version, provider identity, provider activity identity, UTC fetch timestamp, and the unchanged provider response.
- Files are fresh for exactly 30 days from download. Reads do not extend expiry.
- Expired entries are removed during every OVRLEY startup, even when the provider drawer is never opened.
- Cache files remain until expiry after disconnect, revocation, or account replacement.
- Successful list responses are augmented with local cache status only after the list has been received.
- Cache writes use temporary files and atomic replacement. Failed or interrupted writes preserve a previous valid entry.
- A corrupt present entry fails the current import, reports a typed error, and is then deleted. It does not silently become a network request during that same action.
- A missing or expired entry downloads normally.
- Force-download bypasses a fresh entry and atomically replaces it only after the new provider response validates.
- A failed forced download retains the prior valid cache and reports the failure without falling back silently.
- A provider-valid response is cached even when downstream OVRLEY finalization fails.
- Cache write failure is non-fatal and silent for the initial import; finalization may continue from memory and local cache status remains uncached.
- Raw stream responses are bounded at 64 MiB before unbounded allocation or cache installation.

### TanStack Query and frontend state

- TanStack Query is added as the owner of provider connection and activity-list server state. Provider lists do not enter Zustand.
- Query keys include provider ID; monthly keys also include connection ID and calendar month.
- Monthly queries use a 10-minute stale time and process-lifetime garbage-collection time.
- No query persistence plugin is used, so monthly lists disappear on process exit.
- Current month loads when the provider drawer first opens. Other months load on navigation only, with no adjacent prefetch.
- Window-focus refetch, reconnect refetch, polling, and TanStack's own automatic retries are disabled for monthly lists.
- Manual refresh invalidates only the visible month.
- A failed stale refresh retains previous data and exposes the refresh error.
- Connection state is checked lazily on first provider-drawer open and invalidated after login, replacement, disconnect, or definitive revocation.
- A definitive invalid session removes the local credential and provider queries. Temporary network failure preserves both.
- Activity import and force-download are TanStack Query mutations that delegate stream data and disk-cache ownership to Rust.
- Only one import mutation may be active. Successful activation closes the drawer; failures leave it open.

### Reusable presentation and layout

- Reusable prop-driven presentation covers connection states, month navigation, canonical activity summaries, manual/unavailable state, cache status, loading, stale errors, empty months, import progress, and import failure.
- Provider-specific containers supply provider identity, query functions, connection actions, and canonical data; reusable components do not inspect provider responses.
- Manual activities remain visible and non-importable.
- Cached activities expose a distinct functional state and a force-download action.
- Widgets and activity providers use one mutually exclusive active-left-drawer state.
- Final typography, spacing, icons, colors, and visual treatment are deferred.

### Retry and rate-limit behavior

- One retry is allowed at the layer that owns a retryable failure; retries do not compound across TanStack Query, Rust, and the Worker.
- Rust retries a Worker transport or retryable Worker 5xx once.
- The Worker retries a Strava transport or retryable Strava 5xx once and returns a typed terminal upstream error if that retry fails.
- Rate-limit responses are not retried automatically and include a retry time when it can be determined.
- Cloudflare Workers Rate Limiting bindings provide configuration-only abuse controls without D1 request counters.
- Initial development limits are 10 OAuth starts per IP per minute, 40 authorization polls per attempt per minute, 30 monthly lists per session per minute, and 15 stream downloads per session per minute.
- Strava's shared application quota remains authoritative. Binding values are expected to change after application approval and production telemetry.

## Testing Decisions

- Automated behavior tests are required at all five module boundaries: Worker service, native provider platform, pure Strava adapter, frontend provider feature, and canonical activity-source/layout integration.
- Good tests assert externally observable contracts and state transitions rather than private helper structure, library call counts that are not contractual, or visual styling details.
- Security tests must prove that secrets and sensitive activity data do not cross or persist beyond their defined boundaries.
- Failure tests must begin from an existing valid activity/session/cache where relevant and prove that the valid state is preserved.
- Provider contract tests use representative fixed fixtures, including every documented stream, optional missing streams, private/manual summaries, empty streams, malformed metadata, non-finite values, invalid coordinates, and misaligned samples.
- Worker OAuth tests cover exact state matching, proof verification, one-time exchange, denial, cancellation, five-minute expiry, missing required scope, and preservation of a prior session.
- Worker session tests cover keyed credential hashing, one-session-per-profile replacement, independent profiles, one-year inactivity cleanup, token-specific disconnect, and definitive revocation.
- Worker token tests cover authenticated encryption, key identifiers, concurrent refresh serialization, latest-refresh-token persistence, pre-expiry refresh, unexpected 401 recovery, and terminal refresh failure.
- Worker API tests cover complete activity-local month pagination, widened timezone intervals, no partial list, canonical summary mapping, fixed all-stream request parameters, no activity-detail request, no-store headers, one-retry ownership, 429 translation, and rate-limit keys.
- Worker observability tests capture emitted records and prove that authorization values, tokens, athlete identity, activity identity, names, list bodies, coordinates, and telemetry are absent.
- Native credential tests use an injected fake keyring to cover success, persistence, lock/unavailable behavior, replacement, disconnect, and prohibition of plaintext fallback.
- Native HTTP tests use an injected fake transport to cover pinned origin/path behavior, canonical request construction, one-retry limits, timeouts, typed errors, and the 64 MiB response ceiling.
- Native cache tests cover versioned envelope round trips, exact TTL, startup cleanup, cache-status lookup, atomic writes, retained old cache after failed force-download, non-fatal write failure, valid cache after finalization failure, and corruption report-then-delete behavior.
- Pure Strava adapter tests cover strict ingress validation, every field/unit mapping, optional absence, aligned boolean moving state, provider source identity, shared idle processing, and finalization through the canonical pipeline.
- Atomic activation tests prove that network, provider validation, cache-read, adaptation, and finalization failures leave all existing editor/media state unchanged.
- Source migration tests cover local file, provider, and video-telemetry source variants and prove that render/preview consumers no longer require fabricated filenames or source aliases.
- Frontend TanStack Query tests use an isolated QueryClient and mocked IPC to cover provider/connection/month key isolation, 10-minute stale behavior, process-only retention, no prefetch, no focus/reconnect polling, visible-month invalidation, request coalescing, stale-data preservation, and no compounded retries.
- Frontend connection tests cover lazy validation, successful login replacement, required-scope error state, temporary network failure, definitive revocation cleanup, and disconnect without stream-cache or active-activity removal.
- Frontend import tests cover manual activity unavailability, one active mutation, cached versus uncached behavior, force-download dispatch, successful drawer closure, failed drawer retention, and preservation of current editor state.
- Frontend layout tests cover mutual exclusion between Widgets and activity-provider drawers.
- Reusable presentation tests use canonical summaries from more than one synthetic provider identity so accidental Strava-field coupling is detectable before a second real provider exists.
- Prior art includes the existing Vitest and Testing Library component/hook tests, Zustand slice behavior tests, activity-import boundary tests, Widget Drawer tests, Rust activity integration tests, inline unit tests, command tests, and dependency-injected native service tests.
- A small manual integration pass with a Strava development athlete remains required for system-browser authorization, real keyring behavior on Windows/macOS/Linux, private-activity visibility, a real high-resolution stream import, local cache reuse, and token-specific Disconnect.
- Visual styling and Strava brand-compliance review are manual follow-up concerns and are not substitutes for the required functional tests.

## Out of Scope

- Uploading files or activities to Strava.
- Creating, editing, deleting, commenting on, or giving kudos to Strava activities.
- Clubs, routes, segments, athlete statistics, social data, activity zones, laps, and activity-detail requests.
- Strava webhooks or background synchronization.
- Worker-side or Cloudflare edge caching of private activity lists or streams.
- Persisting monthly activity lists across OVRLEY restarts.
- Automatically restoring the previously active provider activity after restart.
- Multiple simultaneously remembered provider accounts within one OS profile.
- Global Strava deauthorization from OVRLEY; local Disconnect revokes only the current token chain.
- Encrypted local stream-cache files or a password-protected portable vault.
- Memory-only or plaintext fallback when the OS credential store is unavailable.
- Deep-link or localhost-loopback OAuth callback handling.
- A generic Worker proxy for arbitrary Strava endpoints.
- Signed per-activity capabilities or an extra activity-detail ownership request.
- Automatic retry after rate limiting.
- Dynamic acceptance of undocumented future Strava streams.
- Rendering a widget from the canonical `moving` series in this feature.
- Final drawer styling, icon selection, typography, spacing, and animation.
- Restyling the existing Widget Drawer.
- Refactoring unrelated activity-import behavior except where required for the canonical source descriptor and atomic shared activation path.

## Further Notes

- The detailed engineering contract is captured in the companion Strava integration specification in the same feature directory. The PRD is authoritative for product scope and outcomes; the specification is authoritative for settled low-level contracts.
- This feature introduces OVRLEY's first runtime cloud dependency. Local file import, editing, preview, and rendering must continue to work without network access.
- Worker source and deployment live in the separate OVRLEY website repository, while desktop/provider infrastructure and the Strava adapter live with the OVRLEY desktop codebase.
- Broad production availability depends on Strava application approval. New Strava applications begin with limited athlete capacity and shared read quotas.
- Final UI work must satisfy Strava attribution and brand guidelines, but visual decisions are intentionally deferred from this PRD.
- Local Strava stream caches are intentionally plaintext under OVRLEY's documents activity directory. Existing debug-build activity diagnostics may also contain finalized Strava telemetry outside the managed 30-day cache; retaining current debug behavior is an accepted decision.
- Rate-limit binding values are initial development configuration, not permanent product limits.
- Future providers may use different Workers, authorization flows, response contracts, and adapters. They should still reuse canonical summaries, query lifecycle, keyring abstraction, cache mechanics, source identity, atomic activation, drawer behavior, and presentation where those contracts apply.

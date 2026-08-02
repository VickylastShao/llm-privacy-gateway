# Privacy notice

LLM Privacy Gateway is a local-first desktop application. The application does not provide an
account service, cloud storage, analytics, advertising, or application telemetry.

## Data that stays on the device

- Upstream API keys, local gateway keys, mapping keys, and per-connection custom sensitive terms
  are stored through Windows Credential Manager.
- Connection metadata, encrypted pseudonym mappings, bounded privacy activity metadata, and
  provider-reported Token totals are stored in the application's local SQLite database.
- Request bodies, response bodies, plaintext pseudonym mappings, and plaintext detected values are
  not written to the activity database or application logs.
- Privacy activity and usage metadata are retained for seven days by the current release.
- When the add-connection form opens, environment import automatically checks only the supported
  API-key and Base-URL variable names. Selecting and importing a discovered configuration remains
  explicit. Discovered raw keys remain in backend memory, never enter the WebView, and are cleared
  after import, cancellation, or five minutes.

## Data that leaves the device

When protection is running, supported API requests are inspected locally and then forwarded to the
upstream Base URL selected by the user. Depending on the selected policy, a request may be blocked,
masked, pseudonymized, or forwarded after monitoring. The chosen upstream provider receives the
resulting forwarded request under that provider's own terms and privacy policy.

Connection tests and supported balance queries send authenticated requests directly to the selected
provider. The application does not send those requests to the project's maintainer.

Traffic that does not use the local gateway Base URL is outside the application's protection.

## Local deletion

Deleting a connection removes its profile, stored credential entries, custom dictionary, and
associated pseudonym mappings. Uninstalling the Windows application preserves the application data
directory so that reinstalling or upgrading does not silently erase configuration. A user who wants
complete local deletion must first remove connections in the application and then delete the
application data directory after uninstalling.

If a release changes the Windows application identifier, the application copies the legacy SQLite
database into the new data directory using an integrity-checked atomic migration. It does not
overwrite an existing destination database and does not delete the legacy source database. The
legacy directory therefore remains local until the user removes it.

## Detection limits

Sensitive-data detection is rule based and cannot guarantee complete detection. The activity view
reports the categories and actions applied to supported requests without displaying the original
sensitive values.

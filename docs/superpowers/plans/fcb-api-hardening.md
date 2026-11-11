# FCB API hardening

Six commits, each with regression coverage:
1. Deterministic writer-generated local and loopback HTTP fixtures; injectable AppConfig.
2. Fail startup on invalid dataset metadata; fresh /readyz probes.
3. Validate pagination and combined queries; serialize pagination URLs.
4. Typed errors; absent IDs 404 and unavailable storage 503.
5. Flask envelopes, header metadata, optional COLLECTION_VERSION.
6. Explicit CRS aliases, standard axis order, truthful Content-Crs.

Exclude concurrency refactoring and documentation endpoints/schema updates.
Validate whole oracle documents and fresh writer round trips. Run formatting,
Clippy, workspace checks/tests and release build. Dataset changes require restart.

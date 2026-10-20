# Plan

Steps and their state. Updated as work lands.

## Steps

- [x] Investigate the environment, the original and its release artifacts;
      record findings in `docs/rea-investigation.md`.
- [x] Record the agreed decisions as ADRs 1–8 and the glossary.
- [x] Scaffold the project from the official applet template and build the
      unmodified baseline; add build dependencies and the toolchain pin.
- [x] Domain models and store: schema, migrations, deduplication, retention,
      sensitive purge, with unit tests.
- [x] Sensitive detection ported with the original's corpus as a test fixture.
- [x] Watcher: clipboard subscription thread with the MIME preference order,
      payload cap, repeat and self-copy rules, tested at the logic level.
- [x] Popup: search field, date-grouped list, pin/delete/clear, incognito
      toggle, empty and no-results states.
- [x] Clipboard write and paste: item back on the clipboard, `Ctrl+V`
      injection, copy-only action.
- [x] D-Bus interface and `--toggle` CLI mode; singleton guard.
- [ ] Validate on the live COSMIC session: applet in the panel, capture, search,
      pin, delete, clear, incognito, sensitive masking, paste into an editor.
- [x] Documentation: README, architecture, compatibility, tests; CI workflow.
- [ ] Confirm a real paste into a focused window (human click in the popup);
      the protocol path itself is covered by the ignored live test.

## V1.1 (0.2.0)

- [x] Paste lifecycle: wait for the clipboard write and the popup close event,
      inject once, with a safety deadline; the gate is unit-tested.
- [x] Paste failure notification over the session bus, without clipboard
      content.
- [x] Shortcut install/uninstall CLI with conflict detection, idempotency and
      integration tests against a temporary cosmic-config home.
- [x] Install/uninstall recipes wire the shortcut; README, ADR, compatibility
      and glossary updated.
- [x] Native popup polish: padding, spacing, single-line previews, right-aligned
      timestamps, two-row footer, empty and confirmation states.
- [x] Version bumped to 0.2.0.
- [ ] Confirm click-to-paste into a real focused editor on the live session and
      record the result.

## Ground rules

- Incremental: build and test after each step; review the diff between steps.
- No commits are created automatically.
- User data is never touched by tests: temporary directories only.
- No clipboard content in logs; no network calls at all.

## Known gaps to close before aiming at "done"

- Paste target validation must be manual (needs a real focused window).
- An image path exists in the schema for phase 2 but no image capture is
  implemented in v1.

# Story 4.3: The scene state in core: zones, alerts and escalation

**Status:** Review (gate 4 recorded in research notes E81 and its question shape added to the vectors)

## Story

As the author who must trust that a figure stands where its session really is,
I want core to decide, from the board's facts alone, which zone each live session belongs in and whether it is in alert, with the time escalation computed at the board's clock and the last known alert kept while data is missing,
so that the scene never classifies, missing data shows as unknown without ever clearing an alert, and an alert reaches the window within a tick of its threshold without spawning anything.

## Acceptance Criteria

1. Types (AD-167, AD-68 as amended): `crates/officina-core/src/scene.rs` defines `Zone` (`reception`, `office`, `decision`, `alert`, `lounge`, `unknown`) and `AlertCause` (`priority`, `waited`) with `closed_enum!` and `ALL`, and `SceneState { zone, waiting_since_ms: Option<i64>, alert: Option<AlertCause>, approval: bool }`; `Card.scene: Option<SceneState>` is `None` for To resume and Done cards and `Some` for every live card; `Board.alert_count: u32` and `Board.next_escalation_at_ms: Option<i64>` exist; `BoardInput.previous_scene` maps card keys to the `SceneState` of the previous board, empty by default; TypeScript bindings regenerated under story 4.2's bump. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; `scene.rs::tests::ended_cards_have_no_scene_state`.
2. Rules (AD-167 as amended, D-462 to D-465, D-482, D-494 to D-496), first match wins: a stale board gives `unknown`; the `unknown` badge gives `unknown`; a Needs you card, or a Working card whose session has a pending approval, is waiting, with `waiting_since_ms` the latest blocking notification after the last prompt and the last stop (Needs you) or the earliest pending approval's `requested_at_ms` (approval only, `approval = true`), and is `alert` when its project's priority is high (`priority`) or, for Needs you only, when its known wait reached `alert_after_ms` (`waited`), else `decision`; a Your turn card whose latest `SessionStart` has `source` `startup` and no `UserPromptSubmit` since is `reception`; another Your turn card is `lounge`; a Working card is `office`. An `idle_prompt` notification changes nothing. Proof: `tests/vectors/scene.json` replayed by `crates/officina-core/tests/scene_vectors.rs::every_case_places_its_cards`, with cases at least for: each zone; reception only on evidence (no hook events gives lounge); a `SessionStart` with `source` `compact`, `resume`, `clear` or `fork`, and one without a source, then a `Stop`, each gives lounge; a `startup` start followed by a `compact` start gives lounge; an `idle_prompt` after a `Stop` stays lounge; a Needs you without a blocking notification has no known start and never escalates by time; a high-priority Needs you is in alert at once; a wait one millisecond short of the threshold and one exactly at it; an approval-only wait on a Working card queues with `approval = true`; the same approval on a Your turn card is not waiting (lounge); an approval-only wait older than the threshold is not in alert on a normal project and is on a high-priority one; Needs you plus an approval keeps Needs you's start; a stale board makes every live card unknown; an unknown badge overrides waiting; the question shape gate 4 recorded (story 4.1).
3. Unknown keeps the last known alert (D-494): a card placed `unknown` by rule 1 or 2 keeps the `alert` and `waiting_since_ms` its key has in `previous_scene`, and never gains an alert; a card whose key is absent from `previous_scene` has none; the next board that places it in a known zone computes its alert afresh. Proof: vector cases with `previous_scene`: "a stale board keeps an alerted card's alert", "an unknown badge keeps an alerted card's alert", "an unknown card past the threshold gains no alert", "fresh data in the lounge clears the alert", "a first board while stale has no alert"; `scene.rs::tests::alert_count_counts_unknown_cards_that_keep_their_alert`.
4. Coverage (AD-14's pattern): a test iterates `Zone::ALL` and `AlertCause::ALL` and fails when a value appears in no case's expectations; the vector file is exempt from every formatter (listed in the formatter ignore files). Proof: `scene_vectors.rs::every_zone_and_cause_has_a_case`; `cargo fmt --check` and `pnpm lint` leave the file untouched in CI.
5. Board order and counts (AD-167, D-465): `columns.needs_you` keeps its order, so digits keep acting on it and an alerted session keeps its number; `alert_count` counts cards with an alert, unknown ones included; `waiting_count` is unchanged. Proof: `scene.rs::tests::{board_order_is_unchanged_by_alerts, alert_count_counts_alerted_cards}`.
6. Next escalation (AD-167, AD-168, D-467, D-494, D-496): `next_escalation_at_ms` is the earliest `waiting_since_ms + alert_after_ms` over Needs you cards in a known zone without an alert and with a known start, `None` otherwise; approval-only and unknown cards never contribute; a board built at that instant has the card in alert and the next value moved on. Proof: `scene_vectors.rs` cases with `expect_next_escalation_at_ms` (one with an approval-only wait and an unknown card that must not set it); `scene.rs::tests::building_at_the_next_escalation_alerts_the_card`.
7. The collector wakes for it and remembers (AD-168, D-467, D-494): when the 250 ms tick reaches the last board's `next_escalation_at_ms`, the collector recomputes the board from its cached inputs with no `claude` spawn, and the new board is emitted because its bytes changed; every board the collector builds receives the previous board's scene states as `previous_scene`. Proof: host `crates/officina-host/tests/watch_escalation.rs::an_escalation_recomputes_without_spawning` (fake claude with a spawn counter, an injected clock past the threshold: one emission, zero spawns), `::no_escalation_no_recompute`, `::a_failed_snapshot_keeps_the_alert` (an alerted card, then the fake claude failing: the next board is stale, the card unknown and still alerted, `alert_count` unchanged).
8. Fixture intents (D-101 as amended): every scene scenario helper of `app/scripts/e2e-fixtures.mjs` carries the zone it means to produce, and the self-check compares each live card's `scene.zone` with it. Proof: CI step "e2e fixtures from the real CLI".
9. `officina board --json` prints the scene fields; it builds one board, so `previous_scene` is empty. Proof: CLI `crates/officina/tests/board.rs::board_prints_scene_states_and_the_alert_count` (a floor with a high-priority project, a fake permission prompt through `officina hook`).
10. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` under Changed: `board --json` carries each live session's zone and alert, the alert count and the next escalation time. Proof: review.

## Tasks

- [x] Read story 4.1's gate 4 entry; add its question shape to the vectors (AC 2): research notes E81, a `permission_prompt` notification seconds after the prompt
- [x] `crates/officina-core/src/board.rs`: fold `last_session_start_source` beside `saw_prompt`; `BoardInput.previous_scene` (AC 1, 2)
- [x] `crates/officina-core/src/scene.rs`: types, `place`, the unknown carry-over, next escalation; call from `build_board` after classification (AC 1-3, 5, 6)
- [x] `tests/vectors/scene.json` and `crates/officina-core/tests/scene_vectors.rs`; formatter ignore entries (AC 2-4, 6)
- [x] `crates/officina-host/src/watch.rs`: escalation wake on the tick, recompute without spawn, `previous_scene` from the last built board; `tests/watch_escalation.rs` (AC 7)
- [x] Regenerate the TypeScript bindings (AC 1)
- [x] Zone intents in `e2e-fixtures.mjs` (AC 8)
- [x] CLI test; `CHANGELOG.md` (AC 9, 10)

## Dev Notes

- Requirements: FR-2, FR-10, FR-13. Decisions: AD-5, AD-13, AD-14, AD-167 as amended, AD-168, AD-4 as amended. Epic decisions: D-462 to D-465, D-467, D-482, D-494 to D-496, D-499, D-514.
- Dependencies: story 4.2 (priority, threshold, schema bump); story 4.1's gate 4 recorded before this story starts (D-499). v0.2's `board.approvals` (story 2.8) for the approval rules.
- Core stays pure: time is `now_ms`, the threshold is `board.alert_after_ms`, the previous states are an input. Approvals are already in `BoardInput` (they build `board.approvals`); match them to cards by `session_id` with the same lookup that fills `ApprovalView.session_name`.
- The blocking notifications are `board.rs`'s `BLOCKING_NOTIFICATIONS`; read it, never retype it. `SessionEvents` already keeps `last_blocking_notification_at_ms`, `last_prompt_at_ms` and `last_stop_at_ms`; reception needs the latest `SessionStart`'s source and whether a `UserPromptSubmit` follows it, folded the same way (`saw_prompt` exists). The event line already records `source` (`events.rs`, `SESSION_START_SOURCES`); compare with the literal `startup` through one constant, not a retyped list.
- `board.stale` is the agents snapshot failing (AD-4); the cards are then the last known ones, which is exactly when a figure must stay put with a question mark and an alert must not vanish (D-494). `previous_scene` comes from the board the collector last built, whether emitted or not, so a recompute without a change still carries it.
- Approval-only waits (D-496): `officina run` blocks for 110 s by default, then the session moves on; the card's column being Working is the observable fact closest to the run still blocking. A request left pending after that stays in the header pill and on its board card.
- Agents samples in the vectors are built from the verified shapes of E1, never pasted from a real workstation (AD-14).
- The collector's recompute without spawn already exists for a quota change (AD-4); the escalation reuses it. `watch::publish` compares boards without `generated_at_ms`, so only a real change emits.

## Testing

- Core unit and vectors: `crates/officina-core/src/scene.rs`, `tests/scene_vectors.rs`.
- Host integration: `crates/officina-host/tests/watch_escalation.rs`.
- CLI integration: `crates/officina/tests/board.rs`.
- CI: bindings drift check, fixture self-checks.

## Dev Agent Record

### Debug Log

- Story order: gate 4 was to be recorded before this story started (D-499). It is not: the owner's buffer only started recording real use on 2026-10-28. The rule a gate 4 finding could change is rule 3's wait start, and the code already handles both possible shapes of a question: with a blocking notification (`elicitation_dialog`), the wait has a known start and escalates by time; without one (the agents `waiting` status only), the start is unknown and it never escalates by time (D-465). Both are vector cases. If gate 4 shows a shape neither covers, a case is added and, if needed, rule 3 changes before story 4.14 closes.
- `SceneState.approval` is true for an approval-only wait (a Working card with a valid pending request) only; a Needs you card with a request keeps Needs you's start and `approval = false`, so its ticket is numbered. Only valid requests count, as `waiting_count` counts them. An unknown card keeps the previous `alert` and `waiting_since_ms` and gets `approval = false`.
- `FRESH_START_SOURCE` (`startup`) is one constant of `events.rs`, asserted to be one of `SESSION_START_SOURCES`. `SessionEvents` keeps the latest `SessionStart`'s time and source; a prompt at or after it ends the arrival.
- The collector keeps the scene states of the board it built last in a `RefCell`, emitted or not, and passes them as `previous_scene`; `watch::publish` records the board's `next_escalation_at_ms`, and the loop rebuilds when the tick reaches it.
- AC 7's tests live in `crates/officina/tests/watch_escalation.rs`, not `crates/officina-host/tests/`: the fake claude is a binary of the `officina` crate, which only that crate's tests can locate (`CARGO_BIN_EXE_*`), and its log counts the spawns.
- `tests/vectors/scene.json` is JSON, which no formatter of the repository touches (rustfmt formats Rust, ESLint lints TypeScript and Svelte, no JSON formatter runs), so it needs no ignore entry; the expectations compare cards by key, so the board's column order does not matter to a case.
- `relay/mod.rs`'s check of `code()` against serde covers `Zone` and `AlertCause`, and reads `scene.rs`.
- Adversarial pass: a stale board on the first build, an unknown badge with a pending request, an invalid request, the earliest of two requests, a request on a Your turn card, a notification answered by a later prompt, an `idle_prompt`, a `SessionStart` without a source, a startup then a compact start, the floor's own threshold, one millisecond short and exactly at it, building one millisecond before and at the next escalation. All are cases above; no miss found that needed a code change.

### Completion Notes

- AC 1: `typescript_bindings.rs::committed_bindings_match_the_rust_types`; `scene.rs::tests::ended_cards_have_no_scene_state`.
- AC 2: `tests/vectors/scene.json` replayed by `scene_vectors.rs::every_case_places_its_cards` (cases named after each rule of the AC), the question shape gate 4 recorded included: "a question through AskUserQuestion carries a permission_prompt notification seconds after the prompt, a known start (gate 4, E81)". The case without a blocking notification stays as the fallback for a wait whose start was not seen.
- AC 3: vector cases "a stale board keeps an alerted card's alert", "an unknown badge keeps an alerted card's alert", "an unknown card past the threshold gains no alert", "fresh data in the lounge clears the alert", "a stale board makes every live card unknown, and a first board while stale has no alert"; `scene.rs::tests::alert_count_counts_unknown_cards_that_keep_their_alert`.
- AC 4: `scene_vectors.rs::every_zone_and_cause_has_a_case`; no formatter runs on JSON (Debug Log).
- AC 5: `scene.rs::tests::{board_order_is_unchanged_by_alerts, alert_count_counts_alerted_cards}`.
- AC 6: vector cases with `expect_next_escalation_at_ms`, one with an approval-only and an unknown card that do not set it; `scene_vectors.rs::building_at_the_next_escalation_alerts_the_card`.
- AC 7: `crates/officina/tests/watch_escalation.rs::{an_escalation_recomputes_without_spawning, no_escalation_no_recompute, a_failed_snapshot_keeps_the_alert}`.
- AC 8: CI step "e2e fixtures from the real CLI" (each scene session's zone against its intent).
- AC 9: `crates/officina/tests/board.rs::board_prints_scene_states_and_the_alert_count`.
- AC 10: `CHANGELOG.md` Changed.

## File List

- `crates/officina-core/src/scene.rs` (new), `board.rs`, `events.rs`, `lib.rs`, `relay/mod.rs`
- `crates/officina-core/tests/scene_vectors.rs` (new), `tests/vectors/scene.json` (new)
- `crates/officina-host/src/collector.rs`, `watch.rs`, `focus.rs`
- `crates/officina/tests/board.rs`, `crates/officina/tests/watch_escalation.rs` (new)
- `app/src/lib/generated/` (Board, Card, SceneState, Zone, AlertCause), `app/src/components/components.test.ts`, `app/src/lib/board.test.ts`
- `app/scripts/e2e-fixtures.mjs`, `CHANGELOG.md`

## Change Log

- 2026-10-28 created with the scene v2 rework (D-460)
- 2026-10-28 reworked after the plan challenge (D-494 to D-496, D-499): unknown keeps the last known alert through `previous_scene`; reception on a `startup` start only; approval-only waits while Working and never by time; gate 4 before this story starts
- 2026-10-28 implemented: the scene state in core with its vector set, the escalation recompute in the collector, the last known alert kept on a stale board, zone intents in the scene fixtures. Status in progress until gate 4's question shape is recorded
- 2026-10-28 gate 4 recorded (research notes E81): an `AskUserQuestion` question emits `permission_prompt`, a known start; its case joins the vectors, rule 3 unchanged. Status review

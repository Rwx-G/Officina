# Story 4.3: The scene state in core: zones, alerts and escalation

**Status:** Approved

## Story

As the author who must trust that a figure stands where its session really is,
I want core to decide, from the board's facts alone, which zone each live session belongs in and whether it is in alert, with the time escalation computed at the board's clock,
so that the scene never classifies, missing data shows as unknown, and an alert reaches the window within a tick of its threshold without spawning anything.

## Acceptance Criteria

1. Types (AD-167, AD-68 as amended): `crates/officina-core/src/scene.rs` defines `Zone` (`reception`, `office`, `decision`, `alert`, `lounge`, `unknown`) and `AlertCause` (`priority`, `waited`) with `closed_enum!` and `ALL`, and `SceneState { zone, waiting_since_ms: Option<i64>, alert: Option<AlertCause>, approval: bool }`; `Card.scene: Option<SceneState>` is `None` for To resume and Done cards and `Some` for every live card; `Board.alert_count: u32` and `Board.next_escalation_at_ms: Option<i64>` exist; TypeScript bindings regenerated under story 4.2's bump. Proof: `crates/officina-core/tests/typescript_bindings.rs::committed_bindings_match_the_rust_types`; `scene.rs::tests::ended_cards_have_no_scene_state`.
2. Rules (AD-167, D-462 to D-465, D-482), first match wins: a stale board gives `unknown`; the `unknown` badge gives `unknown`; a Needs you card, or a live card whose session has a pending approval, is waiting, with `waiting_since_ms` the latest blocking notification after the last prompt and the last stop (Needs you) or the earliest pending approval's `requested_at_ms` (approval only, `approval = true`), and is `alert` when its project's priority is high (`priority`) or when its known wait reached `alert_after_ms` (`waited`), else `decision`; a Your turn card with a `SessionStart` and no `UserPromptSubmit` since is `reception`; another Your turn card is `lounge`; a Working card is `office`. An `idle_prompt` notification changes nothing. Proof: `tests/vectors/scene.json` replayed by `crates/officina-core/tests/scene_vectors.rs::every_case_places_its_cards`, with cases at least for: each zone; reception only on evidence (no hook events gives lounge); an `idle_prompt` after a `Stop` stays lounge; a Needs you without a blocking notification has no known start and never escalates by time; a high-priority Needs you is in alert at once; a wait one millisecond short of the threshold and one exactly at it; an approval-only wait and its escalation; Needs you plus an approval keeps Needs you's start; a stale board makes every live card unknown; an unknown badge overrides waiting.
3. Coverage (AD-14's pattern): a test iterates `Zone::ALL` and `AlertCause::ALL` and fails when a value appears in no case's expectations; the vector file is exempt from every formatter (listed in the formatter ignore files). Proof: `scene_vectors.rs::every_zone_and_cause_has_a_case`; `cargo fmt --check` and `pnpm lint` leave the file untouched in CI.
4. Board order and counts (AD-167, D-465): `columns.needs_you` keeps its order, so digits keep acting on it and an alerted session keeps its number; `alert_count` counts cards with an alert; `waiting_count` is unchanged. Proof: `scene.rs::tests::{board_order_is_unchanged_by_alerts, alert_count_counts_alerted_cards}`.
5. Next escalation (AD-167, AD-168, D-467): `next_escalation_at_ms` is the earliest `waiting_since_ms + alert_after_ms` over waiting cards without an alert and with a known start, `None` otherwise; a board built at that instant has the card in alert and the next value moved on. Proof: `scene_vectors.rs` cases with `expect_next_escalation_at_ms`; `scene.rs::tests::building_at_the_next_escalation_alerts_the_card`.
6. The collector wakes for it (AD-168, D-467): when the 250 ms tick reaches the last board's `next_escalation_at_ms`, the collector recomputes the board from its cached inputs with no `claude` spawn, and the new board is emitted because its bytes changed. Proof: host `crates/officina-host/tests/watch_escalation.rs::an_escalation_recomputes_without_spawning` (fake claude with a spawn counter, an injected clock past the threshold: one emission, zero spawns), `::no_escalation_no_recompute`.
7. Fixture intents (D-101 as amended): every scene scenario helper of `app/scripts/e2e-fixtures.mjs` carries the zone it means to produce, and the self-check compares each live card's `scene.zone` with it. Proof: CI step "e2e fixtures from the real CLI".
8. `officina board --json` prints the scene fields. Proof: CLI `crates/officina/tests/board.rs::board_prints_scene_states_and_the_alert_count` (a floor with a high-priority project, a fake permission prompt through `officina hook`).
9. Documentation (D-121): `CHANGELOG.md` `[Unreleased]` under Changed: `board --json` carries each live session's zone and alert, the alert count and the next escalation time. Proof: review.

## Tasks

- [ ] `crates/officina-core/src/scene.rs`: types, `place`, next escalation; call from `build_board` after classification (AC 1, 2, 4, 5)
- [ ] `tests/vectors/scene.json` and `crates/officina-core/tests/scene_vectors.rs`; formatter ignore entries (AC 2, 3, 5)
- [ ] `crates/officina-host/src/watch.rs`: escalation wake on the tick, recompute without spawn; `tests/watch_escalation.rs` (AC 6)
- [ ] Regenerate the TypeScript bindings (AC 1)
- [ ] Zone intents in `e2e-fixtures.mjs` (AC 7)
- [ ] CLI test; `CHANGELOG.md` (AC 8, 9)

## Dev Notes

- Requirements: FR-2, FR-10, FR-13. Decisions: AD-5, AD-13, AD-14, AD-167, AD-168, AD-4 as amended. Epic decisions: D-462 to D-465, D-467, D-482.
- Dependencies: story 4.2 (priority, threshold, schema bump). v0.2's `board.approvals` (story 2.8) for the approval rules.
- Core stays pure: time is `now_ms`, the threshold is `board.alert_after_ms`. Approvals are already in `BoardInput` (they build `board.approvals`); match them to cards by `session_id` with the same lookup that fills `ApprovalView.session_name`.
- The blocking notifications are `board.rs`'s `BLOCKING_NOTIFICATIONS`; read it, never retype it. `SessionEvents` already keeps `last_blocking_notification_at_ms`, `last_prompt_at_ms` and `last_stop_at_ms`; reception needs a `saw_session_start` fact folded the same way (`saw_prompt` exists).
- `board.stale` is the agents snapshot failing (AD-4); the cards are then the last known ones, which is exactly when a figure must stay put with a question mark.
- Agents samples in the vectors are built from the verified shapes of E1, never pasted from a real workstation (AD-14).
- The collector's recompute without spawn already exists for a quota change (AD-4); the escalation reuses it. `watch::publish` compares boards without `generated_at_ms`, so only a real change emits.

## Testing

- Core unit and vectors: `crates/officina-core/src/scene.rs`, `tests/scene_vectors.rs`.
- Host integration: `crates/officina-host/tests/watch_escalation.rs`.
- CLI integration: `crates/officina/tests/board.rs`.
- CI: bindings drift check, fixture self-checks.

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-10-28 created with the scene v2 rework (D-460)

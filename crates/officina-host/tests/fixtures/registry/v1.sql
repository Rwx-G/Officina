-- Rows a version 1 registry holds, applied on top of migration 1 by
-- tests/registry_schema.rs: every table of schema 1 has at least one row,
-- `setting` one per key that schema allowed, with a text and an integer
-- value. Sample data only.

INSERT INTO event_file (name, ingested_bytes, complete_at_ms, corrupt_lines, rewrites)
VALUES ('2026-10-25.jsonl', 4096, 1793030000000, 1, 0),
       ('2026-10-26.jsonl', 2048, NULL, 0, 1);

INSERT INTO subject (id, project, name, status, status_src, status_at_ms, created_at_ms, routine, merged_into)
VALUES (1, 'api', 'rollout', 'active', 'author', 1793000000000, 1793000000000, 0, NULL),
       (2, 'api', 'old rollout', 'archived', 'author', 1793010000000, 1792990000000, 1, 1),
       (3, 'web', 'landing page', 'to_resume', 'rules', 1793020000000, 1793000500000, 0, NULL);

INSERT INTO session (session_id, project, first_seen_ms, last_seen_ms, subject_id, subject_src, subject_at_ms)
VALUES ('11111111-1111-4111-8111-111111111111', 'api', 1793000100000, 1793001000000, 1, 'rules', 1793000100000),
       ('22222222-2222-4222-8222-222222222222', 'web', 1793000600000, 1793002000000, 3, 'author', 1793000700000),
       ('33333333-3333-4333-8333-333333333333', 'web', 1793000800000, 1793000900000, NULL, NULL, NULL);

INSERT INTO session_fact (session_id, fact, value, src, at_ms)
VALUES ('11111111-1111-4111-8111-111111111111', 'kind', 'interactive', 'agents', 1793000100000),
       ('11111111-1111-4111-8111-111111111111', 'claude_pid', 4100, 'agents', 1793000100000),
       ('11111111-1111-4111-8111-111111111111', 'listed', 1793000100000, 'agents', 1793000100000),
       ('11111111-1111-4111-8111-111111111111', 'last_prompt_at', 1793000200000, 'hook', 1793000200000),
       ('22222222-2222-4222-8222-222222222222', 'kind', 'background', 'agents', 1793000600000),
       ('22222222-2222-4222-8222-222222222222', 'name', 'nightly', 'agents', 1793000600000),
       ('22222222-2222-4222-8222-222222222222', 'background_id', 'bg01', 'agents', 1793000600000),
       ('22222222-2222-4222-8222-222222222222', 'state', 'done', 'agents', 1793001900000),
       ('33333333-3333-4333-8333-333333333333', 'started', '{"source":"startup","claude_pid":4300}', 'hook', 1793000800000),
       ('33333333-3333-4333-8333-333333333333', 'ended', '{"reason":"other"}', 'hook', 1793000900000);

INSERT INTO session_excerpt (session_id, prompt, prompt_at_ms, answer_head, answer_tail, answer_at_ms)
VALUES ('11111111-1111-4111-8111-111111111111', 'add the health endpoint', 1793000200000, 'Added it.', 'Tests pass.', 1793000300000);

INSERT INTO triage_call (id, purpose, case_key, input_hash, started_at_ms, duration_ms, outcome, refusal, input_tokens, output_tokens, cost_microusd)
VALUES (1, 'proposal', 'subject:3', 'abc123', 1793020000000, 2100, 'answer', NULL, 900, 40, 1100),
       (2, 'dry_run', NULL, NULL, 1793021000000, NULL, 'refused', 'cmd_shim', NULL, NULL, NULL);

INSERT INTO confirmation (id, subject_id, session_id, proposed_verdict, proposed_source, proposed_reason, call_id, verdict, auto, confirmed_at_ms, reopened_at_ms)
VALUES (1, 2, NULL, 'finished', 'rules', 'empty_subject', NULL, 'finished', 0, 1793010000000, NULL),
       (2, 3, '22222222-2222-4222-8222-222222222222', 'to_resume', 'model', 'model', 1, 'to_resume', 0, 1793020000000, NULL);

INSERT INTO confirmation_input (confirmation_id, prompt, answer_head, answer_tail, end_reason, cut_mid_turn, expires_at_ms)
VALUES (2, 'draft the landing page', 'Drafted.', 'Left the footer.', NULL, 0, 1795000000000);

INSERT INTO model_answer (session_id, verdict, reason, call_id, at_ms)
VALUES ('22222222-2222-4222-8222-222222222222', 'to_resume', 'the footer is left', 1, 1793020000000);

INSERT INTO model_evaluation (id, at_ms, cases, model_correct, default_correct, model_false_finished, unanswered, input_tokens, output_tokens, cost_microusd)
VALUES (1, 1793015000000, 20, 17, 14, 1, 0, 18000, 800, 22000);

INSERT INTO setting (key, value, src, at_ms)
VALUES ('restore_notice', '{"copy":"registry-2026-10-24.db","restored_at_ms":1792900000000}', 'rules', 1792900000000),
       ('triage_model', '{"on":true}', 'author', 1793016000000),
       ('triage_model_pause', '{"paused":false}', 'author', 1793016000000),
       ('auto_archive', 7, 'author', 1793017000000);

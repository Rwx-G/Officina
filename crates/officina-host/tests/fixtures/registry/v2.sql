-- Rows a version 2 registry holds, applied on top of migrations 1 and 2
-- by tests/registry_schema.rs: every table of schema 2 has at least one
-- row, schema 1's as v1.sql holds them, then migration 2's tables with
-- work orders a host runner launched. Sample data only.

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

INSERT INTO work_order (id, seq, template, project, cwd, values_json, argv_json, prompt_sha256, catalogue_hash, name, subject_id, new_subject_name, source, proposal_id, launched_at_ms, outcome, exit_code, error, printed, bg_id, session_id, bound_at_ms)
VALUES ('0123456789abcdef0123456789abcdef', 1, 'bump', 'api', 'C:/path/to/api', '{"crate":"serde"}', '["-p","--bg"]', 'aa', 'bb', 'bump-1', 1, NULL, 'composer', NULL, 1793030000000, 'started', 0, NULL, 'started', 'bg1', '11111111-1111-4111-8111-111111111111', 1793030005000),
       ('fedcba9876543210fedcba9876543210', 2, 'bump', 'api', 'C:/path/to/api', '{"crate":"tokio"}', '["-p","--bg"]', 'cc', 'bb', 'bump-2', NULL, 'tokio bump', 'proposal', 'p1', 1793031000000, 'failed', 1, 'exit 1', NULL, NULL, NULL, NULL);

INSERT INTO proposal (id, kind, status, invalid_reason, received_at_ms, expires_at_ms, orchestrator_session_id, target_subject_id, target_session_id, template, project, values_json, subject_choice_json, texts_json, decided_at_ms, work_order_id)
VALUES ('p1', 'work_order', 'accepted', NULL, 1793030900000, 1793117300000, '22222222-2222-4222-8222-222222222222', NULL, NULL, 'bump', 'api', '{"crate":"tokio"}', '{"new":"tokio bump"}', '{"rationale":"outdated"}', 1793031000000, 'fedcba9876543210fedcba9876543210');

INSERT INTO orchestrator_start (id, started_at_ms, pid, pid_created_at_ms, outcome, error)
VALUES (1, 1793029000000, 4200, 1793028999000, 'spawned', NULL);

INSERT INTO auto_review (id, approval_id, request_key, project, action, requested_at_ms, mode, outcome, cause, decision, reason, rule_baseline, call_started_at_ms, duration_ms, input_tokens, output_tokens, cost_microusd, author_decision, author_reason, decided_at_ms, run_id, marked_at_ms)
VALUES (1, 'a1', 'k1', 'api', 'deploy', 1793032000000, 'shadow', 'answered', NULL, 'approve', 'looks fine', 1, 1793032000100, 900, 100, 20, 450, 'approved', NULL, 1793032100000, NULL, NULL);

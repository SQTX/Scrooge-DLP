-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0007 (Sub-faza 2B): zamień policies.content_compiled JSONB → BYTEA
-- żeby trzymać msgpack-encoded Policy (a nie JSON). msgpack jest format wire
-- protocol dla push'a do agenta przez gRPC Stream, i to samo trzymamy w DB
-- (uniknięcie re-encoding'u przy każdym push'cie).
--
-- Tabela `policies` w MVP jest pusta (Faza 2 to pierwsza wersja gdzie polityki
-- są tworzone), więc DROP COLUMN + ADD nie traci danych. Gdyby kiedyś trzeba
-- było to powtórzyć z istniejącymi danymi, idziemy z `USING decode(...)`.
-- =============================================================================

ALTER TABLE policies DROP COLUMN content_compiled;
ALTER TABLE policies ADD COLUMN content_compiled BYTEA NOT NULL DEFAULT '\x'::BYTEA;
ALTER TABLE policies ALTER COLUMN content_compiled DROP DEFAULT;

COMMENT ON COLUMN policies.content_compiled IS
  'MessagePack-encoded Policy (scrooge-policy::CompiledPolicy.msgpack). Wysyłany agentowi przez gRPC Stream — deser przez rmp_serde::from_slice po stronie agenta.';

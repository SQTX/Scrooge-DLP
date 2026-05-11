-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0004: polityki (DB = źródło prawdy, YAML jako import/export),
-- historia wersji, przypisania per agent.
-- =============================================================================

CREATE TABLE policies (
    name              TEXT PRIMARY KEY,
    version           INT NOT NULL,
    content_yaml      TEXT NOT NULL,
    -- Skompilowana forma (msgpack lub JSON) — gotowa do push'a do agenta.
    content_compiled  JSONB NOT NULL,
    -- SHA-256 hex content_compiled — do delta-push (agent zna swój hash).
    content_hash      TEXT NOT NULL,
    -- Selektory targetowania (os, tags, groups, hostnames). Format zgodny
    -- z sekcją `targets:` w YAML polityki.
    targets           JSONB NOT NULL,
    priority          INT NOT NULL DEFAULT 100,
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        UUID,
    updated_by        UUID
);

CREATE INDEX policies_enabled_idx  ON policies (enabled);
-- Partial index: ranking po priorytecie tylko dla aktywnych polityk.
CREATE INDEX policies_priority_idx ON policies (priority DESC) WHERE enabled = TRUE;

-- Auto-utrzymanie updated_at - korzysta z funkcji z migracji 0001.
CREATE TRIGGER policies_set_updated_at
    BEFORE UPDATE ON policies
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Pełna historia wersji - append-only. Każdy zapis polityki dorzuca tu rekord.
CREATE TABLE policy_history (
    id             BIGSERIAL PRIMARY KEY,
    policy_name    TEXT NOT NULL,
    version        INT NOT NULL,
    content_yaml   TEXT NOT NULL,
    changed_by     UUID,
    changed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    change_reason  TEXT
);

CREATE INDEX policy_history_policy_name_idx
    ON policy_history (policy_name, version DESC);

-- Przypisania: które polityki są wypchnięte do których agentów, oraz
-- na jakiej wersji jest agent (acked_version).
CREATE TABLE policy_assignments (
    agent_id          UUID NOT NULL REFERENCES agents(id)     ON DELETE CASCADE,
    policy_name       TEXT NOT NULL REFERENCES policies(name) ON DELETE CASCADE,
    assigned_version  INT NOT NULL,
    acked_version     INT,
    last_pushed_at    TIMESTAMPTZ,
    PRIMARY KEY (agent_id, policy_name)
);

-- Partial index: szybkie znalezienie zaległych pushy (do retry przez
-- response-orchestratora).
CREATE INDEX policy_assignments_pending_idx
    ON policy_assignments (policy_name)
    WHERE acked_version IS NULL OR acked_version < assigned_version;

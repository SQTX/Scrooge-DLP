-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0002: zarejestrowani agenci + tagi.
-- =============================================================================

CREATE TABLE agents (
    id               UUID PRIMARY KEY,
    hostname         TEXT NOT NULL,
    os               TEXT NOT NULL
                       CHECK (os IN ('windows', 'linux', 'macos')),
    os_version       TEXT,
    arch             TEXT,
    agent_version    TEXT,
    enrolled_at      TIMESTAMPTZ NOT NULL,
    last_seen        TIMESTAMPTZ,
    status           TEXT NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active', 'disconnected', 'disabled', 'pending')),
    cert_fingerprint TEXT NOT NULL UNIQUE
);

CREATE INDEX agents_status_idx     ON agents (status);
CREATE INDEX agents_hostname_idx   ON agents (hostname);
CREATE INDEX agents_last_seen_idx  ON agents (last_seen DESC);

-- Tagi: dowolne klucz/wartość per agent. Używane do targetowania polityk
-- (np. `department=finance`, `environment=production`).
CREATE TABLE agent_tags (
    agent_id  UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,
    PRIMARY KEY (agent_id, key)
);

CREATE INDEX agent_tags_key_value_idx ON agent_tags (key, value);

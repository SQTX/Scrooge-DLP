-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0003: grupy agentów (named sets do targetowania polityk).
-- =============================================================================

CREATE TABLE groups (
    name        TEXT PRIMARY KEY,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE agent_groups (
    agent_id    UUID NOT NULL REFERENCES agents(id)     ON DELETE CASCADE,
    group_name  TEXT NOT NULL REFERENCES groups(name)   ON DELETE CASCADE,
    PRIMARY KEY (agent_id, group_name)
);

CREATE INDEX agent_groups_group_name_idx ON agent_groups (group_name);

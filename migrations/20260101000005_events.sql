-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0005: eventy z agentów (partitioned by timestamp).
-- =============================================================================
--
-- # Partitioning
--
-- Tabela jest partycjonowana po `timestamp` (PostgreSQL `PARTITION BY RANGE`).
-- Wymagania PG: kolumny partition key MUSZĄ być częścią PRIMARY KEY,
-- stąd `PRIMARY KEY (id, timestamp)`.
--
-- Tworzymy jedną miesięczną partycję (2026-01) plus default catchall.
-- Auto-create kolejnych partition'ów to robota dla `pg_partman` albo
-- crona po stronie managera - poza zakresem MVP.
--
-- # Brak FK do agents(id)
--
-- Eventy NIE mają FK do `agents.id` (świadomie). Pozwala to zachować
-- historię eventów z agentów które zostały usunięte (forensics, audyt).

CREATE TABLE events (
    id                   UUID NOT NULL,
    timestamp            TIMESTAMPTZ NOT NULL,
    agent_id             UUID NOT NULL,
    event_type           TEXT NOT NULL,
    severity             TEXT NOT NULL
                           CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    direction            TEXT
                           CHECK (direction IN ('inbound', 'outbound', 'internal')),
    matched_policy       TEXT,
    matched_rule         TEXT,
    action_taken         TEXT,
    details              JSONB NOT NULL,
    forwarded_to_syslog  BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

-- Początkowa miesięczna partycja (rozwoju MVP startuje w 2026-01).
CREATE TABLE events_2026_01 PARTITION OF events
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');

-- Catchall partycja - łapie wszystko poza zdefiniowanymi zakresami.
-- Powinna zostać "opróżniona" przez przesunięcie danych do dedykowanych
-- partition'ów po ich utworzeniu (operational concern, nie MVP).
CREATE TABLE events_default PARTITION OF events DEFAULT;

-- Indeksy są propagowane z parent do partition'ów automatycznie.
CREATE INDEX events_agent_id_idx    ON events (agent_id, timestamp DESC);
CREATE INDEX events_event_type_idx  ON events (event_type);
CREATE INDEX events_severity_idx    ON events (severity);
CREATE INDEX events_timestamp_idx   ON events (timestamp DESC);

-- Partial index dla syslog-forwarder workera: szuka tylko niewypchniętych.
CREATE INDEX events_unforwarded_idx ON events (timestamp DESC)
    WHERE forwarded_to_syslog = FALSE;

-- GIN dla queries po polach JSONB.details (np. po path, classifier_id).
CREATE INDEX events_details_gin_idx ON events USING gin (details);

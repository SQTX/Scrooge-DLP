-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- Migration 0006: użytkownicy dashboardu (JWT auth), refresh tokens,
-- enrollment tokens dla agentów.
-- =============================================================================

CREATE TABLE users (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username       TEXT NOT NULL UNIQUE,
    password_hash  TEXT NOT NULL,
    role           TEXT NOT NULL
                     CHECK (role IN ('admin', 'analyst', 'viewer')),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login     TIMESTAMPTZ,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX users_enabled_idx ON users (enabled);

-- Refresh tokens: nigdy nie zapisujemy raw - tylko SHA-256 hash. Klient
-- dostaje raw token, manager weryfikuje przez `digest(raw, 'sha256')`.
CREATE TABLE refresh_tokens (
    token_hash  TEXT PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked     BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX refresh_tokens_user_id_idx ON refresh_tokens (user_id);
-- Partial: szybki cleanup wygasłych aktywnych tokenów.
CREATE INDEX refresh_tokens_expires_at_idx ON refresh_tokens (expires_at)
    WHERE revoked = FALSE;

-- Enrollment tokens: użytkowane przez agentów przy pierwszym łączeniu.
-- Też przechowywany hash, nie raw.
CREATE TABLE enrollment_tokens (
    token_hash   TEXT PRIMARY KEY,
    description  TEXT,
    expires_at   TIMESTAMPTZ,
    max_uses     INT,
    uses_count   INT NOT NULL DEFAULT 0,
    created_by   UUID,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partial: scanning po nie-wygasłych tokenach.
CREATE INDEX enrollment_tokens_expires_at_idx ON enrollment_tokens (expires_at)
    WHERE expires_at IS NOT NULL;

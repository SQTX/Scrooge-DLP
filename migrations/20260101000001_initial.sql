-- SPDX-License-Identifier: GPL-2.0-only
--
-- ScroogeDLP - Data Loss Prevention system
-- Copyright (C) 2026 SQTX <sssqtx@gmail.com>
--
-- This program is free software; you can redistribute it and/or modify
-- it under the terms of the GNU General Public License version 2 as
-- published by the Free Software Foundation.
--
-- Migration 0001: extensions + helper triggers.
-- =============================================================================

-- `gen_random_uuid()` jest w PostgreSQL 13+ core, ale zostawiamy `pgcrypto`
-- dla zgodności ze starszymi instancjami i dla `digest()`, `crypt()`.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Reusable trigger function: ustawia `updated_at = NOW()` przy każdym UPDATE.
-- Używana przez tabele które chcą auto-utrzymywanego `updated_at` (`policies`).
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

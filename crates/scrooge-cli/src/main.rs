// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scroogectl` — CLI klient managera ScroogeDLP.
//!
//! Krok 6 — minimalny zestaw subcommandów do bootstrap'a dev environment'u:
//! - `bootstrap-admin` — tworzy admin usera w DB (idempotentne — `ON CONFLICT
//!   DO NOTHING`).
//! - `gen-token` — generuje enrollment token (raw na stdout, hash w DB).
//!
//! Oba subcommandy łączą się BEZPOŚREDNIO do PostgreSQL (nie przez REST API).
//! W kolejnych iteracjach dochodzi REST client mode (`scroogectl agents list`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use scrooge_common::{config::ManagerConfig, crypto};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "scroogectl", version, about = "ScroogeDLP CLI client")]
struct Args {
    /// Ścieżka do `manager.yaml` (potrzebne dla `database.url`).
    #[arg(
        short,
        long,
        env = "MANAGER_CONFIG",
        default_value = "/etc/scrooge/manager.yaml"
    )]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Aplikuje migracje DB. Idempotentne — `sqlx migrate` skipuje już
    /// zaaplikowane wersje.
    Migrate,

    /// Bootstrap admin user (idempotentnie).
    BootstrapAdmin {
        #[arg(long, default_value = "admin")]
        username: String,
        /// Hasło (lub przez env `ADMIN_PASSWORD`).
        #[arg(long, env = "ADMIN_PASSWORD")]
        password: String,
    },

    /// Generuje enrollment token. Raw token wypisuje na stdout (do skopiowania),
    /// SHA-256 hash zapisuje w DB.
    GenToken {
        /// Opis tokenu.
        #[arg(long)]
        description: Option<String>,
        /// Max ile razy token może zostać użyty (puste = unlimited).
        #[arg(long)]
        max_uses: Option<i32>,
        /// Ważność w dniach.
        #[arg(long, default_value = "30")]
        valid_days: i64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let config = ManagerConfig::load(&args.config).context("loading manager config")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database.url)
        .await
        .context("connecting to PostgreSQL")?;

    match args.command {
        Command::Migrate => {
            migrate(&pool).await?;
        },
        Command::BootstrapAdmin { username, password } => {
            bootstrap_admin(&pool, &username, &password).await?;
        },
        Command::GenToken {
            description,
            max_uses,
            valid_days,
        } => {
            gen_token(&pool, description, max_uses, valid_days).await?;
        },
    }
    Ok(())
}

async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .context("applying migrations")?;
    eprintln!("migrations applied");
    Ok(())
}

async fn bootstrap_admin(pool: &PgPool, username: &str, password: &str) -> Result<()> {
    if password.len() < 8 {
        anyhow::bail!("password must be at least 8 characters");
    }
    let hash = crypto::hash_password(password).context("hashing password")?;
    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, role) \
         VALUES ($1, $2, 'admin') \
         ON CONFLICT (username) DO NOTHING",
    )
    .bind(username)
    .bind(&hash)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        eprintln!("user '{username}' already exists — not modified");
    } else {
        eprintln!("admin user '{username}' created");
    }
    Ok(())
}

async fn gen_token(
    pool: &PgPool,
    description: Option<String>,
    max_uses: Option<i32>,
    valid_days: i64,
) -> Result<()> {
    let raw = Uuid::new_v4().to_string();
    let hash = sha256_hex(&raw);
    let expires_at = Utc::now() + chrono::Duration::days(valid_days);

    sqlx::query(
        "INSERT INTO enrollment_tokens (token_hash, description, expires_at, max_uses) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&hash)
    .bind(description)
    .bind(expires_at)
    .bind(max_uses)
    .execute(pool)
    .await?;

    // Raw na stdout - pipe-friendly. Hash + metadata na stderr.
    println!("{raw}");
    eprintln!(
        "enrollment token created (expires in {valid_days} days). \
         Save the token shown above — only its SHA-256 hash is stored in DB."
    );
    Ok(())
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

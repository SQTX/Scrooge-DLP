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
//! Subcommandy:
//! - `init-ca` — generuje root CA + manager server cert (bez DB / configu;
//!   używane przy quickstart przed `docker compose up`).
//! - `migrate` — aplikuje migracje DB (idempotent).
//! - `bootstrap-admin` — tworzy admin user (idempotent przez `ON CONFLICT`).
//! - `gen-token` — generuje enrollment token (raw na stdout, hash w DB).
//!
//! `init-ca` jest jedynym subcommandem który NIE wymaga `manager.yaml` ani
//! połączenia z PG — pozostałe ładują config + pool per-subcommand.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use scrooge_common::{
    ca::{generate_server_csr, RootCa},
    config::ManagerConfig,
    crypto,
};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "scroogectl", version, about = "ScroogeDLP CLI client")]
struct Args {
    /// Ścieżka do `manager.yaml` (wymagane dla wszystkich subcommandów oprócz
    /// `init-ca`).
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
    /// Generuje root CA + server cert dla managera. Bez DB, bez configu.
    InitCa {
        /// Katalog docelowy. Tworzony jeśli nie istnieje.
        #[arg(long, default_value = "./certs")]
        output: PathBuf,

        /// Common Name dla CA.
        #[arg(long, default_value = "ScroogeDLP Root CA")]
        ca_cn: String,

        /// Common Name dla server cert managera.
        #[arg(long, default_value = "scrooge-manager")]
        server_cn: String,

        /// Subject Alternative Names (comma-separated). DNS-like → SAN.DnsName,
        /// parsowalne IP → SAN.IpAddress.
        #[arg(
            long,
            default_value = "localhost,scrooge-manager,127.0.0.1",
            value_delimiter = ','
        )]
        san: Vec<String>,

        /// Nadpisuje istniejące pliki (default: skip jeśli istnieją).
        #[arg(long)]
        force: bool,
    },

    /// Aplikuje migracje DB. Idempotentne.
    Migrate,

    /// Bootstrap admin user (idempotentnie).
    BootstrapAdmin {
        #[arg(long, default_value = "admin")]
        username: String,
        /// Hasło (lub przez env `ADMIN_PASSWORD`).
        #[arg(long, env = "ADMIN_PASSWORD")]
        password: String,
    },

    /// Generuje enrollment token. Raw token na stdout, hash SHA-256 w DB.
    GenToken {
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        max_uses: Option<i32>,
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

    match args.command {
        Command::InitCa {
            output,
            ca_cn,
            server_cn,
            san,
            force,
        } => {
            init_ca(&output, &ca_cn, &server_cn, &san, force)?;
        },
        Command::Migrate => {
            let pool = open_pool(&args.config).await?;
            migrate(&pool).await?;
        },
        Command::BootstrapAdmin { username, password } => {
            let pool = open_pool(&args.config).await?;
            bootstrap_admin(&pool, &username, &password).await?;
        },
        Command::GenToken {
            description,
            max_uses,
            valid_days,
        } => {
            let pool = open_pool(&args.config).await?;
            gen_token(&pool, description, max_uses, valid_days).await?;
        },
    }
    Ok(())
}

async fn open_pool(config_path: &Path) -> Result<PgPool> {
    let config = ManagerConfig::load(config_path).context("loading manager config")?;
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database.url)
        .await
        .context("connecting to PostgreSQL")
}

// ──────────────────────────────────────────────────────────────────────────
// init-ca
// ──────────────────────────────────────────────────────────────────────────

fn init_ca(output: &Path, ca_cn: &str, server_cn: &str, san: &[String], force: bool) -> Result<()> {
    std::fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;

    let ca_pem_path = output.join("ca.pem");
    let ca_key_path = output.join("ca.key");
    let server_pem_path = output.join("server.pem");
    let server_key_path = output.join("server.key");

    let any_exists = ca_pem_path.exists()
        || ca_key_path.exists()
        || server_pem_path.exists()
        || server_key_path.exists();

    if any_exists && !force {
        eprintln!(
            "files already exist in {} — skipping (use --force to regenerate)",
            output.display()
        );
        return Ok(());
    }

    eprintln!("▸ generating root CA ({ca_cn})…");
    let ca = RootCa::generate(ca_cn).context("generating root CA")?;
    ca.save_to_files(&ca_pem_path, &ca_key_path)
        .context("saving CA files")?;

    eprintln!("▸ generating manager server cert (CN={server_cn}, SAN={san:?})…");
    let (csr_pem, server_key_pem) =
        generate_server_csr(server_cn, san).context("generating server CSR")?;
    let server_cert_pem = ca.sign_csr(&csr_pem).context("signing server CSR")?;

    std::fs::write(&server_pem_path, &server_cert_pem)
        .with_context(|| format!("writing {}", server_pem_path.display()))?;
    std::fs::write(&server_key_path, &server_key_pem)
        .with_context(|| format!("writing {}", server_key_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ca_key_path, std::fs::Permissions::from_mode(0o600))?;
        std::fs::set_permissions(&server_key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    eprintln!();
    eprintln!("✔ CA + server cert wygenerowane w {}:", output.display());
    eprintln!("  - ca.pem      (root CA cert — distribute do agentów)");
    eprintln!("  - ca.key      (root CA private key — TRZYMAĆ W SEKRECIE)");
    eprintln!("  - server.pem  (manager TLS cert)");
    eprintln!("  - server.key  (manager TLS private key)");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// migrate
// ──────────────────────────────────────────────────────────────────────────

async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .context("applying migrations")?;
    eprintln!("migrations applied");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// bootstrap-admin
// ──────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────
// gen-token
// ──────────────────────────────────────────────────────────────────────────

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

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

Greenfield. No source code exists yet — this file documents the intended design so subsequent work stays consistent. Update this file when architectural decisions change.

## Purpose

Self-hosted MongoDB backup and restore tool. Wraps the `mongodump` / `mongorestore` binaries behind a small web UI so an operator can manage multiple MongoDB connections, schedule recurring backups per database, prune old backups by retention count, and restore from either a server-side file or an uploaded archive.

## Stack

- **Backend:** Rust + Actix Web. The server shells out to `mongodump` and `mongorestore` — these binaries are a runtime dependency and must be on `PATH`.
- **Frontend:** Static HTML + Tailwind CSS + shadcn-style components. Served by Actix as static assets; no separate frontend build server.
- **Persistence:** Plain JSON files on local disk — no database. The tool itself does not depend on MongoDB to run.
- **Config:** `.env` only holds the HTTP port. Everything else (users, connections, schedules, retention) lives in the JSON files.

## Data layout

Three independent JSON files, each owning one concern. Keep them separate — do not consolidate into one config file.

- **Users / credentials** — login accounts for the UI. Passwords must be hashed (argon2 or bcrypt), never stored plaintext. **First-run bootstrap:** if the users file is missing or empty, the login page must redirect to a one-time "create admin" form instead of prompting for credentials. After the first admin exists, that form is no longer reachable.
- **Connections** — list of MongoDB connection strings. Each connection has an id, a label, the URI, and the set of databases the user has chosen to back up under it. One connection → many databases.
- **Settings / schedules** — per-database backup policy: interval and retention count (how many archives to keep). Pruning is by count, not age.

Connection strings contain credentials. Treat the connections file as sensitive: restrict file permissions and never log its contents.

## Backup / restore semantics

- **Backup format:** `mongodump --archive=<file> --gzip` — single compressed archive per backup. Do not produce a directory dump; the tool standardizes on the archive form so restores are a single file.
- **Retention:** after a successful backup, delete the oldest archives for that database until the count matches the configured retention. Never delete on failure.
- **Scheduling:** the Actix process owns the scheduler (e.g. a tokio task per enabled schedule). There is no external cron. Schedules must survive restarts by being re-read from the settings file at startup.
- **Restore inputs:** the user picks an existing archive on the server *or* uploads one. Target is either a saved connection or an ad-hoc connection string entered in the form. Restore uses `mongorestore --archive=<file> --gzip`.

## UI flow

Single-page-ish: login → connections list → per-connection database list with backup toggle, interval, retention → backups list (download / restore / delete) → restore page (pick server file or upload, pick target connection).

## Conventions for future work

- Do not introduce a database engine, ORM, or migrations system. JSON files are the design, not a placeholder.
- Do not split the binary into multiple services. One Actix process serves the API, the static UI, and runs the scheduler.
- `mongodump` / `mongorestore` invocations must stream stdout/stderr to a per-job log so failures are debuggable; do not silently discard output.
- Never interpolate user-supplied strings into a shell — invoke the binaries with argv arrays only. Connection strings and file paths are the obvious injection vectors.

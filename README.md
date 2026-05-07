# mongodb-utils

A small self-hosted web app for backing up and restoring MongoDB databases.

Point it at one or more MongoDB connections, pick the databases you care about,
and it takes care of recurring backups, retention, and one-click restores.
Everything runs on your own server — no third-party service, no cloud account.

## What it does

- **Multiple connections.** Save any number of MongoDB connection strings
  (self-hosted, Atlas, replica sets) and switch between them from the UI.
- **Per-database scheduling.** For each database on each connection, choose
  how often to back up and how many archives to keep. Older archives are
  pruned automatically.
- **One-click manual backups.** "Backup now" runs on demand and shows live
  per-collection progress so you can see exactly where it is.
- **Compressed archives.** Backups are written as single gzipped archives
  for fast transfer and minimal disk use.
- **Restore from anywhere.** Restore from a backup that's already on the
  server, or upload an archive from your machine. The target can be an
  existing connection or a one-off connection string.
- **Built-in admin.** First visit walks you through creating an admin user;
  subsequent visits go through a normal login.
- **No database to host.** State lives in plain JSON files on disk — no
  Postgres or Redis to run alongside.

# download and setup deb file mongodb database tools from

https://mongodb.com/try/download/bi-connector

## Status

Early but functional. Expect rough edges; issues and PRs welcome.

## License

Released under the [MIT License](LICENSE).

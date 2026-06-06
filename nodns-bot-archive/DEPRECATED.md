# DEPRECATED — Go Bot Archived

This directory contains the **original Go implementation** of the NoDNS bot (`nodns-bot`).

## Status: Superseded by Rust

The Go bot has been fully ported to Rust (`../nodns-bot-rs/`). Feature parity confirmed:

- Parser, auth, DDNS/TSIG, subscriber, store, HTTP API, Cashu payment — all 1:1 ported
- Rust adds: multi-zone `[[dns.zones]]` config, `HumaneDuration` parser, broader unit test coverage
- The Rust bot is **live in production** on the VPS (systemd `nodns-bot.service`)

## Why Archived, Not Deleted

- The Go binary (`nodns-bot` and `nodns-bot-linux`) are retained on the VPS as rollback artifacts
- Schema compatibility was verified during the Go→Rust migration (identical SQLite tables)
- Keeping the source available for reference during the transition period

## When to Remove Entirely

Once the Rust bot has been running production for a sustained period with zero rollbacks needed, this directory and the VPS Go artifacts can be safely deleted.

## Key Files (for reference only)

| File | Purpose |
|---|---|
| `main.go` | Entrypoint, HTTP API, event loop |
| `internal/nostr/parser.go` | Kind 11111 event parsing |
| `internal/nostr/subscriber.go` | Relay subscription + reconnect |
| `internal/auth/authority.go` | Authority/delegation/registrar checks |
| `internal/payment/cashu.go` | Cashu token verification |
| `internal/payment/payment.go` | Payment gating logic |
| `internal/dns/updater.go` | DDNS/TSIG updater |
| `internal/store/sqlite.go` | SQLite schema + queries |
| `internal/config/config.go` | Config schema + defaults |
| `config.example.toml` | Config template |

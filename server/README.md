# Kavka Team Server

Phase 6 (see docs/ROADMAP.md). Self-hosted control plane: SSO, RBAC policy
distribution, profile sync, central audit, alert routing, and a read-mostly web
console. It is never a Kafka wire proxy — desktop clients talk to Kafka
directly (docs/ARCHITECTURE.md D7).

Planned stack: Rust (axum) + PostgreSQL, distributed via Docker/Helm.
This directory is intentionally empty until Phase 6 begins.

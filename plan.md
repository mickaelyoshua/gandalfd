# Plan: Gandalf DNS Blocker Implementation

**Context** — Home network lacks central ad/tracker/porn blocking. Need custom DNS-level protection.
**Objective** — Rust-based DNS server running on Raspberry Pi that filters queries against a blocklist and forwards clean queries.
**Approach** — Rust + Tokio + `hickory-dns`. Phase 1: `ahash::HashSet` for high-performance exact domain matching. Phase 2 (Future): `Trie` for wildcard subdomain matching.

## Status Atual
- **Core (Feito)**: Domínio (`domain.rs`), estrutura da Blocklist (`blocklist.rs`) e Fetcher (`fetcher.rs`) estão prontos, com error handling fortemente tipado (`thiserror`) em vez de panics/erros opacos.
- **Pendente**: Implementação do Handler DNS (`hickory-server`) e orquestração no `main.rs`.

## Steps

1. **~~Init Project~~** (FEITO)
   - Files: `Cargo.toml`
   - Changes: Adicionado `tokio`, `hickory-server`, `reqwest`, `ahash`, `thiserror`.
2. **~~Core Domain~~** (FEITO)
   - Files: `src/domain.rs`
   - Changes: `DomainName` newtype com `DomainError`. `Blocklist` trait.
3. **~~Blocklist Implementation~~** (FEITO)
   - Files: `src/blocklist.rs`
   - Changes: Impl de `Blocklist` com `ahash::HashSet` e ignore tolerante a falhas.
4. **~~Fetcher~~** (FEITO)
   - Files: `src/fetcher.rs`
   - Changes: Download assíncrono com atomic write (resiliência Raspberry Pi) e `FetchError`.
5. **~~Add Observability~~** (FEITO)
   - Files: `Cargo.toml`, `src/lib.rs`, `src/observability.rs`
   - Changes: Adicionado `tracing` e `tracing-subscriber` com stdout/env_filter.
5.5 **~~Add Configuration~~** (FEITO)
   - Files: `Cargo.toml`, `src/settings.rs`, `config.toml`
   - Changes: Adicionado `config`, `toml`, `serde` para configuração Híbrida.
6. **~~DNS Handler~~** (FEITO)
   - Files: `src/handler.rs`
   - Changes: Implementado `RequestHandler`. Delega para `TokioResolver` ou bloqueia com `NXDomain`.
7. **~~Main Entry & App Abstraction~~** (FEITO)
   - Files: `src/main.rs`, `src/app.rs`
   - Changes: Orquestração movida para `GandalfApp`. Inicia `tracing`, carrega as configurações, e encapsula o bind UDP/TCP e o shutdown gracefully (Ctrl+C).
7.2. **~~Multi-URL Blocklist Loading~~** (FEITO)
   - Files: `src/blocklist.rs`
   - Changes: `Blocklist::load_from_urls` implementado para baixar múltiplas listas de forma tolerante a falhas (graceful degradation) e sequencial.
7.5. **~~High Availability Upstream DNS~~** (FEITO)
   - Files: `src/settings.rs`, `src/app.rs`
   - Changes: Mudar `upstream_dns` de `String` para `Vec<String>`. Iterar no `app.rs` criando múltiplos `NameServerConfig` no `ResolverConfig` para failover automático do Hickory.
8. **Phase 2: Trie Implementation** (FUTURE)
   - Files: `src/blocklist/trie.rs`, `src/blocklist.rs`
   - Changes: Suffix Trie para bloqueio wildcard (`*.ads.com`). Transição fluida permitindo subdomínios.
9. **Phase 3: Background Updater** (FUTURE)
   - Files: `Cargo.toml`, `src/main.rs`, `src/handler.rs`
   - Changes: Adicionar dependência `arc-swap` para concorrência read-heavy lock-free. Thread paralela com `tokio::spawn` dormindo em loop e rebaixando listas. Atualização atômica do cache local de domínios sem derrubar o DNS.

## Performance Impact
- Expected memory: ~30-50MB para 300k domínios. Acceptable para Pi.
- Latency: < 1ms para local cache hit/block.

## Risks & Open Questions
- Privilege needed to bind port 53. Mitigação: Usar `setcap` no binário, ou rodar teste dev na porta 5353.
- Memory fragmentation se listas atualizarem dinamicamente. Mitigação: Usar `Arc::swap` na blocklist completa, evitando mutação in-place fragmentada.

## Rollback / TDD Strategy
- Escrita iterativa: Handler mockado primeiro (teste isolado respondendo à query sem resolver).
- Teste de integração rodando queries DNS reais (`dig @127.0.0.1 -p 5353 google.com`) na máquina local.

## Out of Scope
- Web UI / Dashboard.
- DoH/DoT (plaintext apenas por enquanto).

# AI router

Начните с [правил](AGENTS.md).

- Liquidity Vault (недели 8–13): [scope](scopes/vault.md), отдельный workspace [liquidity-vault](../../liquidity-vault/README.md) с программой и Rust-клиентом.
- Платежи (задание 14): [scope](scopes/payments.md), [payments-client](../../clients/payments-client/README.md). Локальный MVP с HTTP API, JSON store и RPC worker. [Спецификация задания 13](../stablecoin-payments.md) описывает целевое развитие, включая PostgreSQL.
- Зависимости и сборка: [Cargo.toml](../../Cargo.toml), [Cargo.lock](../../Cargo.lock), manifests затронутого клиента.
- Dashboard: [scope](scopes/dashboard.md), [dashboard-client](../../clients/dashboard-client/) и его web/.
- CPI: [scope](scopes/composable.md), [composable-client](../../clients/composable-client/) и инструкция compose_transfer существующей токен-программы.
- DeFi: [defi-client](../../clients/defi-client/), интеграция Orca в devnet и адреса пула в README.
- Токен и metadata: [token-client](../../clients/token-client/) и его metadata.json.
- Реализация и запуск: [README](../../README.md), существующие клиенты в clients/.
- Проверки: Cargo format, Clippy и tests затронутых клиентов, затем реальный RPC-запуск для изменённых операций.

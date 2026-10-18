# CPI composition

- Расширяйте существующую токен-программу в `programs/solana-level-1-token-starter/`; не создавайте вторую программу для стандартных token CPI.
- `compose_transfer` объединяет ATA create_idempotent, SOL transfer и transfer_checked. Сохраняйте атомарность, проверку подписи, mint/authority/program и канонического ATA.
- `clients/composable-client/` демонстрирует localnet с кошельками в памяти. Он не обновляет существующий devnet mint и не принимает публичный RPC для записи.
- После изменения программы сначала пересоберите SBF, затем выполните LiteSVM tests. Откат проверяйте чтением аккаунтов после отклонённой транзакции, отдельно от simulation клиента.
- Команды и границы поддержки Token-2022 extensions находятся в корневом README. RPC readback localnet не является подтверждением выполнения в devnet.

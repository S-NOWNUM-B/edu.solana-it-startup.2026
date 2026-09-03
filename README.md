# Solana Level 1 Token Starter

Учебная Anchor-программа Superteam KZ: создание mint и ATA, выпуск, перевод и сжигание токенов. Программа работает через `anchor_spl::token_interface` с Token Program и Token-2022. Rust-тесты выполняются локально в LiteSVM без RPC, validator, SOL и файла кошелька.

## Сдача задания 2

- Репозиторий: [S-NOWNUM-B/edu.solana-it-startup.2026](https://github.com/S-NOWNUM-B/edu.solana-it-startup.2026).
- Ветка: [`task/02-burn`](https://github.com/S-NOWNUM-B/edu.solana-it-startup.2026/tree/task/02-burn).
- Основа: `task/01-tests`, commit `e982692` — тесты первого задания сохранены.
- Добавлена инструкция `burn_tokens(amount: u64)`, строгие account constraints и 18 новых интеграционных тестов. Escrow в эту работу не входит.

## Зафиксированный стек

| Инструмент | Версия | Где зафиксирована |
| --- | --- | --- |
| Anchor CLI и crates | `1.1.2` | `Anchor.toml`, точные зависимости `=1.1.2` |
| Solana CLI | `3.1.10` | `Anchor.toml` |
| Rust | `1.89.0` | `rust-toolchain.toml` |
| LiteSVM | `0.10.0` | dev-dependency `=0.10.0` |

Новых зависимостей нет; `Cargo.lock` сохранён. Для нового TypeScript-клиента предполагается `@solana/kit`; JavaScript-клиент и `@solana/web3.js` в задании не используются.

## Сборка и тестирование с чистого checkout

Предварительно установите указанные выше версии Rust, Solana CLI и Anchor CLI и добавьте их в `PATH`. Команды ниже выполняются в **macOS zsh/bash** или **Windows WSL/Ubuntu bash**:

```bash
git clone --branch task/02-burn https://github.com/S-NOWNUM-B/edu.solana-it-startup.2026.git
cd edu.solana-it-startup.2026
rustc --version
solana --version
anchor --version
anchor build
cargo test
```

`anchor build` создаёт `target/deploy/solana_level_1_token_starter.so` и IDL. `cargo test` загружает этот `.so`, поэтому сборка должна предшествовать тестам, в том числе после изменения кода программы. Ожидаемый результат: **54 интеграционных теста и unit-тест `test_id`**, без падений.

Дополнительные проверки (**macOS zsh/bash; Windows WSL/Ubuntu bash**):

```bash
cargo test --workspace --locked
cargo fmt --all -- --check
```

### Program ID и локальные ключи

Program ID в `declare_id!` и `Anchor.toml` — публичный адрес, не секрет. Program keypair не хранится в Git. Anchor создаёт локальный keypair в игнорируемом `target/deploy/`; в Anchor CLI `1.1.2` несовпадение его адреса с `declare_id!` выводит предупреждение, но не останавливает сборку. Это поведение видно в [исходнике CLI версии 1.1.2](https://github.com/otter-sec/anchor/blob/v1.1.2/cli/src/lib.rs#L2100).

Для локальных LiteSVM-тестов это безопасно: тест загружает `.so` под объявленным `ID`, не использует локальный program keypair и не выполняет деплой. При желании предупреждение можно убрать командой `anchor build --ignore-keys` (**macOS zsh/bash; Windows WSL/Ubuntu bash**); для прохождения задания флаг не требуется.

Для собственного devnet-деплоя сначала создайте локальные ключи и выполните `anchor keys sync`, затем пересоберите программу. Эта команда меняет публичные ID в исходниках и конфигурации, поэтому для обычных тестов она не нужна. Никогда не публикуйте keypair, seed phrase или приватные ключи.

## Архитектура и `burn_tokens`

`src/lib.rs` объявляет пять инструкций; каждый модуль в `src/instructions/` содержит структуру аккаунтов и обработчик. Новый `burn_tokens.rs` использует CPI `token_interface::burn_checked`, передавая `amount` и **decimals из проверенного mint**, а не от клиента. Сумма задаётся в минимальных единицах токена: например, `1_000_000` при decimals `6` — один токен.

### Account constraints

| Аккаунт | Тип и проверки | Назначение |
| --- | --- | --- |
| `authority` | `Signer<'info>` | Требует подпись владельца `source` |
| `mint` | `InterfaceAccount<Mint>`, `mut`, `mint::token_program = token_program` | Проверяет тип mint и программу-владельца; разрешает уменьшить supply |
| `source` | `InterfaceAccount<TokenAccount>`, `mut`, `token::mint = mint`, `token::authority = authority`, `token::token_program = token_program` | Проверяет тип, связь с mint, владельца токенов и программу-владельца; разрешает уменьшить баланс |
| `token_program` | `Interface<TokenInterface>` | Принимает только исполняемую Token Program или Token-2022 |

`InterfaceAccount` допускает обе SPL-программы, поэтому одного типа недостаточно: constraints дополнительно требуют, чтобы **mint и source принадлежали именно переданной token program**. Владение аккаунтом программой и право authority распоряжаться токенами — разные проверки.

Перед CPI обработчик проверяет `amount > 0` через `require!`. Недостаточный баланс отклоняет SPL-программа внутри `burn_checked`. Критичные аккаунты новой инструкции не используют `UncheckedAccount`; клиентским проверкам программа не доверяет.

Сжигать разрешено только владельцу `source`. Mint authority, delegate и multisig не заменяют его в этой инструкции. При успехе баланс и supply уменьшаются ровно на `amount`; остальные token accounts не затрагиваются. При отказе Solana откатывает изменения транзакции, включая CPI; комиссия payer при этом может списаться.

### Ожидаемые ошибки

| Сценарий | Ошибка |
| --- | --- |
| Нулевая сумма с корректными аккаунтами | `TokenStarterError::AmountMustBePositive` — `Amount must be greater than zero` |
| Authority не владеет source | Anchor `ConstraintTokenOwner` |
| Нет подписи authority | Anchor `AccountNotSigner` |
| Source относится к другому mint | Anchor `ConstraintTokenMint` |
| Mint принадлежит другой token program | Anchor `ConstraintMintTokenProgram` |
| Source принадлежит другой token program | Anchor `ConstraintTokenTokenProgram` |
| Вместо token program передана System Program | Anchor `InvalidProgramId` |
| Баланс source меньше amount | SPL `TokenError::InsufficientFunds` |

Anchor проверяет аккаунты до вызова обработчика, поэтому при одновременно неверных аккаунтах и нулевой сумме первым может быть отказ account constraint.

## Тестовое покрытие

Общие помощники находятся в `tests/common/mod.rs`, сценарии — в `tests/token_program.rs`. Каждый сценарий запускается отдельно для Token Program и Token-2022; у каждого запуска собственная LiteSVM. Mint, ATA и начальные балансы создаются через инструкции самой программы. Ключи тестов генерируются в памяти.

### Задание 1: 18 сценариев × 2 программы = 36 тестов

- Создание mint: decimals `0/6/9`, mint/freeze authority, supply, инициализация и программа-владелец.
- Создание ATA: owner, mint, token program, нулевой баланс и инициализация.
- Выпуск: первоначальный и повторный mint, изменения баланса и supply.
- Перевод: оба баланса, перевод всего остатка, неизменность supply.
- Отказы: нулевая сумма, неверный authority, отсутствие подписи, другой mint, одинаковые source/destination, подмена token program, недостаточный баланс.

### Задание 2: 9 сценариев × 2 программы = 18 тестов

- Успешное частичное сжигание, затем весь остаток source при decimals `0/6/9`: после каждой операции баланс и supply уменьшаются на одинаковый `amount`; чужой token account с ненулевым балансом остаётся неизменным.
- Нулевая сумма отклоняется точной пользовательской ошибкой программы.
- Mint authority не может сжечь токены другого владельца.
- Снятый `is_signer` проверяет отказ самой программы, а не только клиентского конструктора транзакции.
- Подмена mint отклоняется; проверяется также неизменность второго mint.
- Подмена token program проверяет соответствие владельца mint.
- Искусственная фикстура с заменённой программой-владельцем source изолированно проверяет `token::token_program`, сохраняя правильные поля mint и authority. Это не моделирование доступной пользователю смены владельца аккаунта в сети.
- Произвольная исполняемая программа вместо SPL отклоняется типом `Interface<TokenInterface>`.
- Недостаточный баланс отклоняется даже при достаточном общем supply: токены на чужом аккаунте нельзя использовать для сжигания.

**Каждый негативный тест** проверяет конкретный код ошибки и полное равенство mint и token accounts до/после транзакции, включая supply и балансы. Payer исключён из сравнения из-за комиссии.

## Безопасность

`target/`, `.anchor/`, `.keys/`, JSON-keypair, `.env*`, `*.pem` и `*.key` исключены через `.gitignore`. Перед публикацией проверяйте staged diff: ignore-правила не защищают уже отслеживаемые файлы. Не добавляйте секреты RPC, seed phrase, приватные ключи или кошельки с реальными средствами.

Это учебная программа, не прошедшая аудит для mainnet. Поддержка базовых Token-2022 операций не означает проверку всех сочетаний расширений mint. Дополнительные правила — в [SECURITY.md](SECURITY.md).

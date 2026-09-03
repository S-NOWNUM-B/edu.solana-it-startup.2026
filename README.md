# Solana Level 1 Token Starter

Учебный starter для итоговых заданий первого уровня курса Superteam KZ. Он показывает современный минимальный каркас токен-программы без привязки к legacy JavaScript SDK.

> Это исходная точка, а не готовое решение. Не работайте напрямую в ветке `main`: для каждого задания создавайте отдельную ветку.

## Как получить проект через GitHub

Если вы ещё не работали с GitHub:

1. Нажмите **Fork** в правом верхнем углу страницы и создайте копию репозитория в своём аккаунте.
2. На странице своей копии нажмите **Code** и скопируйте HTTPS-ссылку.
3. Выполните в терминале:

   ```bash
   git clone <ссылка-на-ваш-fork>
   cd education
   git checkout -b task/01-tests
   ```

4. После выполнения задания сохраните изменения:

   ```bash
   git add .
   git commit -m "Complete task 01 tests"
   git push -u origin task/01-tests
   ```

5. Отправьте преподавателю ссылку на ветку `task/01-tests` или на последний commit.

Не знаете Git? Для этих заданий достаточно операций `clone`, `checkout -b`, `add`, `commit` и `push`; команды выше можно использовать как готовый сценарий.

## Задание 1 — покрыть токен-программу тестами

В проекте уже есть минимальный LiteSVM-тест `create_token`. Его нужно усилить и добавить тесты остальных реализованных инструкций.

### Что нужно сделать

- В тесте `create_token` проверить `decimals`, mint authority, supply и владельца mint, а не только наличие аккаунта.
- Покрыть `create_token_account`: проверить владельца token account, mint и token program.
- Покрыть `mint_tokens`: проверить изменение баланса получателя и общего supply.
- Покрыть `transfer_tokens`: проверить оба баланса и неизменность общего supply.
- Добавить негативные сценарии: нулевая сумма, неверный authority, другой mint и одинаковые source/destination.
- Обновить README в своём fork: указать версии, команды запуска и кратко описать добавленные тесты.

### Готовность задания

Чистый checkout вашей ветки должен проходить:

```bash
anchor build --ignore-keys
cargo test --workspace --locked
```

Флаг `--ignore-keys` нужен только потому, что локальный program keypair намеренно не хранится в учебном репозитории. Для собственного devnet-деплоя создайте keypair локально и синхронизируйте ID командой `anchor keys sync`, но не добавляйте файл keypair в Git.

Не публикуйте keypair, seed phrase, приватные ключи или `.env` с секретами.

Следующие задания выполняются в ветках `task/02-burn` и `task/03-escrow`. Их условия выдаются на учебной платформе; готовой реализации в starter нет.

## Зафиксированный стек

- Anchor CLI и crates: `1.1.2`
- Solana CLI: `3.1.10`
- Rust: `1.89.0`
- тесты программ: Rust + LiteSVM `0.10.0`
- токены: `anchor_spl::token_interface`, совместимый с Token Program и Token-2022
- рекомендуемый клиент для нового TypeScript-кода: `@solana/kit`

`@solana/web3.js` относится к legacy-стеку. TypeScript-клиент Anchor `@anchor-lang/core` по-прежнему зависит от `@solana/web3.js` v1, поэтому в этом starter тесты написаны на Rust и LiteSVM. Для нового клиентского приложения используйте `@solana/kit`, если задание явно не требует другого.

Оригинальный Token Program остается рабочим и широко используется. Для новых токенов в учебных заданиях используйте Token-2022, а program-код пишите через `token_interface`, чтобы сохранить совместимость с обоими Token Program.

## Что уже реализовано

- создание mint с выбранной token-программой;
- создание associated token account;
- выпуск токенов через `mint_to`;
- перевод через `transfer_checked`;
- проверки положительной суммы, полномочий, mint и token program на уровне Anchor accounts constraints;
- один эталонный LiteSVM-тест создания Token-2022 mint.

Функции `burn_tokens` и Escrow намеренно отсутствуют: студент реализует их в следующих заданиях.

## Быстрый старт

1. Установите версии из раздела «Зафиксированный стек» через AVM, rustup и официальный Solana installer.
2. Для локального прохождения заданий выполните `anchor build --ignore-keys`. Для собственного devnet-деплоя создайте локальный program keypair и выполните `anchor keys sync`. Не коммитьте keypair или seed phrase.
3. После первой сборки выполните `cargo test --workspace --locked`.
4. Разрабатывайте каждое задание в отдельной ветке: `task/01-tests`, `task/02-burn`, `task/03-escrow`.

Тест загружает собранный файл `target/deploy/solana_level_1_token_starter.so`, поэтому перед первым `cargo test` нужен `anchor build --ignore-keys`.

## Правила сдачи

- сдавайте публичную ссылку на GitHub-репозиторий и указывайте ветку или commit SHA;
- добавьте в README команды сборки и тестирования, ожидаемый результат и краткое описание архитектуры;
- не добавляйте в репозиторий private keys, seed phrases, `.env` с секретами или файлы keypair;
- не используйте `@solana/web3.js` в новом клиентском коде;
- для переводов токенов используйте `transfer_checked`, а не unchecked transfer;
- не подменяйте проверки полномочий только клиентской логикой: все критичные инварианты должны проверяться программой.

## Что считается современным решением

Современность здесь определяется не только номером версии. Решение должно использовать строгие account constraints, проверяемые state transitions, Token-2022 для нового токена, `token_interface` для совместимости, `transfer_checked` для переводов и воспроизводимые LiteSVM-тесты. Если официальные стабильные рекомендации Solana или Anchor изменятся, студент должен зафиксировать выбранные версии и объяснить отклонение в README.

## Результат задания 1 — ветка `task/01-tests`

Исходные условия выше сохранены. Вместо минимального `tests/create_token.rs` добавлены `programs/solana-level-1-token-starter/tests/token_program.rs` и общий модуль `tests/common/mod.rs`: 18 сценариев, каждый отдельно для Token-2022 и Token Program (36 интеграционных тестов).

Версии сохранены: Anchor CLI/crates `1.1.2`, Solana CLI `3.1.10`, Rust `1.89.0`, LiteSVM `0.10.0`. Новых зависимостей нет, `Cargo.lock` не изменён.

Команды из корня проекта после установки этих версий:

```bash
anchor build --ignore-keys
cargo test --workspace --locked
cargo fmt --all -- --check
```

Ожидаемый результат: собраны `.so` и IDL; проходят 36 интеграционных тестов и стандартный unit-тест `test_id`, без падений; форматирование проходит проверку.

Покрытие тестами:

- `create_token`: decimals 0/6/9, mint authority, freeze authority, нулевой supply, инициализация и token-программа — владелец mint.
- `create_token_account`: создание ATA, owner, mint, token-программа, нулевой баланс и инициализация.
- `mint_tokens`: первоначальный и повторный выпуск, точные изменения баланса и supply.
- `transfer_tokens`: оба баланса, перевод всего остатка, сохранение supply; получатель имеет ненулевой начальный баланс.
- Обязательные отказы: нулевая сумма, неверный authority, другой mint, одинаковые source/destination. Дополнительно: отсутствие подписи authority, подмена token-программы и недостаточный баланс.
- При каждом отказе проверяются конкретная ошибка и неизменность mint/token accounts целиком. Payer исключён из сравнения из-за комиссии.

Архитектура: `src/lib.rs` объявляет четыре инструкции, `src/instructions/` содержит account constraints и CPI через `token_interface`; перевод использует `transfer_checked`. Тесты создают mint и ATA через саму программу в отдельной LiteSVM для каждого запуска. Ключи генерируются в памяти; RPC, validator и файл кошелька не нужны. Код программы не изменён.

Особенности Anchor 1.1.2: одинаковые изменяемые аккаунты отклоняются с `ConstraintDuplicateMutableAccount` (2040) раньше пользовательской ошибки `SourceEqualsDestination`. Подмена token-программы при создании ATA даёт `IncorrectProgramId` из CPI, при mint/transfer — `ConstraintMintTokenProgram` (2022).

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

## Результат задания 2 — ветка `task/02-burn`

Исходные условия и итоги задания 1 выше сохранены без изменений; этот раздел описывает состояние после задания 2.

Добавлена инструкция `burn_tokens(amount: u64)` в `src/instructions/burn_tokens.rs` и подключена в `src/lib.rs`. Сжигание выполняется через `anchor_spl::token_interface::burn_checked`, decimals берутся из проверенного mint. Сумма задаётся в минимальных единицах токена. Нулевая сумма отклоняется ошибкой `TokenStarterError::AmountMustBePositive`.

Account constraints:

- `authority: Signer` — обязательная подпись владельца source; mint authority не даёт права сжигать чужие токены.
- `mint: InterfaceAccount<Mint>` — изменяемый аккаунт с `mint::token_program = token_program`.
- `source: InterfaceAccount<TokenAccount>` — изменяемый аккаунт с `token::mint = mint`, `token::authority = authority` и `token::token_program = token_program`.
- `token_program: Interface<TokenInterface>` — только исполняемая Token Program или Token-2022; mint и source должны принадлежать именно выбранной программе.
- Критичные аккаунты новой инструкции не используют `UncheckedAccount`; все проверки выполняются программой, а не клиентом. Недостаточный баланс отклоняет SPL-программа внутри CPI.

Добавлены 9 сценариев для каждой token-программы — 18 новых интеграционных тестов:

- Частичное сжигание и весь остаток при decimals 0/6/9: баланс source и общий supply уменьшаются ровно на amount, чужой token account не меняется.
- Отказы: нулевая сумма, неверный authority, отсутствие подписи, другой mint, подмена token program у mint и source, передача System Program вместо SPL и недостаточный баланс даже при достаточном общем supply.
- Для каждого отказа проверяются конкретная ошибка и неизменность аккаунтов целиком, включая supply и балансы. Payer исключён из сравнения из-за комиссии.
- Проверка программы-владельца source использует искусственную LiteSVM-фикстуру с изменённым owner, чтобы изолировать `token::token_program`.

Версии сохранены: Anchor CLI/crates `1.1.2`, Solana CLI `3.1.10`, Rust `1.89.0`, LiteSVM `0.10.0`. Новых зависимостей нет, `Cargo.lock` не изменён.

Команды из корня проекта:

```bash
anchor build
cargo test
cargo test --workspace --locked
cargo fmt --all -- --check
```

Проверено на отдельном чистом checkout: сборка создаёт `.so` и IDL, проходят 54 интеграционных теста и unit-тест `test_id`; форматирование проходит проверку. Перед тестами и после изменения программы нужно пересобирать `.so`.

Уточнение к исходной инструкции сборки: в Anchor CLI `1.1.2` несовпадение автоматически созданного локального program keypair с `declare_id!` выводит предупреждение, но не останавливает `anchor build`. Для LiteSVM флаг `--ignore-keys` необязателен: тест загружает программу под объявленным ID без деплоя и использования keypair. Ключи и секреты не опубликованы; дополнительно исключены `.env`, `.env.*`, `*.pem` и `*.key`.

## Результат задания 3 — ветка `task/03-escrow`

Добавлена отдельная Anchor-программа `programs/escrow`. Предыдущая токен-программа и её тесты не изменены. Стек сохранён: Anchor CLI/crates `1.1.2`, Solana CLI `3.1.10`, Rust `1.89.0`, LiteSVM `0.10.0`. В `Cargo.lock` добавлен только локальный пакет `escrow`; версии существующих зависимостей не изменились. TypeScript-код не нужен.

### Архитектура

- `src/state.rs`: `EscrowState` хранит sender, receiver, mint, amount, `deal_id: u64`, bump и статус. PDA: `[b"escrow", sender, deal_id.to_le_bytes()]`; sender может создавать несколько независимых сделок, одинаковый ID у разных sender допустим.
- Vault — отдельный Token-2022 account с PDA `[b"vault", escrow_state]`, mint сделки и authority = PDA state. Vault создаётся при `initialize` и не разделяется между сделками.
- `DealReceipt` — PDA `[b"used", sender, deal_id.to_le_bytes()]`, постоянная отметка использования ID и текущего/итогового статуса. Закрытие state не позволяет повторно открыть сделку с тем же ID.
- `src/instructions.rs`: типизированные аккаунты, `Signer`, `has_one`, проверка seeds/stored bump и статуса, `token::mint/authority/token_program`, ATA constraints. `Interface<TokenInterface>` дополнительно ограничен адресом Token-2022: legacy Token Program не допускается. Критичных `UncheckedAccount` нет.
- `src/lib.rs`: четыре инструкции и переходы состояния. `src/token_cpi.rs`: переводы из vault через `token_interface::transfer_checked` с PDA signer seeds и закрытие через `token_interface::close_account`. Decimals берутся из mint. В `release` крупные account-обёртки помещены в `Box`, чтобы не превышать SBF-стек 4 KiB.

### State machine

| Инструкция | До → после | Действие |
| --- | --- | --- |
| `initialize(deal_id, amount)` | Отсутствует → Created | Sender оплачивает создание state, receipt и vault; amount > 0, receiver отличается от sender |
| `deposit()` | Created → Funded | Только sender; ровно сохранённый amount переводится из его token account в vault |
| `release()` | Funded → Released | Только sender; ровно amount переводится в ATA сохранённого receiver, излишек vault — в ATA sender |
| `cancel()` | Created / Funded → Cancelled | Только sender; весь баланс vault, включая посторонние поступления, возвращается в ATA sender |

После `release/cancel` пустой vault закрывается через SPL CPI, state — через `close = sender`. Их rent полностью возвращается sender. Терминальный статус остаётся в receipt. Повторное завершение отклоняется, поскольку state уже закрыт; повторное создание ID — поскольку receipt уже существует.

`deposit`, `release` и `cancel` не принимают сумму от клиента: amount фиксируется при создании. Прямое пополнение vault через SPL само по себе не меняет Created на Funded. Sender и receiver представлены обычными кошельками; receiver — `SystemAccount`, без требования его подписи. Перед `release` должны существовать ATA receiver и sender, перед `cancel` — ATA sender. Их можно создать стандартной Associated Token Program отдельной инструкцией, в том числе в той же транзакции. Внутри escrow не используется `init_if_needed`.

### Архитектурные решения и ограничения

- **Защита от повторного ID после закрытия.** Полностью удалить все следы сделки и одновременно запретить повторное использование произвольного ID невозможно. Вместо незакрытого state или ограничения ID монотонным счётчиком выбран маленький receipt: 8 байт discriminator + 1 байт статуса. Его rent остаётся заблокированным; state и vault закрываются по условию. Это осознанная плата за постоянную защиту от повторов.
- **Ограниченный набор mint.** Принимается только базовый Token-2022 mint без расширений и freeze authority. Так исключаются transfer fees, hooks, permanent delegate и заморозка, которые могут нарушить точную сумму перевода или доступность возврата. Это не универсальная поддержка расширений Token-2022; расширять набор следует отдельно вместе с проверками соответствующих угроз.
- **Посторонние поступления.** Требование `vault.amount == amount` позволило бы постороннему заблокировать закрытие переводом одного токена. Поэтому receiver получает только согласованный amount, а излишек возвращается sender. Поступление в чужой vault следует считать безвозвратным пожертвованием sender.

### Threat model и ошибки

Недоверенный клиент может подставлять любые аккаунты, менять метаданные подписей и порядок вызовов. Защита выполняется в программе:

- Нет подписи — `AccountNotSigner`; другой sender — `UnauthorizedSender`; sender = receiver — `SenderEqualsReceiver`; нулевая сумма — `AmountMustBePositive`.
- Подмена mint/receiver — `InvalidMint` / `InvalidReceiver`. Подмена state, receipt или vault — PDA constraints; mint, authority и программа-владелец token accounts проверяются отдельно. Выплата и возврат требуют канонические ATA нужного владельца.
- Повторный deposit, release до deposit или несовместимый статус — `InvalidStatus`. После закрытия — `AccountNotInitialized`. Повторный ID отклоняет `init` существующего receipt (System Program `AccountAlreadyInUse`).
- Недостаточно токенов у sender — SPL `InsufficientFunds`; баланс Funded-vault меньше amount — `InvalidVaultBalance`. Неподдерживаемый mint — `UnsupportedMint`; другая SPL-программа или произвольная программа отклоняются constraints/типом `Interface`.
- Ошибка любого CPI откатывает всю инструкцию/транзакцию, включая предыдущие переводы, закрытия, rent и смену статуса. Комиссия fee payer может быть списана и при отказе.

Границы модели: решение управляется sender, не арбитром; receiver не может самостоятельно забрать средства или отменить сделку, срок исполнения не задан. Upgrade authority программы, компрометация ключа sender и доступность сети вне этих гарантий. Mainnet-аудит не проводился. Локальные keypair остаются в игнорируемом `target/`, ключи тестов генерируются в памяти; секретов в исходниках нет.

### Команды и результаты тестов

Из корня workspace после установки зафиксированных версий (zsh/bash):

```bash
anchor build
cargo test
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy -p escrow --all-targets --locked -- -D warnings
```

`anchor build` собирает обе программы и IDL. До тестов нужен свежий `target/deploy/escrow.so`; RPC, validator и кошелёк не требуются. Только escrow-тесты можно запустить командой `cargo test -p escrow --locked` (zsh/bash).

Проверено в отдельной чистой копии исходников без `target/` и program keypair: `anchor build`, `cargo test`, форматирование и строгий Clippy новой программы проходят. Дополнительно проходит `cargo test --workspace --locked`; lockfile при проверках не меняется.

Добавлены **36 escrow-тестов**; вместе с предыдущими 54 токен-тестами — **90 интеграционных тестов и 2 unit-теста `test_id`**. Покрыты release/cancel end-to-end, отмена Created, проверка полей state и vault, точные балансы и supply, закрытие аккаунтов и возврат rent. Негативные сценарии включают все перечисленные выше ошибки, повтор ID до/после завершения, изоляцию сделок, подмену vault/receipt и неверный bump, неподдерживаемые mint, посторонние поступления, откат успешного первого CPI при отказе второго и откат release при ошибке следующей инструкции транзакции.

Каждый негативный тест сравнивает полное состояние задействованных аккаунтов до/после отказа, включая sender, receipt и ещё не созданные PDA; исключён только fee payer. Тесты повреждённых bump/owner/balance и заморозки используют явно обозначенные искусственные LiteSVM-фикстуры — в сети посторонний не может напрямую переписать эти поля.

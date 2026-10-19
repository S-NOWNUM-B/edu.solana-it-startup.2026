<div align="center">

# SNOWNUMB Pay

Локальный кабинет счетов в USDC на Solana devnet

![Rust](https://img.shields.io/badge/Rust-1.89.0-000000?style=for-the-badge&logo=rust&logoColor=white)
![Solana](https://img.shields.io/badge/Solana-devnet-14F195?style=for-the-badge&logo=solana&logoColor=black)

</div>

Продавец создаёт счёт, получает публичную страницу с Solana Pay QR и закрывает ненужные счета. Фоновый worker и кнопка проверки независимо читают finalized-транзакции через RPC. Сервер не подписывает переводы: плательщик использует собственный кошелёк, USDC поступает напрямую продавцу.

<div align="center">

## Запуск

</div>

В терминале macOS zsh/bash из корня репозитория, Rust 1.89.0:

```bash
# Собрать и запустить сервер с проверкой devnet genesis и USDC mint
cargo run -p payments-client --locked
```

Откройте `http://127.0.0.1:8790`. Для кабинета скопируйте токен из `.anchor/payments-admin-token.txt` в поле доступа. Файл создаётся с правами 0600 и исключён из Git; токен не попадает в URL и остаётся в памяти страницы. При перезагрузке страницы войдите снова. Вместо файла можно заранее задать `PAYMENTS_ADMIN_TOKEN`: 24–128 латинских букв, цифр или дефисов. Не публикуйте токен и не вставляйте его в снимки экрана.

```bash
# Использовать свой публичный адрес продавца и другой локальный порт
cargo run -p payments-client --locked -- --wallet <PUBKEY> --port 8791
```

Параметры `--url <DEVNET_RPC>` и `--store <PATH>` меняют RPC и путь JSON. Кластер должен оставаться devnet. Адрес сервера — строго `127.0.0.1`, не `localhost`; Host и Origin проверяются. Другой процесс не может одновременно открыть тот же store. При смене продавца используйте новый путь store: несовместимые сохранённые реквизиты отклоняются.

<div align="center">

## Сценарий

</div>

1. Введите токен оператора и создайте счёт, например «Разработка интерфейса» на `125.50` USDC. Повтор создания с тем же Idempotency-Key и телом вернёт прежний счёт; изменение тела даст 409.
2. Откройте публичную страницу счёта. Она показывает сеть, сумму, mint, получателя, ATA и reference; можно скопировать ссылку или Solana Pay URI.
3. Для фактической оплаты переключите совместимый кошелёк в devnet. URI не содержит вымышленного параметра cluster. На телефоне адрес `127.0.0.1` не открывает сервер компьютера; QR содержит непосредственно Solana Pay URI, а не ссылку на локальный сайт.
4. Нажмите проверку либо дождитесь следующего прохода worker с интервалом 10 секунд. paid появляется только после проверки finalized-транзакции; затем доступна JSON-квитанция.
5. Закрытие разрешено только для open. Поздние, повторные, отклонённые и неоднозначные переводы сохраняются в журнале; автоматического возврата нет.

<div align="center">

## Платёж и хранение

</div>

Фиксированы devnet USDC mint `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU`, SPL Token Program и 6 decimals. Суммы разбираются в целые u64, передаются API строками; ноль, знак, экспонента и дробь длиннее 6 знаков отклоняются. `125.50` нормализуется в `125.5`, amount_raw — `125500000`.

Проверяются подпись транзакции, отсутствие onchain-ошибки, канонический ATA продавца, mint/owner/program/decimals, точные дельты отправителя и получателя и reference как readonly nonsigner в самой инструкции перевода. Поддерживаются compiled legacy/v0; неизвестные версии и неоднозначные наборы инструкций требуют ручной проверки. Сверка допускает один SPL-перевод и ограниченный набор сопутствующих инструкций, включая создание ATA. Произвольная композиция переводов не принимается.

`.anchor/payments-store.json` содержит счета, попытки и квитанции. Запись идёт через уникальный файл с atomic rename; lock предотвращает одновременный запуск с одним store. Это хранение для одного локального процесса, без PostgreSQL, резервного копирования и гарантий восстановления после потери питания. Данные сохраняются после обычного перезапуска. Случайный public_id даёт доступ к реквизитам счёта всем, кто знает ссылку; персональные данные в заголовок не добавляйте.

<div align="center">

## HTTP API

</div>

Все тела POST — JSON. Тело check и close должно быть пустым объектом. Лимит 16 KiB; чужой Origin, неизвестные поля создания и клиентский признак paid отклоняются.

<table align="center">
  <tr><th align="center" valign="middle">Метод и путь</th><th align="center" valign="middle">Доступ</th><th align="center" valign="middle">Результат</th></tr>
  <tr><td align="center" valign="middle">GET <code>/api/config</code></td><td align="center" valign="middle">Публичный</td><td align="center" valign="middle">Сеть, mint, продавец и finalized</td></tr>
  <tr><td align="center" valign="middle">GET <code>/api/invoices</code></td><td align="center" valign="middle">Bearer</td><td align="center" valign="middle">Список счетов</td></tr>
  <tr><td align="center" valign="middle">POST <code>/api/invoices</code></td><td align="center" valign="middle">Bearer и Idempotency-Key</td><td align="center" valign="middle"><code>{title,amount}</code> → счёт open</td></tr>
  <tr><td align="center" valign="middle">GET <code>/api/invoices/{id}</code></td><td align="center" valign="middle">Публичный</td><td align="center" valign="middle">Счёт, попытки, квитанция</td></tr>
  <tr><td align="center" valign="middle">POST <code>/api/invoices/{id}/check</code></td><td align="center" valign="middle">Публичный</td><td align="center" valign="middle">Серверная RPC-сверка и rpc_error</td></tr>
  <tr><td align="center" valign="middle">POST <code>/api/invoices/{id}/close</code></td><td align="center" valign="middle">Bearer</td><td align="center" valign="middle">open → closed</td></tr>
  <tr><td align="center" valign="middle">GET <code>/api/invoices/{id}/qr.svg</code></td><td align="center" valign="middle">Публичный</td><td align="center" valign="middle">QR с Solana Pay URI</td></tr>
  <tr><td align="center" valign="middle">GET <code>/api/invoices/{id}/receipt</code></td><td align="center" valign="middle">Публичный</td><td align="center" valign="middle">JSON для paid, иначе 409</td></tr>
</table>

За один поиск обрабатываются до 30 последних кандидатов, по 10 на страницу, с бюджетом около 20 секунд и таймаутом RPC 5 секунд. Переполнение или недоступность RPC возвращает rpc_error и сохраняет прежнее состояние. Постоянного курсора и гарантированного обхода всей истории нет. Публичная сверка сериализована; защиты публичного сервиса от DoS и многопользовательского доступа этот loopback MVP не предоставляет.

<div align="center">

## Проверки и результат

</div>

В терминале macOS zsh/bash из корня репозитория:

```bash
# Проверить формат, предупреждения и платёжные сценарии
cargo fmt --all -- --check
cargo clippy -p payments-client --all-targets --locked -- -D warnings
cargo test -p payments-client --locked
```

16 автоматических тестов прошли: точность сумм, persistence/idempotency/lock, повторный и поздний перевод, проверка транзакций, полный путь RPC → paid → receipt, ошибки RPC и HTTP-защита. Положительный платёжный путь использует контролируемые RPC-ответы, не перевод в сети.

При запуске выполнены 16 HTTP-проверок, реальное чтение devnet genesis и mint, поиск по reference открытого счёта без найденных переводов и перезапуск с сохранением двух счетов open/closed. Фактическая оплата USDC, подтверждение физическим кошельком, публичный deploy, mainnet и работа с менторами не проверялись. PostgreSQL, несколько продавцов, полный workflow и мониторинг остаются развитием [проекта задания 13](../../docs/stablecoin-payments.md).

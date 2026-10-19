"use strict";

const $ = selector => document.querySelector(selector);
const publicMatch = window.location.pathname.match(/^\/pay\/([^/]+)\/?$/);
const publicId = publicMatch ? decodeURIComponent(publicMatch[1]) : null;
const states = { open: "Ожидает оплаты", paid: "Оплачен", closed: "Закрыт" };
let adminToken = "";
let invoices = [];
let selectedInvoice = null;
let operationPending = false;
let pendingCreate = null;

function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  return element;
}

function quantity(value) {
  const [whole, fraction] = String(value).split(".");
  return whole.replace(/\B(?=(\d{3})+(?!\d))/g, "\u202f") + (fraction ? `.${fraction}` : "");
}

function short(value) {
  return value.length > 20 ? `${value.slice(0, 8)}…${value.slice(-8)}` : value;
}

function message(target, text, kind = "") {
  target.textContent = text;
  target.className = `status${kind ? ` ${kind}` : ""}`;
}

function notify(text, failure = false) {
  const target = $("#notification");
  target.textContent = text;
  target.className = `notification${failure ? " error" : ""}`;
}

function errorText(error) {
  if (error.name === "AbortError") return "Сервер не ответил вовремя. Повторите запрос.";
  if (error instanceof TypeError) return "Не удалось связаться с сервером. Проверьте соединение и повторите запрос.";
  return error.message;
}

async function request(path, { method = "GET", body, admin = false, idempotencyKey } = {}) {
  const headers = { Accept: "application/json" };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (admin) headers.Authorization = `Bearer ${adminToken}`;
  if (idempotencyKey) headers["Idempotency-Key"] = idempotencyKey;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 65000);
  try {
    const response = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), cache: "no-store", signal: controller.signal, credentials: "same-origin" });
    let data;
    try { data = await response.json(); } catch { throw new Error("Сервер вернул некорректный ответ. Повторите запрос."); }
    if (!response.ok) throw new Error(typeof data.error === "string" ? data.error : `Ошибка запроса (${response.status}).`);
    return data;
  } finally {
    clearTimeout(timeout);
  }
}

function invoicePath(id) {
  return `/api/invoices/${encodeURIComponent(id)}`;
}

function setAccess(connected) {
  $("#access-form").hidden = connected;
  $("#disconnect").hidden = !connected;
  $("#invoice-fields").disabled = !connected;
  $("#refresh-invoices").disabled = !connected;
  $("#access-description").textContent = connected ? "Доступ открыт. Токен хранится только в памяти этой страницы." : "Токен оператора нужен для управления счетами.";
}

function renderList() {
  const target = $("#invoices");
  $("#invoice-count").textContent = String(invoices.length);
  $("#open-count").textContent = String(invoices.filter(invoice => invoice.state === "open").length);
  $("#paid-count").textContent = String(invoices.filter(invoice => invoice.state === "paid").length);
  if (!invoices.length) {
    const row = node("tr");
    const cell = node("td", "Счетов пока нет. Создайте первый слева.", "empty");
    cell.colSpan = 3;
    row.append(cell);
    target.replaceChildren(row);
    return;
  }
  target.replaceChildren(...invoices.map(invoice => {
    const row = node("tr", undefined, invoice.public_id === selectedInvoice?.public_id ? "selected-row" : "");
    const title = node("td");
    const button = node("button", invoice.title, "invoice-button");
    button.type = "button";
    button.dataset.invoiceId = invoice.public_id;
    button.addEventListener("click", () => openInvoice(invoice.public_id, true));
    title.append(button, node("span", short(invoice.public_id), "invoice-id"));
    const state = node("td");
    state.append(node("span", states[invoice.state] || invoice.state, `badge ${invoice.state}`));
    row.append(title, state, node("td", quantity(invoice.amount), "align-right mono value"));
    return row;
  }));
}

function explorer(signature) {
  const link = node("a", short(signature), "mono");
  link.href = `https://explorer.solana.com/tx/${encodeURIComponent(signature)}?cluster=devnet`;
  link.title = signature;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  link.setAttribute("aria-label", `Транзакция ${signature} в Solana Explorer`);
  return link;
}

function renderInvoice(invoice) {
  selectedInvoice = invoice;
  $("#payment").hidden = false;
  $("#payment-content").hidden = false;
  $("#payment-title").textContent = invoice.title;
  $("#payment-amount").textContent = quantity(invoice.amount);
  $("#invoice-state").textContent = states[invoice.state] || invoice.state;
  $("#invoice-state").className = `badge ${invoice.state}`;
  $("#merchant-wallet").textContent = invoice.merchant_wallet;
  $("#recipient-ata").textContent = invoice.recipient_ata;
  $("#payment-mint").textContent = invoice.mint;
  $("#payment-reference").textContent = invoice.reference;
  $("#payment-uri").textContent = invoice.solana_pay_url;
  $("#payment-qr").src = `${invoicePath(invoice.public_id)}/qr.svg`;
  $("#wallet-link").href = invoice.solana_pay_url.startsWith("solana:") ? invoice.solana_pay_url : "#";
  $("#public-link").href = `/pay/${encodeURIComponent(invoice.public_id)}`;
  $("#public-link").hidden = !!publicId;
  $("#payment-actions").hidden = invoice.state !== "open";
  $("#paid-note").hidden = invoice.state !== "paid";
  $("#closed-note").hidden = invoice.state !== "closed";
  $("#check-payment").hidden = invoice.state !== "open";
  $("#close-invoice").hidden = !!publicId || !adminToken || invoice.state !== "open";
  $("#receipt-link").href = `${invoicePath(invoice.public_id)}/receipt`;
  $("#receipt-details").hidden = !invoice.receipt;
  if (invoice.receipt) {
    $("#receipt-signature").replaceChildren(explorer(invoice.receipt.signature));
    $("#receipt-slot").textContent = String(invoice.receipt.slot);
  }
  $("#attempts").replaceChildren(...(invoice.attempts.length ? invoice.attempts.map(attempt => {
    const item = node("div", undefined, "attempt");
    const header = node("div", undefined, "attempt-head");
    const labels = { verified: "Подтверждён", rejected: "Отклонён", review_required: "Требует проверки" };
    header.append(node("span", labels[attempt.outcome] || attempt.outcome, `badge ${attempt.outcome === "review_required" ? "review" : attempt.outcome}`), explorer(attempt.signature));
    item.append(header, node("p", attempt.reason));
    return item;
  }) : [node("p", "Проверенных переводов пока нет.", "empty")]));
  if (!publicId) {
    const index = invoices.findIndex(item => item.public_id === invoice.public_id);
    if (index !== -1) invoices[index] = invoice;
    renderList();
  }
}

async function loadInvoices() {
  const data = await request("/api/invoices", { admin: true });
  invoices = data.invoices;
  renderList();
}

async function openInvoice(id, focus = false) {
  if (operationPending) return;
  $("#payment").hidden = false;
  $("#payment").setAttribute("aria-busy", "true");
  message($("#payment-status"), "Получаю данные счёта…");
  try {
    const data = await request(invoicePath(id));
    renderInvoice(data.invoice);
    message($("#payment-status"), data.invoice.state === "open" ? "Счёт готов к оплате. Сохраните reference при переводе через Solana Pay." : "Статус счёта получен с сервера.");
    if (focus) $("#payment").scrollIntoView({ behavior: "instant", block: "start" });
  } catch (error) {
    $("#payment-content").hidden = true;
    $("#payment-title").textContent = "Счёт недоступен";
    message($("#payment-status"), errorText(error), "error");
  } finally {
    $("#payment").setAttribute("aria-busy", "false");
  }
}

$("#access-form").addEventListener("submit", async event => {
  event.preventDefault();
  if (operationPending) return;
  adminToken = $("#admin-token").value.trim();
  if (!adminToken) return;
  operationPending = true;
  $("#connect").disabled = true;
  message($("#operator-status"), "Проверяю доступ…");
  try {
    await loadInvoices();
    $("#admin-token").value = "";
    setAccess(true);
    message($("#operator-status"), "Рабочее пространство открыто.", "success");
  } catch (error) {
    adminToken = "";
    setAccess(false);
    message($("#operator-status"), errorText(error), "error");
  } finally {
    operationPending = false;
    $("#connect").disabled = false;
  }
});

$("#disconnect").addEventListener("click", () => {
  if (operationPending) return;
  adminToken = "";
  invoices = [];
  selectedInvoice = null;
  setAccess(false);
  $("#payment").hidden = true;
  renderList();
  for (const id of ["invoice-count", "open-count", "paid-count"]) document.getElementById(id).textContent = "—";
  $("#invoices").firstElementChild.firstElementChild.textContent = "Войдите, чтобы увидеть счета.";
  message($("#operator-status"), "Доступ закрыт. Токен удалён из памяти страницы.");
  $("#admin-token").focus();
});

$("#refresh-invoices").addEventListener("click", async () => {
  const button = $("#refresh-invoices");
  button.disabled = true;
  message($("#operator-status"), "Обновляю счета…");
  try { await loadInvoices(); message($("#operator-status"), "Список счетов обновлён.", "success"); }
  catch (error) { message($("#operator-status"), errorText(error), "error"); }
  finally { button.disabled = !adminToken; }
});

for (const id of ["invoice-title", "invoice-amount"]) document.getElementById(id).addEventListener("input", event => event.target.setCustomValidity(""));

$("#invoice-form").addEventListener("submit", async event => {
  event.preventDefault();
  if (operationPending || !adminToken) return;
  const title = $("#invoice-title").value.trim();
  const amount = $("#invoice-amount").value.trim();
  if (!title || new TextEncoder().encode(title).length > 100) {
    $("#invoice-title").setCustomValidity("Введите назначение длиной до 100 байт UTF-8 (для кириллицы — до 50 букв).");
    $("#invoice-title").reportValidity();
    return;
  }
  if (!/^\d+(?:\.\d{1,6})?$/.test(amount) || !/[1-9]/.test(amount)) {
    $("#invoice-amount").setCustomValidity("Введите сумму больше нуля: точка и не более 6 знаков после неё.");
    $("#invoice-amount").reportValidity();
    return;
  }
  const body = { title, amount };
  const fingerprint = JSON.stringify(body);
  if (!pendingCreate || pendingCreate.fingerprint !== fingerprint) pendingCreate = { fingerprint, key: crypto.randomUUID() };
  operationPending = true;
  $("#create-invoice").disabled = true;
  message($("#operator-status"), "Создаю счёт…");
  try {
    const data = await request("/api/invoices", { method: "POST", admin: true, body, idempotencyKey: pendingCreate.key });
    pendingCreate = null;
    if (!invoices.some(invoice => invoice.public_id === data.invoice.public_id)) invoices.unshift(data.invoice);
    renderInvoice(data.invoice);
    $("#invoice-form").reset();
    message($("#operator-status"), "Счёт создан. Скопируйте ссылку и отправьте плательщику.", "success");
    message($("#payment-status"), "Счёт готов к оплате. Платёж будет проверен по reference.");
  } catch (error) {
    message($("#operator-status"), errorText(error), "error");
  } finally {
    operationPending = false;
    $("#create-invoice").disabled = false;
  }
});

$("#check-payment").addEventListener("click", async () => {
  if (!selectedInvoice || operationPending) return;
  operationPending = true;
  const id = selectedInvoice.public_id;
  $("#check-payment").disabled = true;
  $("#check-payment").textContent = "Проверяю переводы…";
  $("#payment").setAttribute("aria-busy", "true");
  message($("#payment-status"), "Ищу перевод по reference и проверяю сумму, получателя и mint в RPC…");
  try {
    const data = await request(`${invoicePath(id)}/check`, { method: "POST", body: {} });
    renderInvoice(data.invoice);
    if (data.rpc_error) message($("#payment-status"), `Проверка неполная: ${data.rpc_error} Статус счёта сохранён. Повторите проверку.`, "error");
    else if (data.invoice.state === "paid") message($("#payment-status"), "Оплата подтверждена. Квитанция доступна для скачивания.", "success");
    else message($("#payment-status"), `Подходящий платёж пока не подтверждён. Проверено переводов: ${data.checked}. Если перевод отправлен, дождитесь finalized и повторите проверку.`);
  } catch (error) {
    message($("#payment-status"), `${errorText(error)} Статус счёта не изменён.`, "error");
  } finally {
    operationPending = false;
    $("#check-payment").disabled = false;
    $("#check-payment").textContent = "Проверить оплату ↻";
    $("#payment").setAttribute("aria-busy", "false");
  }
});

$("#close-invoice").addEventListener("click", async () => {
  if (!selectedInvoice || operationPending || !adminToken) return;
  operationPending = true;
  $("#close-invoice").disabled = true;
  try {
    const data = await request(`${invoicePath(selectedInvoice.public_id)}/close`, { method: "POST", admin: true, body: {} });
    renderInvoice(data.invoice);
    message($("#payment-status"), "Счёт закрыт. Его ссылка больше не предназначена для оплаты.");
  } catch (error) { message($("#payment-status"), errorText(error), "error"); }
  finally { operationPending = false; $("#close-invoice").disabled = false; }
});

async function copy(text, label) {
  try { await navigator.clipboard.writeText(text); notify(`${label} скопирована.`); }
  catch { notify("Не удалось скопировать автоматически. Выделите ссылку в разделе «Ссылка Solana Pay» или скопируйте адрес страницы.", true); }
}
$("#copy-uri").addEventListener("click", () => { if (selectedInvoice) copy(selectedInvoice.solana_pay_url, "Ссылка Solana Pay"); });
$("#copy-link").addEventListener("click", () => { if (selectedInvoice) copy(`${window.location.origin}/pay/${encodeURIComponent(selectedInvoice.public_id)}`, "Ссылка оплаты"); });
$("#payment-qr").addEventListener("error", () => notify("QR-код недоступен. Используйте кнопку «Открыть в кошельке» или скопируйте Solana Pay URI.", true));

async function initialize() {
  document.body.classList.toggle("public-view", !!publicId);
  $("#operator").hidden = !!publicId;
  if (publicId) {
    document.title = "Оплата счёта · SNOWNUMB Pay";
    await openInvoice(publicId);
  } else {
    try {
      const config = await request("/api/config");
      if (config.network !== "devnet" || config.decimals !== 6) throw new Error("Конфигурация сервера не соответствует USDC в Solana devnet.");
    } catch (error) {
      $("#connect").disabled = true;
      message($("#operator-status"), errorText(error), "error");
    }
  }
}

initialize();

// Synthetic custom dashboard: what a hand-written HA REST/WebSocket panel does.
'use strict';

const BASE = location.origin;
const TOKEN = 'synthetic-token';
const entities = new Map();
let ws = null;
let msgId = 0;
const pending = new Map();

const $ = (sel, root = document) => root.querySelector(sel);

function formatState(e) {
  const unit = e.attributes?.unit_of_measurement ?? '';
  return `${e.state}${unit ? ` ${unit}` : ''}`;
}

function render(e) {
  let card = document.getElementById(`card-${e.entity_id}`);
  if (!card) {
    const tpl = document.getElementById('card-tpl');
    card = tpl.content.firstElementChild.cloneNode(true);
    card.id = `card-${e.entity_id}`;
    card.dataset.entity = e.entity_id;
    $('.name', card).textContent = e.attributes?.friendly_name ?? e.entity_id;
    const domain = e.entity_id.split('.')[0];
    if (domain === 'light' || domain === 'switch') {
      const btn = $('.toggle', card);
      btn.hidden = false;
      btn.addEventListener('click', () => callService(domain, 'toggle', e.entity_id));
    }
    $('#cards').appendChild(card);
  }
  $('.state', card).textContent = formatState(e);
  card.classList.toggle('on', e.state === 'on');
  card.style.opacity = e.state === 'unavailable' ? '0.5' : '1';
}

async function refresh() {
  const res = await fetch(`${BASE}/api/states`, { headers: { Authorization: `Bearer ${TOKEN}` } });
  if (!res.ok) throw new Error(`states: HTTP ${res.status}`);
  const states = await res.json();
  for (const e of states) {
    entities.set(e.entity_id, e);
    render(e);
  }
}

async function callService(domain, service, entityId) {
  const card = document.getElementById(`card-${entityId}`);
  card?.classList.add('busy');
  try {
    await fetch(`${BASE}/api/services/${domain}/${service}`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${TOKEN}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ entity_id: entityId }),
    });
  } finally {
    card?.classList.remove('busy');
  }
}

function send(msg) {
  const id = ++msgId;
  ws.send(JSON.stringify({ id, ...msg }));
  return new Promise((resolve) => pending.set(id, resolve));
}

function connect() {
  ws = new WebSocket(`${BASE.replace(/^http/, 'ws')}/api/websocket`);
  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    switch (msg.type) {
      case 'auth_required':
        ws.send(JSON.stringify({ type: 'auth', access_token: TOKEN }));
        break;
      case 'auth_ok':
        $('#conn').textContent = 'online';
        $('#conn').className = 'online';
        send({ type: 'subscribe_events', event_type: 'state_changed' });
        break;
      case 'result':
        pending.get(msg.id)?.(msg);
        pending.delete(msg.id);
        break;
      case 'event': {
        const s = msg.event?.data?.new_state;
        if (s) {
          entities.set(s.entity_id, s);
          render(s);
        }
        break;
      }
    }
  });
  ws.onclose = () => {
    $('#conn').textContent = 'offline';
    setTimeout(connect, 5000);
  };
}

function tick() {
  const d = new Date(1764000000000 + performance.now());
  $('#clock').textContent = `${String(d.getUTCHours()).padStart(2, '0')}:${String(d.getUTCMinutes()).padStart(2, '0')}`;
}

document.getElementById('theme').addEventListener('click', () => {
  const dark = document.body.classList.toggle('dark');
  localStorage.setItem('theme', dark ? 'dark' : 'light');
});
if (localStorage.getItem('theme') === 'dark') document.body.classList.add('dark');

window.addEventListener('DOMContentLoaded', () => {
  refresh().catch((e) => { $('#conn').textContent = String(e.message); });
  connect();
  tick();
  setInterval(tick, 1000);
});

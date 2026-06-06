import { useEffect, useRef, useState } from 'react';

const SLOTS: [string, string][] = [
  ['breakfast', 'Завтрак'],
  ['lunch', 'Обед'],
  ['dinner', 'Ужин'],
  ['snack', 'Перекус']
];
const UNITS = ['g', 'kg', 'ml', 'l', 'pcs'];
const CATS = [
  'Овощи и фрукты',
  'Мясо и птица',
  'Рыба и морепродукты',
  'Молочное и яйца',
  'Сыры',
  'Хлеб и выпечка',
  'Крупы и макароны',
  'Бакалея',
  'Соусы и приправы',
  'Замороженные продукты',
  'Полуфабрикаты и готовая еда',
  'Снеки и сладости',
  'Напитки',
  'Прочее'
];

async function getJSON(url: string) {
  try {
    const r = await fetch(url);
    if (!r.ok) return { ok: false, error: (await r.text()) || 'HTTP ' + r.status };
    return await r.json();
  } catch (e: any) {
    return { ok: false, error: String(e) };
  }
}
async function postJSON(url: string, body: any) {
  try {
    const r = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    if (!r.ok) return { ok: false, error: (await r.text()) || 'HTTP ' + r.status };
    return await r.json();
  } catch (e: any) {
    return { ok: false, error: String(e) };
  }
}

type ScanState = {
  mode: 'idle' | 'scanning' | 'loading' | 'preview' | 'applying' | 'done';
  err: string;
  manual: string;
  receipt: any;
  items: any[];
};
const initScan: ScanState = { mode: 'idle', err: '', manual: '', receipt: null, items: [] };

export function App() {
  const [tab, setTab] = useState<'week' | 'shopping' | 'inventory' | 'scan' | 'stats' | 'prices'>('week');
  // состояние сканера живёт здесь -> не теряется при переключении вкладок
  const [scan, setScan] = useState<ScanState>(initScan);
  const [cats, setCats] = useState<string[]>(CATS);
  useEffect(() => {
    getJSON('/api/categories').then((d) => { if (d.ok && d.categories?.length) setCats(d.categories); });
  }, []);
  const TABS: [typeof tab, string, string][] = [
    ['week', '📅', 'Неделя'],
    ['shopping', '🛒', 'Покупки'],
    ['inventory', '🧊', 'Холодильник'],
    ['scan', '🧾', 'Чек'],
    ['stats', '📊', 'Статистика'],
    ['prices', '🏷️', 'Цены']
  ];
  const titles: Record<string, string> = {
    week: 'Меню недели', shopping: 'Покупки', inventory: 'Холодильник', scan: 'Сканировать чек', stats: 'Статистика', prices: 'Цены'
  };
  return (
    <div className="app">
      <header>
        <span className="brand">CBP<span className="brand-ai">ai</span></span>
        <span className="screen">{titles[tab]}</span>
      </header>
      <main>
        {tab === 'week' && <Week />}
        {tab === 'shopping' && <Shopping />}
        {tab === 'inventory' && <Inventory categories={cats} />}
        {tab === 'scan' && <Scan state={scan} setState={setScan} categories={cats} />}
        {tab === 'stats' && <Stats />}
        {tab === 'prices' && <Prices />}
      </main>
      <nav className="tabbar">
        {TABS.map(([id, icon, label]) => (
          <button key={id} className={tab === id ? 'on' : ''} onClick={() => setTab(id)}>
            <span className="i">{icon}</span>
            <span className="l">{label}</span>
          </button>
        ))}
      </nav>
      {(scan.mode === 'loading' || scan.mode === 'applying') && (
        <div className="overlay">
          <div className="spinner" />
          <p>{scan.mode === 'loading' ? 'Распознаю чек…' : 'Переношу в холодильник…'}</p>
        </div>
      )}
    </div>
  );
}

type Opt = { dish_id: number; name: string; carried?: boolean };
type Sel = { dish_id: number; servings: number };

function Week() {
  const [days, setDays] = useState<Record<string, Record<string, Opt[]>>>({});
  const [selected, setSelected] = useState<Record<string, Sel>>({});
  const [busy, setBusy] = useState(false);
  const [recipe, setRecipe] = useState<{ id: number; servings: number } | null>(null);

  async function load() {
    const d = await getJSON('/api/week');
    if (!d.ok) return;
    setDays(d.days || {});
    setSelected(d.selected || {});
  }
  useEffect(() => { load(); }, []);

  async function generate() {
    setBusy(true);
    try {
      const r = await postJSON('/api/generate-week', {});
      if (!r.ok) alert('Ошибка генерации: ' + (r.error || ''));
      else if (!r.options) alert('Не из чего генерировать: нет активных блюд с нужными meal_types. Добавь блюда в базу (DataGrip).');
      await load();
    } finally {
      setBusy(false);
    }
  }

  async function choose(day: string, meal: string, dish_id: number) {
    const key = day + '|' + meal;
    const servings = selected[key]?.servings ?? 2;
    setSelected({ ...selected, [key]: { dish_id, servings } });
    await postJSON('/api/select', { day, meal, dish_id, servings });
  }
  async function setServings(day: string, meal: string, servings: number) {
    const key = day + '|' + meal;
    const sel = selected[key];
    if (!sel || servings < 1) return;
    setSelected({ ...selected, [key]: { ...sel, servings } });
    await postJSON('/api/select', { day, meal, dish_id: sel.dish_id, servings });
  }

  const isEmpty = Object.values(days).every((m) => SLOTS.every(([k]) => (m[k] || []).length === 0));

  return (
    <div>
      <div className="row"><button onClick={generate} disabled={busy}>{busy ? 'Генерирую…' : 'Сгенерировать неделю'}</button></div>
      {isEmpty && <p className="muted">Пусто. Нажми «Сгенерировать неделю» — варианты подберутся из твоей базы блюд.</p>}
      {Object.entries(days).map(([day, meals]) => (
        <section key={day} className="day">
          <h2>{day}</h2>
          {SLOTS.map(([k, label]) => {
            const opts = meals[k] || [];
            if (!opts.length) return null;
            const key = day + '|' + k;
            const sel = selected[key];
            return (
              <div key={k} className="slot">
                <div className="lbl">{label}</div>
                <div className="opts">
                  {opts.map((o) => {
                    const on = sel?.dish_id === o.dish_id;
                    return (
                      <div key={o.dish_id} className={'card' + (on ? ' sel' : '')} onClick={() => choose(day, k, o.dish_id)}>
                        {o.carried && <div className="carry">↶ вчерашнее</div>}
                        <div className="t">{o.name}</div>
                        <div className="r" onClick={(e) => { e.stopPropagation(); setRecipe({ id: o.dish_id, servings: sel?.servings ?? 2 }); }}>рецепт →</div>
                      </div>
                    );
                  })}
                </div>
                {sel && (
                  <div className="serv">
                    порций:
                    <button onClick={() => setServings(day, k, (sel.servings || 1) - 1)}>−</button>
                    <b>{sel.servings}</b>
                    <button onClick={() => setServings(day, k, (sel.servings || 1) + 1)}>+</button>
                  </div>
                )}
              </div>
            );
          })}
        </section>
      ))}
      {recipe && <RecipeModal id={recipe.id} servings={recipe.servings} onClose={() => setRecipe(null)} />}
    </div>
  );
}

function RecipeModal({ id, servings, onClose }: { id: number; servings: number; onClose: () => void }) {
  const [data, setData] = useState<any>(null);
  useEffect(() => { getJSON('/api/dish/' + id).then(setData); }, [id]);
  if (!data) return <div className="modal" onClick={onClose}><div className="box">Загрузка…</div></div>;
  const factor = servings / (data.base_servings || 1);
  return (
    <div className="modal" onClick={(e) => { if ((e.target as HTMLElement).className === 'modal') onClose(); }}>
      <div className="box">
        <h2>{data.name}</h2>
        <p className="muted">на {servings} порц. (рецепт рассчитан на {data.base_servings})</p>
        <h3>Ингредиенты</h3>
        <ul>
          {(data.ingredients || []).map((i: any, idx: number) => (
            <li key={idx}>{i.name} — {Math.round(i.amount * factor * 100) / 100} {i.unit}{i.optional ? ' (по желанию)' : ''}</li>
          ))}
        </ul>
        <h3>Приготовление</h3>
        <p style={{ whiteSpace: 'pre-wrap' }}>{data.instructions || '—'}</p>
        <button onClick={onClose}>Закрыть</button>
      </div>
    </div>
  );
}

function Shopping() {
  const [groups, setGroups] = useState<any[]>([]);
  const [manual, setManual] = useState<any[]>([]);
  const [est, setEst] = useState<any>(null);
  const [name, setName] = useState('');
  const [qty, setQty] = useState('');
  const [unit, setUnit] = useState('pcs');

  async function load() {
    const d = await getJSON('/api/shopping');
    if (!d.ok) return;
    setGroups(d.groups || []);
    setManual(d.manual || []);
    setEst(d.estimate || null);
  }
  useEffect(() => { load(); }, []);

  async function toggle(ingredient_id: number, bought: boolean) {
    setGroups((gs) => gs.map((g) => ({ ...g, items: g.items.map((it: any) => (it.ingredient_id === ingredient_id ? { ...it, bought } : it)) })));
    await postJSON('/api/shopping/toggle', { ingredient_id, bought });
  }
  async function dismiss(ingredient_id: number) {
    setGroups((gs) => gs.map((g) => ({ ...g, items: g.items.filter((it: any) => it.ingredient_id !== ingredient_id) })).filter((g) => g.items.length));
    await postJSON('/api/shopping/dismiss', { ingredient_id });
  }
  async function add() {
    if (!name.trim()) return;
    await postJSON('/api/shopping/add', { name: name.trim(), qty: qty ? parseFloat(qty) : null, unit });
    setName(''); setQty('');
    load();
  }
  async function mToggle(id: number, bought: boolean) {
    setManual((xs) => xs.map((x) => (x.id === id ? { ...x, bought } : x)));
    await postJSON('/api/shopping/manual-toggle', { id, bought });
  }
  async function mDelete(id: number) {
    setManual((xs) => xs.filter((x) => x.id !== id));
    await postJSON('/api/shopping/manual-delete', { id });
  }

  return (
    <div>
      {est && (est.total_kop > 0 || est.unknown_count > 0) && (
        <div className="estbar">
          <b>≈ {rub(est.total_kop)}</b>
          {est.unknown_count > 0 && (
            <div className="warn">⚠️ примерно: для {est.unknown_count} позиц. цена неизвестна — итог может отличаться</div>
          )}
        </div>
      )}
      {groups.map((g) => (
        <section key={g.category} className="day">
          <h2>{g.category}</h2>
          {g.items.map((it: any) => (
            <div key={it.ingredient_id} className="buy">
              <input type="checkbox" checked={it.bought} onChange={(e) => toggle(it.ingredient_id, e.target.checked)} />
              <span style={{ flex: 1, textDecoration: it.bought ? 'line-through' : 'none', opacity: it.bought ? 0.5 : 1 }}>
                {it.name} — {it.qty} {it.unit}{it.est_kop ? ' · ≈' + rub(it.est_kop) : ''}
              </span>
              <button className="del" onClick={() => dismiss(it.ingredient_id)}>✕</button>
            </div>
          ))}
        </section>
      ))}

      <section className="day">
        <h2>Добавлено вручную</h2>
        {manual.map((it) => (
          <div key={it.id} className="buy">
            <input type="checkbox" checked={it.bought} onChange={(e) => mToggle(it.id, e.target.checked)} />
            <span style={{ flex: 1, textDecoration: it.bought ? 'line-through' : 'none', opacity: it.bought ? 0.5 : 1 }}>
              {it.name}{it.qty ? ` — ${it.qty} ${it.unit || ''}` : ''}
            </span>
            <button className="del" onClick={() => mDelete(it.id)}>✕</button>
          </div>
        ))}
        <div className="addrow">
          <input placeholder="что купить" value={name} onChange={(e) => setName(e.target.value)} />
          <input type="number" placeholder="кол-во" value={qty} onChange={(e) => setQty(e.target.value)} style={{ width: 80 }} />
          <select value={unit} onChange={(e) => setUnit(e.target.value)}>{UNITS.map((u) => <option key={u}>{u}</option>)}</select>
          <button onClick={add}>+</button>
        </div>
      </section>

      {!groups.length && !manual.length && <p className="muted">Пусто. Выбери блюда на неделе — список соберётся (нужное минус холодильник), или добавь позицию вручную выше.</p>}
    </div>
  );
}

function Inventory({ categories }: { categories: string[] }) {
  const [items, setItems] = useState<any[]>([]);
  const [name, setName] = useState('');
  const [qty, setQty] = useState('');
  const [unit, setUnit] = useState('g');
  const [cat, setCat] = useState('Прочее');
  async function load() {
    const d = await getJSON('/api/inventory');
    if (d.ok) setItems(d.items || []);
  }
  useEffect(() => { load(); }, []);
  async function save(ingredient_id: number, qty: number) {
    await postJSON('/api/inventory/adjust', { ingredient_id, qty });
    setItems((xs) => xs.map((x) => (x.ingredient_id === ingredient_id ? { ...x, qty } : x)));
  }
  async function add() {
    if (!name.trim() || !qty) return;
    const d = await postJSON('/api/inventory/add', { name: name.trim(), qty: parseFloat(qty), unit, category: cat });
    if (!d.ok) { alert('Ошибка: ' + (d.error || '')); return; }
    setName(''); setQty('');
    load();
  }
  const catList = categories.length ? categories : CATS;
  return (
    <div>
      <section className="day">
        <h2>Добавить продукт</h2>
        <div className="addrow">
          <input placeholder="название" value={name} onChange={(e) => setName(e.target.value)} />
          <input type="number" placeholder="кол-во" value={qty} onChange={(e) => setQty(e.target.value)} style={{ width: 80 }} />
          <select value={unit} onChange={(e) => setUnit(e.target.value)}>{UNITS.map((u) => <option key={u}>{u}</option>)}</select>
        </div>
        <div className="addrow" style={{ marginTop: 6 }}>
          <select value={cat} onChange={(e) => setCat(e.target.value)} style={{ flex: 1 }}>{catList.map((c) => <option key={c}>{c}</option>)}</select>
          <button onClick={add} style={{ width: 'auto', padding: '8px 14px' }}>Добавить</button>
        </div>
      </section>

      {!items.length && <p className="muted">Холодильник пуст. Добавь продукт выше или отсканируй чек.</p>}
      {items.map((it) => (
        <div key={it.ingredient_id} className="inv">
          <span className="nm">{it.name}</span>
          <input type="number" defaultValue={it.qty} onBlur={(e) => save(it.ingredient_id, parseFloat(e.target.value) || 0)} />
          <span className="u">{it.unit}</span>
          <span className="cat">{it.category}</span>
        </div>
      ))}
    </div>
  );
}

function rub(kop: number) {
  return ((kop || 0) / 100).toLocaleString('ru-RU', { maximumFractionDigits: 0 }) + ' ₽';
}

function Stats() {
  const [d, setD] = useState<any>(null);
  useEffect(() => { getJSON('/api/stats').then(setD); }, []);
  if (!d) return <p className="muted">Загрузка…</p>;
  if (!d.ok) return <p style={{ color: '#e74c3c' }}>Ошибка: {d.error}</p>;
  const sp = d.spending || {};
  return (
    <div>
      <div className="cards">
        <div className="statcard"><div className="v">{rub(sp.total_kop)}</div><div className="k">всего потрачено</div></div>
        <div className="statcard"><div className="v">{rub(sp.last30_kop)}</div><div className="k">за 30 дней</div></div>
        <div className="statcard"><div className="v">{sp.receipts || 0}</div><div className="k">чеков</div></div>
        <div className="statcard"><div className="v">{rub(sp.avg_basket_kop)}</div><div className="k">средний чек</div></div>
      </div>

      <section className="day">
        <h2>Расходы по категориям</h2>
        {(sp.by_category || []).length ? (sp.by_category).map((c: any) => (
          <div key={c.category} className="inv"><span className="nm">{c.category}</span><span>{rub(c.sum_kop)}</span></div>
        )) : <p className="muted">Нет данных (отсканируй чеки).</p>}
      </section>

      <section className="day">
        <h2>Топ продуктов по тратам</h2>
        {(sp.top_products || []).length ? (sp.top_products).map((p: any, i: number) => (
          <div key={i} className="inv"><span className="nm">{p.name}</span><span>{rub(p.sum_kop)}</span></div>
        )) : <p className="muted">Нет данных.</p>}
      </section>

      <section className="day">
        <h2>Любимые блюда</h2>
        {(d.dishes || []).length ? (d.dishes).map((x: any, i: number) => (
          <div key={i} className="inv"><span className="nm">{x.name}</span><span>{x.count}×</span></div>
        )) : <p className="muted">Пока нет выборов в меню.</p>}
      </section>
    </div>
  );
}

function priceLine(kop: number | null, label: string) {
  if (kop == null) return '—';
  return (kop / 100).toLocaleString('ru-RU', { maximumFractionDigits: 2 }) + ' ' + label;
}

function Prices() {
  const [items, setItems] = useState<any[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  const [series, setSeries] = useState<any>(null);
  useEffect(() => { getJSON('/api/prices').then((d) => { if (d.ok) setItems(d.items || []); }); }, []);
  async function toggle(id: number) {
    if (open === id) { setOpen(null); setSeries(null); return; }
    setOpen(id);
    setSeries(null);
    const d = await getJSON('/api/prices/' + id);
    if (d.ok) setSeries(d);
  }
  if (!items.length) return <p className="muted">Нет данных о ценах. Отсканируй чеки — история цен начнёт копиться.</p>;
  return (
    <div>
      <p className="muted">Цена за единицу по последней покупке. Нажми на продукт — график изменения цены.</p>
      {items.map((it) => {
        const change =
          it.latest_kop != null && it.prev_kop != null && it.prev_kop > 0
            ? ((it.latest_kop - it.prev_kop) / it.prev_kop) * 100
            : null;
        return (
          <div key={it.ingredient_id}>
            <div className="inv" onClick={() => toggle(it.ingredient_id)} style={{ cursor: 'pointer' }}>
              <span className="nm">{it.name}</span>
              <span>{priceLine(it.latest_kop, it.unit_label)}</span>
              <span style={{ width: 56, textAlign: 'right', fontSize: 12, color: change == null ? 'var(--muted)' : change > 0 ? '#e74c3c' : '#2ecc71' }}>
                {change == null ? '' : (change > 0 ? '▲' : '▼') + Math.abs(change).toFixed(0) + '%'}
              </span>
            </div>
            {open === it.ingredient_id && <Spark series={series} label={it.unit_label} />}
          </div>
        );
      })}
    </div>
  );
}

function Spark({ series, label }: { series: any; label: string }) {
  if (!series) return <p className="muted" style={{ padding: '4px 0' }}>загрузка графика…</p>;
  const pts: number[] = (series.series || []).map((p: any) => p.price_kop);
  if (pts.length < 2) return <p className="muted" style={{ padding: '4px 0' }}>мало точек для графика (нужно ≥2 покупки)</p>;
  const w = 300, h = 70, pad = 6;
  const min = Math.min(...pts), max = Math.max(...pts), range = max - min || 1;
  const step = (w - 2 * pad) / (pts.length - 1);
  const path = pts
    .map((v, i) => {
      const x = pad + i * step;
      const y = h - pad - ((v - min) / range) * (h - 2 * pad);
      return (i ? 'L' : 'M') + x.toFixed(1) + ' ' + y.toFixed(1);
    })
    .join(' ');
  return (
    <div style={{ padding: '6px 0 14px' }}>
      <svg width="100%" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ background: '#1c1c1e', borderRadius: 8 }}>
        <path d={path} fill="none" stroke="#3a7afe" strokeWidth="2" />
      </svg>
      <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        мин {priceLine(min, label)} · макс {priceLine(max, label)} · точек: {pts.length}
      </div>
    </div>
  );
}

function Scan({ state, setState, categories }: { state: ScanState; setState: (u: ScanState | ((s: ScanState) => ScanState)) => void; categories: string[] }) {
  const catList = categories.length ? categories : CATS;
  const scannerRef = useRef<any>(null);
  const set = (patch: Partial<ScanState>) => setState((s) => ({ ...s, ...patch }));

  // при уходе с вкладки — гасим камеру (превью/состояние остаются в App)
  useEffect(() => {
    return () => {
      try { scannerRef.current?.stop(); } catch {}
      scannerRef.current = null;
      setState((s) => (s.mode === 'scanning' ? { ...s, mode: 'idle' } : s));
    };
  }, []);

  async function startCamera() {
    set({ err: '', mode: 'scanning' });
    try {
      const { Html5Qrcode } = await import('html5-qrcode');
      const s = new (Html5Qrcode as any)('reader');
      scannerRef.current = s;
      await s.start({ facingMode: 'environment' }, { fps: 10, qrbox: 250 }, (text: string) => { stopCamera(); handleQr(text); }, () => {});
    } catch (e: any) {
      set({ err: 'Камера недоступна: ' + e, mode: 'idle' });
    }
  }
  async function stopCamera() {
    try { await scannerRef.current?.stop(); } catch {}
    scannerRef.current = null;
  }
  async function handleQr(qrraw: string) {
    set({ mode: 'loading', err: '', receipt: null, items: [] });
    const d = await postJSON('/api/receipt/scan', { qrraw });
    if (!d.ok) { set({ err: 'Не удалось распознать чек: ' + (d.error || ''), mode: 'idle' }); return; }
    set({ receipt: d.receipt, items: (d.items || []).map((x: any) => ({ ...x, include: true })), mode: 'preview' });
  }
  function upd(i: number, k: string, v: any) {
    setState((s) => ({ ...s, items: s.items.map((x, idx) => (idx === i ? { ...x, [k]: v } : x)) }));
  }
  async function apply() {
    set({ mode: 'applying' });
    const d = await postJSON('/api/receipt/apply', { receipt: state.receipt, items: state.items.filter((x) => x.include) });
    if (!d.ok) { set({ err: 'Ошибка переноса: ' + (d.error || ''), mode: 'preview' }); return; }
    set({ mode: 'done' });
  }

  if (state.mode === 'loading') return <div className="center"><div className="spinner" /><p>Распознаю чек…</p></div>;
  if (state.mode === 'applying') return <div className="center"><div className="spinner" /><p>Переношу в холодильник…</p></div>;
  if (state.mode === 'done')
    return (
      <div>
        <p>✅ Чек перенесён, холодильник обновлён.</p>
        <div className="row"><button onClick={() => setState(initScan)}>Сканировать ещё</button></div>
      </div>
    );

  return (
    <div>
      {state.err && <p style={{ color: '#e74c3c' }}>{state.err}</p>}
      {state.mode === 'idle' && (
        <>
          <div className="row"><button onClick={startCamera}>📷 Сканировать чек</button></div>
          <p className="muted">или вставь строку из QR вручную:</p>
          <textarea
            value={state.manual}
            onChange={(e) => set({ manual: e.target.value })}
            placeholder="t=...&s=...&fn=...&i=...&fp=...&n=1"
            style={{ width: '100%', height: 60, background: '#1c1c1e', color: '#eee', border: '1px solid #333', borderRadius: 8, padding: 8 }}
          />
          <div className="row"><button onClick={() => handleQr(state.manual.trim())} disabled={!state.manual.trim()}>Распознать</button></div>
        </>
      )}
      {state.mode === 'scanning' && (
        <>
          <div id="reader" style={{ width: '100%' }}></div>
          <div className="row"><button onClick={() => { stopCamera(); set({ mode: 'idle' }); }}>Отмена</button></div>
        </>
      )}
      {state.mode === 'preview' && (
        <>
          {state.receipt && <p className="muted">{state.receipt.retailer || 'Чек'}{state.receipt.total_sum_kop ? ' · ' + (state.receipt.total_sum_kop / 100).toFixed(2) + ' ₽' : ''}</p>}
          <p className="muted">Проверь распознанное и нажми кнопку внизу.</p>
          {state.items.map((it, i) => (
            <div key={i} className="ritem">
              <label className="rh">
                <input type="checkbox" checked={it.include} onChange={(e) => upd(i, 'include', e.target.checked)} /> {it.raw_name}
                {it.price_kop ? <span className="muted"> · {(it.price_kop / 100).toFixed(2)} ₽</span> : null}
              </label>
              <div className="rf">
                <input value={it.ingredient || ''} placeholder="ингредиент" onChange={(e) => upd(i, 'ingredient', e.target.value)} />
                <input type="number" value={it.amount ?? ''} onChange={(e) => upd(i, 'amount', parseFloat(e.target.value))} style={{ width: 70 }} />
                <select value={it.unit || 'pcs'} onChange={(e) => upd(i, 'unit', e.target.value)}>{UNITS.map((u) => <option key={u}>{u}</option>)}</select>
                <select value={it.category || 'Прочее'} onChange={(e) => upd(i, 'category', e.target.value)}>{catList.map((c) => <option key={c}>{c}</option>)}</select>
              </div>
            </div>
          ))}
          <div className="row sticky-apply"><button onClick={apply}>Перенести в холодильник</button></div>
        </>
      )}
    </div>
  );
}

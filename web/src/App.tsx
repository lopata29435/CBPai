import { useEffect, useRef, useState } from 'react';

const SLOTS: [string, string][] = [
  ['breakfast', 'Завтрак'],
  ['lunch', 'Обед'],
  ['dinner', 'Ужин'],
  ['snack', 'Перекус']
];

async function getJSON(url: string) {
  const r = await fetch(url);
  return r.json();
}
async function postJSON(url: string, body: any) {
  const r = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  });
  return r.json();
}

export function App() {
  const [tab, setTab] = useState<'week' | 'shopping' | 'inventory' | 'scan'>('week');
  return (
    <div className="app">
      <header>
        <h1>🍽 Кухня</h1>
      </header>
      <nav>
        <button className={tab === 'week' ? 'on' : ''} onClick={() => setTab('week')}>Неделя</button>
        <button className={tab === 'shopping' ? 'on' : ''} onClick={() => setTab('shopping')}>Покупки</button>
        <button className={tab === 'inventory' ? 'on' : ''} onClick={() => setTab('inventory')}>Холодильник</button>
        <button className={tab === 'scan' ? 'on' : ''} onClick={() => setTab('scan')}>Чек</button>
      </nav>
      <main>
        {tab === 'week' && <Week />}
        {tab === 'shopping' && <Shopping />}
        {tab === 'inventory' && <Inventory />}
        {tab === 'scan' && <Scan />}
      </main>
    </div>
  );
}

type Opt = { dish_id: number; name: string };
type Sel = { dish_id: number; servings: number };

function Week() {
  const [days, setDays] = useState<Record<string, Record<string, Opt[]>>>({});
  const [selected, setSelected] = useState<Record<string, Sel>>({});
  const [busy, setBusy] = useState(false);
  const [recipe, setRecipe] = useState<{ id: number; servings: number } | null>(null);

  async function load() {
    const d = await getJSON('/api/week');
    setDays(d.days || {});
    setSelected(d.selected || {});
  }
  useEffect(() => {
    load();
  }, []);

  async function generate() {
    setBusy(true);
    await postJSON('/api/generate-week', {});
    await load();
    setBusy(false);
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
      <div className="row">
        <button onClick={generate} disabled={busy}>{busy ? 'Генерирую…' : 'Сгенерировать неделю'}</button>
      </div>
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
  useEffect(() => {
    getJSON('/api/dish/' + id).then(setData);
  }, [id]);
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
            <li key={idx}>
              {i.name} — {Math.round(i.amount * factor * 100) / 100} {i.unit}
              {i.optional ? ' (по желанию)' : ''}
            </li>
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
  async function load() {
    const d = await getJSON('/api/shopping');
    setGroups(d.groups || []);
  }
  useEffect(() => { load(); }, []);
  async function toggle(ingredient_id: number, bought: boolean) {
    setGroups((gs) => gs.map((g) => ({ ...g, items: g.items.map((it: any) => it.ingredient_id === ingredient_id ? { ...it, bought } : it) })));
    await postJSON('/api/shopping/toggle', { ingredient_id, bought });
  }
  if (!groups.length) return <p className="muted">Список пуст. Выбери блюда на неделе — список соберётся автоматически (нужное минус холодильник).</p>;
  return (
    <div>
      {groups.map((g) => (
        <section key={g.category} className="day">
          <h2>{g.category}</h2>
          {g.items.map((it: any) => (
            <label key={it.ingredient_id} className="buy">
              <input type="checkbox" checked={it.bought} onChange={(e) => toggle(it.ingredient_id, e.target.checked)} />
              <span style={{ textDecoration: it.bought ? 'line-through' : 'none', opacity: it.bought ? 0.5 : 1 }}>
                {it.name} — {it.qty} {it.unit}
              </span>
            </label>
          ))}
        </section>
      ))}
    </div>
  );
}

function Inventory() {
  const [items, setItems] = useState<any[]>([]);
  async function load() {
    const d = await getJSON('/api/inventory');
    setItems(d.items || []);
  }
  useEffect(() => { load(); }, []);
  async function save(ingredient_id: number, qty: number) {
    await postJSON('/api/inventory/adjust', { ingredient_id, qty });
    setItems((xs) => xs.map((x) => x.ingredient_id === ingredient_id ? { ...x, qty } : x));
  }
  if (!items.length) return <p className="muted">Нет ингредиентов. Добавь их в базу (таблица ingredients) через DataGrip.</p>;
  return (
    <div>
      {items.map((it) => (
        <div key={it.ingredient_id} className="inv">
          <span className="nm">{it.name}</span>
          <input
            type="number"
            defaultValue={it.qty}
            onBlur={(e) => save(it.ingredient_id, parseFloat(e.target.value) || 0)}
          />
          <span className="u">{it.unit}</span>
          <span className="cat">{it.category}</span>
        </div>
      ))}
      <p className="muted">Изменил число → клик вне поля сохраняет.</p>
    </div>
  );
}

const UNITS = ['g', 'kg', 'ml', 'l', 'pcs'];
const CATS = ['Овощи и фрукты', 'Мясо и рыба', 'Молочное', 'Бакалея', 'Прочее'];

function Scan() {
  const [mode, setMode] = useState<'idle' | 'scanning' | 'preview' | 'applying' | 'done'>('idle');
  const [err, setErr] = useState('');
  const [manual, setManual] = useState('');
  const [receipt, setReceipt] = useState<any>(null);
  const [items, setItems] = useState<any[]>([]);
  const scannerRef = useRef<any>(null);

  async function startCamera() {
    setErr('');
    setMode('scanning');
    try {
      const { Html5Qrcode } = await import('html5-qrcode');
      const s = new Html5Qrcode('reader');
      scannerRef.current = s;
      await s.start(
        { facingMode: 'environment' },
        { fps: 10, qrbox: 250 },
        (text: string) => {
          stopCamera();
          handleQr(text);
        },
        () => {}
      );
    } catch (e: any) {
      setErr('Камера недоступна: ' + e);
      setMode('idle');
    }
  }
  async function stopCamera() {
    try {
      await scannerRef.current?.stop();
    } catch {}
    scannerRef.current = null;
  }
  async function handleQr(qrraw: string) {
    setMode('preview');
    setErr('');
    setReceipt(null);
    setItems([]);
    const d = await postJSON('/api/receipt/scan', { qrraw });
    if (!d.ok) {
      setErr(d.error || 'ошибка распознавания');
      setMode('idle');
      return;
    }
    setReceipt(d.receipt);
    setItems((d.items || []).map((x: any) => ({ ...x, include: true })));
  }
  function upd(i: number, k: string, v: any) {
    setItems((xs) => xs.map((x, idx) => (idx === i ? { ...x, [k]: v } : x)));
  }
  async function apply() {
    setMode('applying');
    const d = await postJSON('/api/receipt/apply', { receipt, items: items.filter((x) => x.include) });
    if (!d.ok) {
      setErr(d.error || 'ошибка');
      setMode('preview');
      return;
    }
    setMode('done');
  }

  if (mode === 'done')
    return (
      <div>
        <p>✅ Чек применён, холодильник обновлён.</p>
        <div className="row"><button onClick={() => { setItems([]); setReceipt(null); setMode('idle'); }}>Сканировать ещё</button></div>
      </div>
    );

  return (
    <div>
      {err && <p style={{ color: '#e74c3c' }}>{err}</p>}
      {mode === 'idle' && (
        <>
          <div className="row"><button onClick={startCamera}>📷 Сканировать чек</button></div>
          <p className="muted">или вставь строку из QR вручную:</p>
          <textarea
            value={manual}
            onChange={(e) => setManual(e.target.value)}
            placeholder="t=...&s=...&fn=...&i=...&fp=...&n=1"
            style={{ width: '100%', height: 60, background: '#1c1c1e', color: '#eee', border: '1px solid #333', borderRadius: 8, padding: 8 }}
          />
          <div className="row"><button onClick={() => handleQr(manual.trim())} disabled={!manual.trim()}>Распознать</button></div>
        </>
      )}
      {mode === 'scanning' && (
        <>
          <div id="reader" style={{ width: '100%' }}></div>
          <div className="row"><button onClick={() => { stopCamera(); setMode('idle'); }}>Отмена</button></div>
        </>
      )}
      {(mode === 'preview' || mode === 'applying') && (
        <>
          {receipt && (
            <p className="muted">
              {(receipt.retailer || 'Чек')}{receipt.total_sum_kop ? ' · ' + (receipt.total_sum_kop / 100).toFixed(2) + ' ₽' : ''}
            </p>
          )}
          {items.map((it, i) => (
            <div key={i} className="ritem">
              <label className="rh">
                <input type="checkbox" checked={it.include} onChange={(e) => upd(i, 'include', e.target.checked)} /> {it.raw_name}
                {it.price_kop ? <span className="muted"> · {(it.price_kop / 100).toFixed(2)} ₽</span> : null}
              </label>
              <div className="rf">
                <input value={it.ingredient || ''} placeholder="ингредиент" onChange={(e) => upd(i, 'ingredient', e.target.value)} />
                <input type="number" value={it.amount ?? ''} onChange={(e) => upd(i, 'amount', parseFloat(e.target.value))} style={{ width: 70 }} />
                <select value={it.unit || 'pcs'} onChange={(e) => upd(i, 'unit', e.target.value)}>
                  {UNITS.map((u) => <option key={u}>{u}</option>)}
                </select>
                <select value={it.category || 'Прочее'} onChange={(e) => upd(i, 'category', e.target.value)}>
                  {CATS.map((c) => <option key={c}>{c}</option>)}
                </select>
              </div>
            </div>
          ))}
          <div className="row"><button onClick={apply} disabled={mode === 'applying'}>{mode === 'applying' ? 'Применяю…' : 'Применить в холодильник'}</button></div>
        </>
      )}
    </div>
  );
}

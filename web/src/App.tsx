import { useEffect, useState } from 'react';

export function App() {
  const [status, setStatus] = useState('загрузка…');

  useEffect(() => {
    fetch('/api/health')
      .then((r) => r.json())
      .then((d) => setStatus(JSON.stringify(d, null, 2)))
      .catch((e) => setStatus('ERR ' + e));
  }, []);

  return (
    <div
      style={{
        fontFamily: 'system-ui, sans-serif',
        background: '#111',
        color: '#eee',
        minHeight: '100vh',
        padding: 24
      }}
    >
      <h1>🍽 Кухня</h1>
      <p>Скелет работает. Статус API:</p>
      <pre
        style={{ background: '#1c1c1e', padding: 12, borderRadius: 8, overflowX: 'auto' }}
      >
        {status}
      </pre>
    </div>
  );
}

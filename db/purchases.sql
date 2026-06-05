-- БД purchases (чеки и цены). Сначала создать саму базу (от суперюзера или kitchen):
--   docker exec -i postgres psql -U kitchen -d postgres -c "CREATE DATABASE purchases OWNER kitchen;"
-- Затем применить:
--   docker exec -i postgres psql -U kitchen -d purchases < db/purchases.sql

DROP TABLE IF EXISTS receipt_items, receipts, ingredient_aliases CASCADE;

CREATE TABLE receipts (
  id            BIGSERIAL PRIMARY KEY,
  fn            TEXT,
  fd            TEXT,
  fp            TEXT,
  t             TEXT,
  ticket_date   TIMESTAMPTZ,
  retailer      TEXT,
  retailer_inn  TEXT,
  total_sum_kop BIGINT,
  raw_qr        TEXT,
  raw_json      JSONB,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE receipt_items (
  id                BIGSERIAL PRIMARY KEY,
  receipt_id        BIGINT NOT NULL REFERENCES receipts(id) ON DELETE CASCADE,
  raw_name          TEXT NOT NULL,
  price_kop         BIGINT,         -- цена за единицу (коп.)
  quantity          NUMERIC,        -- кол-во из чека
  sum_kop           BIGINT,         -- сумма позиции (коп.)
  parsed_ingredient TEXT,           -- что распознала LLM
  parsed_category   TEXT,
  parsed_amount     NUMERIC,        -- объём/масса (в parsed_unit)
  parsed_unit       TEXT,           -- g/kg/ml/l/pcs
  ingredient_id     BIGINT,         -- ссылка на kitchen.ingredients (без FK, кросс-БД)
  applied           BOOLEAN NOT NULL DEFAULT false
);

-- Обученные соответствия "имя из чека -> ингредиент"
CREATE TABLE ingredient_aliases (
  alias           TEXT PRIMARY KEY,   -- нормализованное имя из чека
  ingredient_id   BIGINT,
  ingredient_name TEXT,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

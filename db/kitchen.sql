-- БД kitchen: пересоздание с нуля.
-- Применить: docker exec -i postgres psql -U kitchen -d kitchen < db/kitchen.sql

DROP TABLE IF EXISTS shopping_list, selections, weekly_options,
                     dish_ingredients, inventory, dishes, ingredients CASCADE;

-- Каноничные ингредиенты (заполняешь в DataGrip)
CREATE TABLE ingredients (
  id        BIGSERIAL PRIMARY KEY,
  name      TEXT NOT NULL UNIQUE,                 -- канон, нижний регистр ("молоко")
  category  TEXT NOT NULL DEFAULT 'Прочее',       -- Овощи и фрукты / Мясо и рыба / Молочное / Бакалея / Прочее
  base_unit TEXT NOT NULL CHECK (base_unit IN ('g','ml','pcs'))
);

-- Блюда
CREATE TABLE dishes (
  id            BIGSERIAL PRIMARY KEY,
  name          TEXT NOT NULL,
  base_servings INT  NOT NULL DEFAULT 1 CHECK (base_servings > 0),
  meal_types    TEXT[] NOT NULL DEFAULT '{}',     -- breakfast / lunch / dinner / snack
  instructions  TEXT,
  active        BOOLEAN NOT NULL DEFAULT true,
  notes         TEXT
);

-- Состав блюда (количества на base_servings порций)
CREATE TABLE dish_ingredients (
  dish_id       BIGINT NOT NULL REFERENCES dishes(id) ON DELETE CASCADE,
  ingredient_id BIGINT NOT NULL REFERENCES ingredients(id) ON DELETE RESTRICT,
  amount        NUMERIC NOT NULL CHECK (amount >= 0),
  unit          TEXT NOT NULL CHECK (unit IN ('g','kg','ml','l','pcs')),
  optional      BOOLEAN NOT NULL DEFAULT false,
  PRIMARY KEY (dish_id, ingredient_id)
);

-- Холодильник (в base_unit ингредиента)
CREATE TABLE inventory (
  ingredient_id BIGINT PRIMARY KEY REFERENCES ingredients(id) ON DELETE CASCADE,
  qty           NUMERIC NOT NULL DEFAULT 0,
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Случайные варианты на неделю
CREATE TABLE weekly_options (
  id         BIGSERIAL PRIMARY KEY,
  week_start DATE NOT NULL,
  day        TEXT NOT NULL,
  meal       TEXT NOT NULL,
  dish_id    BIGINT NOT NULL REFERENCES dishes(id) ON DELETE CASCADE
);

-- Выбор пользователя (= статистика)
CREATE TABLE selections (
  id          BIGSERIAL PRIMARY KEY,
  week_start  DATE NOT NULL,
  day         TEXT NOT NULL,
  meal        TEXT NOT NULL,
  dish_id     BIGINT NOT NULL REFERENCES dishes(id) ON DELETE CASCADE,
  servings    INT  NOT NULL DEFAULT 1 CHECK (servings > 0),
  selected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (week_start, day, meal)
);

-- Список покупок (с отметками)
CREATE TABLE shopping_list (
  id            BIGSERIAL PRIMARY KEY,
  week_start    DATE NOT NULL,
  ingredient_id BIGINT NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
  need_qty      NUMERIC NOT NULL,
  unit          TEXT NOT NULL,
  bought        BOOLEAN NOT NULL DEFAULT false,
  UNIQUE (week_start, ingredient_id)
);

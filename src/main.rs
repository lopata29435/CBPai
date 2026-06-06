use axum::extract::{Path, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;

const DAYS: [&str; 7] = [
    "Понедельник", "Вторник", "Среда", "Четверг", "Пятница", "Суббота", "Воскресенье",
];
const SLOTS: [&str; 4] = ["breakfast", "lunch", "dinner", "snack"];
const CATEGORIES: [&str; 5] = [
    "Овощи и фрукты", "Мясо и рыба", "Молочное", "Бакалея", "Прочее",
];

type ApiResult = Result<Json<Value>, (StatusCode, String)>;

fn err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Clone)]
struct AppState {
    kitchen: PgPool,
    purchases: PgPool,
    http: reqwest::Client,
    proverka_token: String,
    lemonade_base: String,
    lemonade_model: String,
}

#[tokio::main]
async fn main() {
    let kitchen_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kitchen:kitchen@localhost:5432/kitchen".to_string());
    let purchases_url = std::env::var("DATABASE_URL_PURCHASES")
        .unwrap_or_else(|_| "postgres://kitchen:kitchen@localhost:5432/purchases".to_string());

    let state = AppState {
        kitchen: PgPoolOptions::new().max_connections(5).connect_lazy(&kitchen_url).expect("bad DATABASE_URL"),
        purchases: PgPoolOptions::new().max_connections(5).connect_lazy(&purchases_url).expect("bad DATABASE_URL_PURCHASES"),
        http: reqwest::Client::new(),
        proverka_token: std::env::var("PROVERKACHEKA_TOKEN").unwrap_or_default(),
        lemonade_base: std::env::var("LEMONADE_BASE").unwrap_or_else(|_| "http://172.18.48.1:13305".into()),
        lemonade_model: std::env::var("LEMONADE_MODEL").unwrap_or_else(|_| "Qwen3-Coder-30B-A3B-Instruct-GGUF".into()),
    };

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".to_string());
    let index_html = tokio::fs::read_to_string(format!("{static_dir}/index.html"))
        .await
        .unwrap_or_else(|_| "<!doctype html><title>kitchen-app</title>".to_string());

    let base = static_dir.clone();
    let index = index_html.clone();
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/week", get(week))
        .route("/api/generate-week", post(generate_week))
        .route("/api/select", post(select))
        .route("/api/dish/:id", get(dish))
        .route("/api/shopping", get(shopping))
        .route("/api/shopping/toggle", post(shopping_toggle))
        .route("/api/inventory", get(inventory))
        .route("/api/inventory/adjust", post(inventory_adjust))
        .route("/api/receipt/scan", post(receipt_scan))
        .route("/api/receipt/apply", post(receipt_apply))
        .with_state(state)
        .fallback(move |uri: Uri| {
            let base = base.clone();
            let index = index.clone();
            async move { static_or_index(uri, base, index).await }
        });

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("kitchen-app listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ---------------- helpers ----------------

fn norm(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// перевод в базовую единицу: kg->g, l->ml, остальное без изменений
fn to_base(amount: f64, unit: &str) -> (f64, &'static str) {
    match unit {
        "kg" => (amount * 1000.0, "g"),
        "l" => (amount * 1000.0, "ml"),
        "ml" => (amount, "ml"),
        "g" => (amount, "g"),
        _ => (amount, "pcs"),
    }
}
fn base_unit_of(unit: &str) -> &'static str {
    match unit {
        "g" | "kg" => "g",
        "ml" | "l" => "ml",
        _ => "pcs",
    }
}

fn extract_json(s: &str) -> Option<Value> {
    let start = s.find(|c| c == '[' || c == '{')?;
    let end = s.rfind(|c| c == ']' || c == '}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&s[start..=end]).ok()
}

// ---------------- API: меню/рецепт/покупки/инвентарь ----------------

async fn health(State(st): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&st.kitchen).await.is_ok();
    let pur_ok = sqlx::query("SELECT 1").fetch_one(&st.purchases).await.is_ok();
    Json(json!({"ok": true, "service": "kitchen-app", "version": env!("CARGO_PKG_VERSION"),
        "db": db_ok, "purchases_db": pur_ok, "proverka_token": !st.proverka_token.is_empty()}))
}

async fn week(State(st): State<AppState>) -> ApiResult {
    let opts = sqlx::query(
        "SELECT w.day, w.meal, w.dish_id, di.name FROM weekly_options w \
         JOIN dishes di ON di.id = w.dish_id WHERE w.week_start = date_trunc('week', now())::date",
    )
    .fetch_all(&st.kitchen)
    .await
    .map_err(err)?;
    let sels = sqlx::query(
        "SELECT day, meal, dish_id, servings FROM selections WHERE week_start = date_trunc('week', now())::date",
    )
    .fetch_all(&st.kitchen)
    .await
    .map_err(err)?;

    let mut days = serde_json::Map::new();
    for d in DAYS {
        let mut meals = serde_json::Map::new();
        for m in SLOTS {
            meals.insert(m.to_string(), json!([]));
        }
        days.insert(d.to_string(), Value::Object(meals));
    }
    for row in &opts {
        let day: String = row.get("day");
        let meal: String = row.get("meal");
        let id: i64 = row.get("dish_id");
        let name: String = row.get("name");
        if let Some(arr) = days
            .get_mut(&day)
            .and_then(|v| v.as_object_mut())
            .and_then(|o| o.get_mut(&meal))
            .and_then(|v| v.as_array_mut())
        {
            arr.push(json!({"dish_id": id, "name": name}));
        }
    }
    let mut selected = serde_json::Map::new();
    for row in &sels {
        let day: String = row.get("day");
        let meal: String = row.get("meal");
        let id: i64 = row.get("dish_id");
        let servings: i32 = row.get("servings");
        selected.insert(format!("{day}|{meal}"), json!({"dish_id": id, "servings": servings}));
    }
    Ok(Json(json!({"ok": true, "days": days, "selected": selected})))
}

async fn generate_week(State(st): State<AppState>) -> ApiResult {
    sqlx::query("DELETE FROM weekly_options WHERE week_start = date_trunc('week', now())::date")
        .execute(&st.kitchen)
        .await
        .map_err(err)?;
    let res = sqlx::query(
        r#"
        INSERT INTO weekly_options (week_start, day, meal, dish_id)
        SELECT date_trunc('week', now())::date, d.day, m.meal, x.id
        FROM (VALUES ('Понедельник'),('Вторник'),('Среда'),('Четверг'),('Пятница'),('Суббота'),('Воскресенье')) AS d(day)
        CROSS JOIN (VALUES ('breakfast'),('lunch'),('dinner'),('snack')) AS m(meal)
        CROSS JOIN LATERAL (
            SELECT id FROM dishes WHERE active AND m.meal = ANY(meal_types) ORDER BY random() LIMIT 3
        ) AS x
        "#,
    )
    .execute(&st.kitchen)
    .await
    .map_err(err)?;
    Ok(Json(json!({"ok": true, "options": res.rows_affected()})))
}

#[derive(Deserialize)]
struct SelectBody {
    day: String,
    meal: String,
    dish_id: Option<i64>,
    servings: Option<i32>,
}

async fn select(State(st): State<AppState>, Json(b): Json<SelectBody>) -> ApiResult {
    match b.dish_id {
        Some(id) => {
            let servings = b.servings.unwrap_or(1).max(1);
            sqlx::query(
                "INSERT INTO selections (week_start, day, meal, dish_id, servings) \
                 VALUES (date_trunc('week', now())::date, $1, $2, $3, $4) \
                 ON CONFLICT (week_start, day, meal) \
                 DO UPDATE SET dish_id = EXCLUDED.dish_id, servings = EXCLUDED.servings, selected_at = now()",
            )
            .bind(&b.day).bind(&b.meal).bind(id).bind(servings)
            .execute(&st.kitchen).await.map_err(err)?;
        }
        None => {
            sqlx::query("DELETE FROM selections WHERE week_start = date_trunc('week', now())::date AND day = $1 AND meal = $2")
                .bind(&b.day).bind(&b.meal).execute(&st.kitchen).await.map_err(err)?;
        }
    }
    Ok(Json(json!({"ok": true})))
}

async fn dish(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult {
    let d = sqlx::query("SELECT name, base_servings, instructions FROM dishes WHERE id = $1")
        .bind(id).fetch_optional(&st.kitchen).await.map_err(err)?;
    let Some(d) = d else {
        return Err((StatusCode::NOT_FOUND, "dish not found".into()));
    };
    let name: String = d.get("name");
    let base_servings: i32 = d.get("base_servings");
    let instructions: Option<String> = d.get("instructions");
    let ings = sqlx::query(
        "SELECT i.name, i.base_unit, i.category, di.amount::float8 AS amount, di.unit, di.optional \
         FROM dish_ingredients di JOIN ingredients i ON i.id = di.ingredient_id WHERE di.dish_id = $1 ORDER BY i.name",
    )
    .bind(id).fetch_all(&st.kitchen).await.map_err(err)?;
    let items: Vec<Value> = ings.iter().map(|r| json!({
        "name": r.get::<String,_>("name"),
        "category": r.get::<String,_>("category"),
        "base_unit": r.get::<String,_>("base_unit"),
        "amount": r.get::<f64,_>("amount"),
        "unit": r.get::<String,_>("unit"),
        "optional": r.get::<bool,_>("optional"),
    })).collect();
    Ok(Json(json!({"ok": true, "id": id, "name": name, "base_servings": base_servings,
        "instructions": instructions, "ingredients": items})))
}

async fn shopping(State(st): State<AppState>) -> ApiResult {
    let rows = sqlx::query(
        r#"
        WITH need AS (
            SELECT i.id, i.name, i.category, i.base_unit,
                   SUM((CASE di.unit WHEN 'kg' THEN di.amount*1000 WHEN 'l' THEN di.amount*1000 ELSE di.amount END)
                       * s.servings::numeric / d.base_servings) AS need_base
            FROM selections s
            JOIN dishes d ON d.id = s.dish_id
            JOIN dish_ingredients di ON di.dish_id = s.dish_id AND NOT di.optional
            JOIN ingredients i ON i.id = di.ingredient_id
            WHERE s.week_start = date_trunc('week', now())::date
            GROUP BY i.id, i.name, i.category, i.base_unit
        )
        SELECT n.id, n.name, n.category, n.base_unit,
               (n.need_base - COALESCE(inv.qty, 0))::float8 AS deficit,
               COALESCE(sl.bought, false) AS bought
        FROM need n
        LEFT JOIN inventory inv ON inv.ingredient_id = n.id
        LEFT JOIN shopping_list sl ON sl.week_start = date_trunc('week', now())::date AND sl.ingredient_id = n.id
        WHERE (n.need_base - COALESCE(inv.qty, 0)) > 0
        ORDER BY n.category, n.name
        "#,
    )
    .fetch_all(&st.kitchen).await.map_err(err)?;

    let mut groups: Vec<Value> = Vec::new();
    let mut cur_cat: Option<String> = None;
    let mut cur_items: Vec<Value> = Vec::new();
    for r in &rows {
        let cat: String = r.get("category");
        let item = json!({
            "ingredient_id": r.get::<i64,_>("id"),
            "name": r.get::<String,_>("name"),
            "qty": (r.get::<f64,_>("deficit") * 100.0).round() / 100.0,
            "unit": r.get::<String,_>("base_unit"),
            "bought": r.get::<bool,_>("bought"),
        });
        if cur_cat.as_deref() != Some(cat.as_str()) {
            if let Some(c) = cur_cat.take() {
                groups.push(json!({"category": c, "items": cur_items.clone()}));
                cur_items.clear();
            }
            cur_cat = Some(cat);
        }
        cur_items.push(item);
    }
    if let Some(c) = cur_cat.take() {
        groups.push(json!({"category": c, "items": cur_items}));
    }
    Ok(Json(json!({"ok": true, "groups": groups})))
}

#[derive(Deserialize)]
struct ToggleBody {
    ingredient_id: i64,
    bought: bool,
}

async fn shopping_toggle(State(st): State<AppState>, Json(b): Json<ToggleBody>) -> ApiResult {
    sqlx::query(
        "INSERT INTO shopping_list (week_start, ingredient_id, need_qty, unit, bought) \
         VALUES (date_trunc('week', now())::date, $1, 0, '', $2) \
         ON CONFLICT (week_start, ingredient_id) DO UPDATE SET bought = EXCLUDED.bought",
    )
    .bind(b.ingredient_id).bind(b.bought).execute(&st.kitchen).await.map_err(err)?;
    Ok(Json(json!({"ok": true})))
}

async fn inventory(State(st): State<AppState>) -> ApiResult {
    let rows = sqlx::query(
        "SELECT i.id, i.name, i.category, i.base_unit, COALESCE(inv.qty, 0)::float8 AS qty \
         FROM ingredients i LEFT JOIN inventory inv ON inv.ingredient_id = i.id ORDER BY i.category, i.name",
    )
    .fetch_all(&st.kitchen).await.map_err(err)?;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "ingredient_id": r.get::<i64,_>("id"),
        "name": r.get::<String,_>("name"),
        "category": r.get::<String,_>("category"),
        "unit": r.get::<String,_>("base_unit"),
        "qty": (r.get::<f64,_>("qty") * 100.0).round() / 100.0,
    })).collect();
    Ok(Json(json!({"ok": true, "items": items})))
}

#[derive(Deserialize)]
struct AdjustBody {
    ingredient_id: i64,
    qty: f64,
}

async fn inventory_adjust(State(st): State<AppState>, Json(b): Json<AdjustBody>) -> ApiResult {
    sqlx::query(
        "INSERT INTO inventory (ingredient_id, qty, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (ingredient_id) DO UPDATE SET qty = EXCLUDED.qty, updated_at = now()",
    )
    .bind(b.ingredient_id).bind(b.qty).execute(&st.kitchen).await.map_err(err)?;
    Ok(Json(json!({"ok": true})))
}

// ---------------- API: чек ----------------

#[derive(Deserialize)]
struct ScanBody {
    qrraw: String,
}

async fn receipt_scan(State(st): State<AppState>, Json(b): Json<ScanBody>) -> ApiResult {
    if st.proverka_token.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "PROVERKACHEKA_TOKEN не задан".into()));
    }
    let resp = st
        .http
        .post("https://proverkacheka.com/api/v1/check/get")
        .form(&[("token", st.proverka_token.as_str()), ("qrraw", b.qrraw.as_str())])
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("proverkacheka: {e}")))?;
    let v: Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("proverkacheka json: {e}")))?;
    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
    if code != 1 {
        return Err((StatusCode::BAD_GATEWAY, format!("proverkacheka code={code} (1=успех)")));
    }
    let j = &v["data"]["json"];
    let items = j["items"].as_array().cloned().unwrap_or_default();

    // известные ингредиенты — чтобы LLM мапила на существующие имена
    let known: Vec<String> = sqlx::query("SELECT name FROM ingredients ORDER BY name")
        .fetch_all(&st.kitchen)
        .await
        .map_err(err)?
        .iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();

    let mut out: Vec<Value> = Vec::new();
    let mut unknown_idx: Vec<usize> = Vec::new();
    for it in &items {
        let name = it["name"].as_str().unwrap_or("").to_string();
        let price = it["price"].as_i64();
        let quantity = it["quantity"].as_f64();
        let sum = it["sum"].as_i64();
        let alias_key = norm(&name);
        let al = sqlx::query("SELECT ingredient_id, ingredient_name FROM ingredient_aliases WHERE alias = $1")
            .bind(&alias_key)
            .fetch_optional(&st.purchases)
            .await
            .map_err(err)?;
        let mut item = json!({
            "raw_name": name, "price_kop": price, "quantity": quantity, "sum_kop": sum,
            "ingredient": Value::Null, "ingredient_id": Value::Null,
            "category": "Прочее", "amount": quantity, "unit": "pcs", "from_alias": false
        });
        if let Some(a) = al {
            let iid: Option<i64> = a.get("ingredient_id");
            let iname: Option<String> = a.get("ingredient_name");
            item["ingredient"] = json!(iname);
            item["ingredient_id"] = json!(iid);
            item["from_alias"] = json!(true);
        } else {
            unknown_idx.push(out.len());
        }
        out.push(item);
    }

    // неизвестные — в LLM батчами по 6
    for chunk in unknown_idx.chunks(6) {
        let batch: Vec<Value> = chunk
            .iter()
            .map(|&i| json!({"name": out[i]["raw_name"], "quantity": out[i]["quantity"]}))
            .collect();
        if let Some(arr) = llm_parse_items(&st, &known, &batch).await {
            if arr.len() == chunk.len() {
                for (k, &i) in chunk.iter().enumerate() {
                    let p = &arr[k];
                    let ing = p.get("ingredient").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                    out[i]["category"] = p.get("category").cloned().unwrap_or(json!("Прочее"));
                    if let Some(a) = p.get("amount") {
                        out[i]["amount"] = a.clone();
                    }
                    if let Some(u) = p.get("unit") {
                        out[i]["unit"] = u.clone();
                    }
                    if !ing.is_empty() {
                        out[i]["ingredient"] = json!(ing);
                        if let Some(row) = sqlx::query("SELECT id FROM ingredients WHERE name = $1")
                            .bind(ing.to_lowercase())
                            .fetch_optional(&st.kitchen)
                            .await
                            .map_err(err)?
                        {
                            out[i]["ingredient_id"] = json!(row.get::<i64, _>("id"));
                        }
                    }
                }
            }
            // при несовпадении длины — оставляем позиции как есть (заполнит пользователь)
        }
    }

    let receipt_meta = json!({
        "fn": j["fiscalDriveNumber"], "fd": j["fiscalDocumentNumber"], "fp": j["fiscalSign"],
        "t": v["request"]["manual"]["check_time"].as_str().or_else(|| j["dateTime"].as_str()),
        "retailer": j["user"], "retailer_inn": j["userInn"], "total_sum_kop": j["totalSum"],
        "raw_qr": b.qrraw, "raw_json": v.clone()
    });
    Ok(Json(json!({"ok": true, "receipt": receipt_meta, "items": out})))
}

async fn llm_parse_items(st: &AppState, known: &[String], batch: &[Value]) -> Option<Vec<Value>> {
    let sys = format!(
        "Ты сопоставляешь товары из кассового чека с кулинарными ингредиентами. \
         Верни СТРОГО JSON-массив той же длины и порядка, что вход. Для каждой позиции объект: \
         {{\"ingredient\": <каноничное имя ингредиента на русском в нижнем регистре; по возможности выбери из списка известных>, \
         \"category\": <одна из: {cats}>, \"amount\": <число: масса/объём/штуки>, \"unit\": <g|kg|ml|l|pcs>}}. \
         amount бери из названия (например '...930мл' -> 930 ml; '2 кг' -> 2 kg); если в названии нет — используй количество (quantity) с unit pcs. \
         Если это не продукт питания (пакет, услуга и т.п.) — \"ingredient\": \"\". Без markdown и пояснений. \
         Известные ингредиенты: {known}.",
        cats = CATEGORIES.join(", "),
        known = known.join(", ")
    );
    let body = json!({
        "model": st.lemonade_model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": sys},
            {"role": "user", "content": serde_json::to_string(batch).ok()?}
        ]
    });
    let resp = st
        .http
        .post(format!("{}/api/v1/chat/completions", st.lemonade_base))
        .json(&body)
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;
    let content = v["choices"][0]["message"]["content"].as_str()?;
    match extract_json(content)? {
        Value::Array(a) => Some(a),
        _ => None,
    }
}

#[derive(Deserialize)]
struct ApplyItem {
    raw_name: String,
    price_kop: Option<i64>,
    quantity: Option<f64>,
    sum_kop: Option<i64>,
    ingredient: Option<String>,
    category: Option<String>,
    amount: Option<f64>,
    unit: Option<String>,
}

#[derive(Deserialize)]
struct ApplyReceipt {
    #[serde(rename = "fn")]
    fn_: Option<String>,
    fd: Option<String>,
    fp: Option<String>,
    t: Option<String>,
    retailer: Option<String>,
    retailer_inn: Option<String>,
    total_sum_kop: Option<i64>,
    raw_qr: Option<String>,
    raw_json: Option<Value>,
}

#[derive(Deserialize)]
struct ApplyBody {
    receipt: ApplyReceipt,
    items: Vec<ApplyItem>,
}

async fn receipt_apply(State(st): State<AppState>, Json(b): Json<ApplyBody>) -> ApiResult {
    let r = &b.receipt;
    let raw_json_str = r.raw_json.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "{}".into());
    let rec = sqlx::query(
        "INSERT INTO receipts (fn, fd, fp, t, retailer, retailer_inn, total_sum_kop, raw_qr, raw_json) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::jsonb) RETURNING id",
    )
    .bind(&r.fn_).bind(&r.fd).bind(&r.fp).bind(&r.t).bind(&r.retailer)
    .bind(&r.retailer_inn).bind(r.total_sum_kop).bind(&r.raw_qr).bind(&raw_json_str)
    .fetch_one(&st.purchases)
    .await
    .map_err(err)?;
    let receipt_id: i64 = rec.get("id");

    let mut applied = 0u32;
    for it in &b.items {
        let ing_name = it.ingredient.as_ref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
        let unit = it.unit.clone().unwrap_or_else(|| "pcs".into());
        let category = it.category.clone().unwrap_or_else(|| "Прочее".into());
        let amount = it.amount.unwrap_or(0.0);

        // резолвим/создаём ингредиент в kitchen
        let mut ingredient_id: Option<i64> = None;
        if let Some(name) = &ing_name {
            let found = sqlx::query("SELECT id FROM ingredients WHERE name = $1")
                .bind(name).fetch_optional(&st.kitchen).await.map_err(err)?;
            ingredient_id = match found {
                Some(row) => Some(row.get::<i64, _>("id")),
                None => {
                    let created = sqlx::query(
                        "INSERT INTO ingredients (name, category, base_unit) VALUES ($1,$2,$3) \
                         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
                    )
                    .bind(name).bind(&category).bind(base_unit_of(&unit))
                    .fetch_one(&st.kitchen).await.map_err(err)?;
                    Some(created.get::<i64, _>("id"))
                }
            };
        }

        // позиция чека (purchases)
        sqlx::query(
            "INSERT INTO receipt_items (receipt_id, raw_name, price_kop, quantity, sum_kop, \
             parsed_ingredient, parsed_category, parsed_amount, parsed_unit, ingredient_id, applied) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(receipt_id).bind(&it.raw_name).bind(it.price_kop).bind(it.quantity).bind(it.sum_kop)
        .bind(&ing_name).bind(&category).bind(amount).bind(&unit)
        .bind(ingredient_id).bind(ingredient_id.is_some() && amount > 0.0)
        .execute(&st.purchases)
        .await
        .map_err(err)?;

        // обновляем холодильник + запоминаем алиас
        if let Some(iid) = ingredient_id {
            if amount > 0.0 {
                let (base_amt, _bu) = to_base(amount, &unit);
                sqlx::query(
                    "INSERT INTO inventory (ingredient_id, qty, updated_at) VALUES ($1,$2,now()) \
                     ON CONFLICT (ingredient_id) DO UPDATE SET qty = inventory.qty + EXCLUDED.qty, updated_at = now()",
                )
                .bind(iid).bind(base_amt).execute(&st.kitchen).await.map_err(err)?;
                applied += 1;
            }
            sqlx::query(
                "INSERT INTO ingredient_aliases (alias, ingredient_id, ingredient_name) VALUES ($1,$2,$3) \
                 ON CONFLICT (alias) DO UPDATE SET ingredient_id = EXCLUDED.ingredient_id, ingredient_name = EXCLUDED.ingredient_name",
            )
            .bind(norm(&it.raw_name)).bind(iid).bind(ing_name.as_deref())
            .execute(&st.purchases)
            .await
            .map_err(err)?;
        }
    }

    Ok(Json(json!({"ok": true, "receipt_id": receipt_id, "applied": applied})))
}

// ---------------- static / SPA ----------------

async fn static_or_index(uri: Uri, base: String, index: String) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if !rel.is_empty() && !rel.contains("..") {
        let full = format!("{base}/{rel}");
        if let Ok(bytes) = tokio::fs::read(&full).await {
            return ([(header::CONTENT_TYPE, content_type(rel))], bytes).into_response();
        }
    }
    Html(index).into_response()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

use axum::extract::{Path, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;

const DAYS: [&str; 7] = [
    "Понедельник",
    "Вторник",
    "Среда",
    "Четверг",
    "Пятница",
    "Суббота",
    "Воскресенье",
];
const SLOTS: [&str; 4] = ["breakfast", "lunch", "dinner", "snack"];

type ApiResult = Result<Json<Value>, (StatusCode, String)>;

fn err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://kitchen:kitchen@localhost:5432/kitchen".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .expect("invalid DATABASE_URL");

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
        .with_state(pool)
        .fallback(move |uri: Uri| {
            let base = base.clone();
            let index = index.clone();
            async move { static_or_index(uri, base, index).await }
        });

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("kitchen-app listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ---------------- API ----------------

async fn health(State(pool): State<PgPool>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok();
    Json(json!({"ok": true, "service": "kitchen-app", "version": env!("CARGO_PKG_VERSION"), "db": db_ok}))
}

async fn week(State(pool): State<PgPool>) -> ApiResult {
    let opts = sqlx::query(
        "SELECT w.day, w.meal, w.dish_id, di.name \
         FROM weekly_options w JOIN dishes di ON di.id = w.dish_id \
         WHERE w.week_start = date_trunc('week', now())::date",
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let sels = sqlx::query(
        "SELECT day, meal, dish_id, servings FROM selections \
         WHERE week_start = date_trunc('week', now())::date",
    )
    .fetch_all(&pool)
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

async fn generate_week(State(pool): State<PgPool>) -> ApiResult {
    sqlx::query("DELETE FROM weekly_options WHERE week_start = date_trunc('week', now())::date")
        .execute(&pool)
        .await
        .map_err(err)?;
    let res = sqlx::query(
        r#"
        INSERT INTO weekly_options (week_start, day, meal, dish_id)
        SELECT date_trunc('week', now())::date, d.day, m.meal, x.id
        FROM (VALUES ('Понедельник'),('Вторник'),('Среда'),('Четверг'),('Пятница'),('Суббота'),('Воскресенье')) AS d(day)
        CROSS JOIN (VALUES ('breakfast'),('lunch'),('dinner'),('snack')) AS m(meal)
        CROSS JOIN LATERAL (
            SELECT id FROM dishes
            WHERE active AND m.meal = ANY(meal_types)
            ORDER BY random() LIMIT 3
        ) AS x
        "#,
    )
    .execute(&pool)
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

async fn select(State(pool): State<PgPool>, Json(b): Json<SelectBody>) -> ApiResult {
    match b.dish_id {
        Some(id) => {
            let servings = b.servings.unwrap_or(1).max(1);
            sqlx::query(
                "INSERT INTO selections (week_start, day, meal, dish_id, servings) \
                 VALUES (date_trunc('week', now())::date, $1, $2, $3, $4) \
                 ON CONFLICT (week_start, day, meal) \
                 DO UPDATE SET dish_id = EXCLUDED.dish_id, servings = EXCLUDED.servings, selected_at = now()",
            )
            .bind(&b.day)
            .bind(&b.meal)
            .bind(id)
            .bind(servings)
            .execute(&pool)
            .await
            .map_err(err)?;
        }
        None => {
            // снять выбор
            sqlx::query(
                "DELETE FROM selections WHERE week_start = date_trunc('week', now())::date AND day = $1 AND meal = $2",
            )
            .bind(&b.day)
            .bind(&b.meal)
            .execute(&pool)
            .await
            .map_err(err)?;
        }
    }
    Ok(Json(json!({"ok": true})))
}

async fn dish(State(pool): State<PgPool>, Path(id): Path<i64>) -> ApiResult {
    let d = sqlx::query("SELECT name, base_servings, instructions FROM dishes WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(err)?;
    let Some(d) = d else {
        return Err((StatusCode::NOT_FOUND, "dish not found".into()));
    };
    let name: String = d.get("name");
    let base_servings: i32 = d.get("base_servings");
    let instructions: Option<String> = d.get("instructions");

    let ings = sqlx::query(
        "SELECT i.name, i.base_unit, i.category, di.amount::float8 AS amount, di.unit, di.optional \
         FROM dish_ingredients di JOIN ingredients i ON i.id = di.ingredient_id \
         WHERE di.dish_id = $1 ORDER BY i.name",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    let items: Vec<Value> = ings
        .iter()
        .map(|r| {
            json!({
                "name": r.get::<String, _>("name"),
                "category": r.get::<String, _>("category"),
                "base_unit": r.get::<String, _>("base_unit"),
                "amount": r.get::<f64, _>("amount"),
                "unit": r.get::<String, _>("unit"),
                "optional": r.get::<bool, _>("optional"),
            })
        })
        .collect();

    Ok(Json(json!({
        "ok": true,
        "id": id,
        "name": name,
        "base_servings": base_servings,
        "instructions": instructions,
        "ingredients": items
    })))
}

async fn shopping(State(pool): State<PgPool>) -> ApiResult {
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
    .fetch_all(&pool)
    .await
    .map_err(err)?;

    // группируем по категории
    let mut groups: Vec<Value> = Vec::new();
    let mut cur_cat: Option<String> = None;
    let mut cur_items: Vec<Value> = Vec::new();
    for r in &rows {
        let cat: String = r.get("category");
        let item = json!({
            "ingredient_id": r.get::<i64, _>("id"),
            "name": r.get::<String, _>("name"),
            "qty": (r.get::<f64, _>("deficit") * 100.0).round() / 100.0,
            "unit": r.get::<String, _>("base_unit"),
            "bought": r.get::<bool, _>("bought"),
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

async fn shopping_toggle(State(pool): State<PgPool>, Json(b): Json<ToggleBody>) -> ApiResult {
    sqlx::query(
        "INSERT INTO shopping_list (week_start, ingredient_id, need_qty, unit, bought) \
         VALUES (date_trunc('week', now())::date, $1, 0, '', $2) \
         ON CONFLICT (week_start, ingredient_id) DO UPDATE SET bought = EXCLUDED.bought",
    )
    .bind(b.ingredient_id)
    .bind(b.bought)
    .execute(&pool)
    .await
    .map_err(err)?;
    Ok(Json(json!({"ok": true})))
}

async fn inventory(State(pool): State<PgPool>) -> ApiResult {
    let rows = sqlx::query(
        "SELECT i.id, i.name, i.category, i.base_unit, COALESCE(inv.qty, 0)::float8 AS qty \
         FROM ingredients i LEFT JOIN inventory inv ON inv.ingredient_id = i.id \
         ORDER BY i.category, i.name",
    )
    .fetch_all(&pool)
    .await
    .map_err(err)?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "ingredient_id": r.get::<i64, _>("id"),
                "name": r.get::<String, _>("name"),
                "category": r.get::<String, _>("category"),
                "unit": r.get::<String, _>("base_unit"),
                "qty": (r.get::<f64, _>("qty") * 100.0).round() / 100.0,
            })
        })
        .collect();
    Ok(Json(json!({"ok": true, "items": items})))
}

#[derive(Deserialize)]
struct AdjustBody {
    ingredient_id: i64,
    qty: f64,
}

async fn inventory_adjust(State(pool): State<PgPool>, Json(b): Json<AdjustBody>) -> ApiResult {
    sqlx::query(
        "INSERT INTO inventory (ingredient_id, qty, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (ingredient_id) DO UPDATE SET qty = EXCLUDED.qty, updated_at = now()",
    )
    .bind(b.ingredient_id)
    .bind(b.qty)
    .execute(&pool)
    .await
    .map_err(err)?;
    Ok(Json(json!({"ok": true})))
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

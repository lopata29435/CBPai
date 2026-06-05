# kitchen-app

Домашнее приложение: меню на неделю из **своей** базы блюд, список покупок и инвентаризация холодильника (в т.ч. через сканирование QR-кода чека → proverkacheka.com). Бэкенд на Rust (Axum), фронт — React + Vite (PWA). Один Docker-образ, собирается в GitHub Actions и публикуется в GHCR; сервер тянет готовый образ.

## Стек
- Backend: Rust + Axum + sqlx (Postgres) + reqwest.
- Frontend: React + Vite (PWA), `html5-qrcode`.
- Данные: Postgres, БД `kitchen` (рецепты/инвентарь) и `purchases` (чеки/цены).
- LLM: Lemonade (OpenAI-совместимый) для парсинга чеков и переводов.

## Локальная разработка
```bash
# фронт
cd web && npm install && npm run dev      # http://localhost:5173 (проксит /api на :3000)
# бэк (в отдельном терминале, нужен Rust)
cargo run                                  # http://localhost:3000
```

## Сборка образа вручную
```bash
docker build -t kitchen-app .
docker run --rm -p 3000:3000 kitchen-app
```

## Деплой на сервере
1. Запушить в репозиторий → GitHub Actions соберёт и запушит `ghcr.io/<owner>/kitchen-app:latest`.
2. На сервере положить `deploy/docker-compose.yml` в `~/stacks/kitchen-app/docker-compose.yml` (заменить `OWNER`), создать `.env` из `deploy/.env.example`.
3. Если пакет приватный: `docker login ghcr.io` (PAT с `read:packages`).
4. `cd ~/stacks/kitchen-app && docker compose pull && docker compose up -d`
5. Доступ из тайнета: `sudo tailscale serve --bg --https=8001 http://127.0.0.1:3000`

## Базы данных
```bash
docker exec -i postgres psql -U kitchen -d kitchen   < db/kitchen.sql
docker exec -i postgres psql -U kitchen -d postgres  -c "CREATE DATABASE purchases OWNER kitchen;"
docker exec -i postgres psql -U kitchen -d purchases < db/purchases.sql
```

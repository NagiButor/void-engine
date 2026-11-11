# PROJECT.md — инструкция для агентов (вместо CLAUDE.md)

Этот файл — главный источник правил для любого ИИ-агента (Claude Code и др.), работающего в репозитории. Читай его перед началом работы и обновляй, если меняется архитектура, команды или соглашения.

## Что это

**V.O.I.D. Engine** (Virtual Objects & Interactive Dynamics) — игровой движок автора Mi5hlen (Михаил Однолетко) и демо «Physics Lab» (интерактивная физическая лаборатория от первого лица, вдохновлена демо Half-Life 2 E3 2003). Платформа — Windows. Язык — **только Rust**. Репозиторий: https://github.com/NagiButor/void-engine

> ⚠️ Физика: автор называет движок физики «Box3D», но в текущем коде используется **Rapier** (`rapier3d 0.22`, см. `crates/void-engine/src/physics.rs`). Не подменяй физический движок без явной просьбы владельца. Если нужен именно Box3D — это отдельная задача миграции, согласуй её сначала.

## Структура

```text
Cargo.toml               workspace (void-engine, void-demo), общие версии зависимостей
crates/void-engine/src/  библиотека движка
  render.rs              wgpu: устройство, HDR/тени/viewmodel/post проходы
  mesh.rs                процедурные меши (скошенные боксы, сферы, lathe)
  textures.rs            процедурные бесшовные атласы цвет/roughness + normal
  glb.rs                 загрузчик статических GLB
  physics.rs             обёртка над Rapier: группы, лучи, пружина захвата, удары
  scene.rs               сборка пропов и сама лаборатория (PropKind, spawn, reset, draws)
  audio.rs               пространственный 32-голосный микшер на rodio, WAV
  lib.rs                 экспорт модулей, asset_dir(), save_rgba(), тест валидации шейдеров
crates/void-demo/src/    игра (бинарь void_demo)
  main.rs                event loop, игрок, взаимодействие, частицы, меню/HUD/настройки, --selftest
  rig.rs                 руки/оружие от первого лица, IK локтей, анимации
  settings.rs            настройки в settings.toml
assets/                  shaders (pbr.wgsl, post.wgsl), models (GLB), audio/sfx (WAV), fonts
tools/                   build-rust.ps1 (упаковка), generate_sfx.py / postprocess_sfx.py (звуки)
docs/                    скриншоты для README
```

Стек: `wgpu 0.20` (DX12/Vulkan), `winit 0.29`, `egui 0.28`, `glam 0.27`, `rapier3d 0.22`, `rodio 0.19`, `image`, `toml`/`serde`. Версии заданы в `[workspace.dependencies]` — новые зависимости добавляй туда же.

## Команды

```powershell
cargo build --release --locked
cargo run --release --locked -p void-demo -- --rhi=dx12      # или vulkan / auto
cargo run --release --locked -p void-demo -- --selftest      # headless-проверка физики/ассетов
.\tools\build-rust.ps1                                       # пакет в build/bin/Release
```

Обязательная проверка перед завершением задачи (то же самое делает CI, `.github/workflows/rust.yml`):

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p void-demo -- --selftest
```

Полезные флаги демо: `--smoke=N` (N кадров и выход), `--screenshot=путь.png`, `--showcase`, `--benchmark=N --novsync`, `--autostart`, `--adapter=N`, `--width/--height`, `--fullscreen`, `--validation`, `--help`. Ассеты ищутся рядом с exe, в предках репозитория или через `VOID_ASSETS`.

## Соглашения

- Единицы: метры, ось Y вверх. Физика — фиксированный шаг 60 Гц (`FIXED_DT`).
- Группы коллизий в `physics.rs`: `WORLD=1`, `PROP=2`, `PLAYER=4`. Удерживаемый предмет временно меняет фильтр (`hold_filter`) — не ломай восстановление фильтра (есть тест).
- Движок (`void-engine`) — переиспользуемая библиотека без игровой логики; игровая логика, ввод, UI и настройки живут в `void-demo`.
- Шейдеры — WGSL в `assets/shaders`; тест `gpu_shaders_parse_and_validate` обязан проходить после любой правки.
- Не используй `unsafe` без необходимости; ошибки — через `anyhow::Result`.
- Код должен проходить `clippy -D warnings` и `rustfmt`.
- Не придумывай функции в README: рендер — приближения (не трассировка лучей и не полный GI), и так и должно быть описано.
- `Cargo.lock` коммитится; собирай с `--locked`.
- Бинарные ассеты (GLB, WAV, шрифт) — авторские/сгенерированные; не заменяй и не удаляй без просьбы. Лицензии сторонних зависимостей — в `THIRD_PARTY_NOTICES.md` (обновляй при добавлении зависимостей).
- Лицензия проприетарная source-available (`LICENSE`, `COMMERCIAL_LICENSING.md`).

## Правила работы агента

1. Сначала прочитай релевантный код, затем меняй. Делай минимальные точечные правки, не рефакторь без запроса.
2. Не коммить, не пушь и не откатывай состояние репозитория без явной просьбы пользователя. Незакоммиченные файлы нельзя восстановить после `git reset --hard` / `git clean` — предупреждай.
3. Мусор (`target/`, `build/`, `*.log`, `settings.toml`, `__pycache__`) в `.gitignore` — не добавляй в репозиторий.
4. Временные файлы и скриншоты экспериментов не кладись в `docs/` — там только то, на что ссылается README.
5. После изменений в рендере/физике/сцене проверяй `--selftest` и, если есть GPU, `--smoke=90 --screenshot=...` в `dx12` и `vulkan`.
6. Обновляй `CHANGELOG.md` и README при заметных изменениях поведения.
7. Общайся с владельцем по-русски.

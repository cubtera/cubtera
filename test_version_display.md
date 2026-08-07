# Тест отображения версии в бета-релизах

## 🧪 Проверка версионирования

### Ожидаемое поведение:

1. **Стабильная версия:**
   ```bash
   $ cubtera --version
   cubtera 1.0.15
   ```

2. **Бета-версия (PR #123):**
   ```bash
   $ cubtera --version
   cubtera 1.0.15-beta.pr123
   ```

### Как это работает:

1. **Workflow обновляет Cargo.toml:**
   ```toml
   # До
   version = "1.0.15"
   
   # После (в бета-билде)
   version = "1.0.15-beta.pr123"
   ```

2. **Clap автоматически использует версию:**
   ```rust
   command!()  // Читает version из Cargo.toml
   ```

3. **Docker образы тоже показывают правильную версию:**
   ```bash
   $ docker run ghcr.io/cubtera/cubtera:beta-pr123 --version
   cubtera 1.0.15-beta.pr123
   ```

### Проверка в разных форматах:

- **CLI**: `cubtera --version`
- **API**: `GET /version` endpoint
- **Docker**: `docker run image --version`
- **Homebrew**: `brew info cubtera-beta`

### Важные моменты:

✅ **Правильно:**
- Версия обновляется в Cargo.toml перед сборкой
- Clap автоматически подхватывает новую версию
- Все бинарники показывают бета-версию

❌ **Было бы неправильно:**
- Оставлять стабильную версию в коде
- Показывать `1.0.15` вместо `1.0.15-beta.pr123`
- Разные версии в CLI и API 
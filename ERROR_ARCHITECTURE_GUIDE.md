# Cubtera Error Architecture Guide

## 🎯 Архитектурное решение: Гибридный подход

После анализа различных подходов к обработке ошибок, мы выбрали **гибридный подход** для Cubtera:

### Варианты рассмотренные:

1. **Монолитный** - один глобальный `CubteraError` для всего
2. **Модульный** - каждый модуль имеет свой тип ошибок  
3. **Гибридный** ⭐ - централизованный `CubteraError` + модульные Result типы

## 🏗️ Архитектура ошибок

### Центральный тип ошибок

```rust
#[derive(Debug, Error)]
pub enum CubteraError {
    /// Tools module errors (автоматическая конверсия)
    #[error("Tools error: {0}")]
    Tools(#[from] crate::tools::ToolsError),
    
    /// Configuration module errors
    #[error("Configuration error: {message}")]
    Config { message: String },
    
    /// Dimension module errors  
    #[error("Dimension error: {message}")]
    Dimension { message: String },
    
    /// Runner module errors
    #[error("Runner error: {runner_type}: {message}")]
    Runner { runner_type: String, message: String },
    
    /// CLI command errors
    #[error("CLI error: {command}: {message}")]
    Cli { command: String, message: String },
    
    /// API endpoint errors
    #[error("API error: {endpoint}: {message}")]
    Api { endpoint: String, message: String },
    
    /// Critical system errors
    #[error("Critical system error: {message}")]
    Critical { message: String },
    
    // ... другие варианты
}
```

### Система приоритетов ошибок

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,      // Предупреждения, продолжаем работу
    Medium,   // Ошибки, используем значения по умолчанию
    High,     // Серьезные ошибки, завершаем работу
    Critical, // Критические ошибки, немедленный выход
}
```

## 🔄 Стратегия миграции

### 1. Compatibility Layer

Создали слой совместимости для плавной миграции:

```rust
// Старый код
let config = read_config()
    .unwrap_or_exit("Failed to read config".to_string());

// Новый код (CLI)
let config = read_config()
    .unwrap_or_exit_with_log("Failed to read config");

// Новый код (библиотека)
fn load_config() -> CubteraResult<Config> {
    read_config()
        .to_config_error("Failed to read config")
}
```

### 2. Smart Error Handling

```rust
// Автоматическая обработка по приоритету
let config = load_config()
    .handle_by_severity(); // Автоматически решает что делать

// Кастомная обработка
let config = load_config()
    .handle_with_strategy(|severity, error| {
        match severity {
            ErrorSeverity::Critical => std::process::exit(1),
            ErrorSeverity::High => panic!("Critical error: {}", error),
            _ => Config::default()
        }
    });
```

### 3. Extension Traits

```rust
// Конверсия в специфичные типы ошибок
std::fs::read_to_string("config.toml")
    .to_config_error("Failed to read config")?;

// Добавление контекста
database_operation()
    .with_context("Database connection failed")?;
```

## 📋 Преимущества выбранного подхода

### ✅ Плюсы:

1. **Единая точка обработки** - все ошибки проходят через `CubteraError`
2. **Модульность** - каждый модуль может иметь свои Result типы
3. **Автоматические конверсии** - `From` traits для популярных типов
4. **Система приоритетов** - умная обработка по severity
5. **Обратная совместимость** - старый код продолжает работать
6. **Постепенная миграция** - можно мигрировать по частям

### ⚠️ Минусы:

1. **Сложность** - больше кода для поддержки
2. **Overhead** - дополнительные конверсии
3. **Learning curve** - нужно изучить новые patterns

## 🚀 Примеры использования

### Модуль Configuration

```rust
// src/core/cfg/error.rs
pub type ConfigResult<T> = std::result::Result<T, CubteraError>;

impl Config {
    pub fn load(path: &Path) -> ConfigResult<Self> {
        let content = std::fs::read_to_string(path)
            .to_config_error("Failed to read config file")?;
            
        let config: Config = toml::from_str(&content)
            .to_config_error("Invalid config format")?;
            
        Ok(config)
    }
}
```

### CLI Commands

```rust
// src/bin/cli/commands/deploy.rs
use cubtera::prelude::*;

pub fn deploy_command(args: &DeployArgs) -> CubteraResult<()> {
    // Загружаем конфигурацию
    let config = Config::load(&args.config_path)?;
    
    // Валидируем dimension
    let dimension = Dimension::load(&args.dimension)
        .to_dimension_error("Invalid dimension")?;
    
    // Выполняем deployment
    let runner = create_runner(&config, &dimension)?;
    runner.execute()
        .to_runner_error(&runner.name(), "Deployment failed")?;
    
    Ok(())
}

// В main.rs
fn main() {
    if let Err(e) = deploy_command(&args) {
        // Умная обработка по severity
        match e.severity() {
            ErrorSeverity::Critical | ErrorSeverity::High => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            _ => {
                eprintln!("Warning: {}", e);
            }
        }
    }
}
```

### API Endpoints

```rust
// src/bin/api/handlers/units.rs
use rocket::serde::json::Json;

#[post("/units", data = "<unit>")]
pub fn create_unit(unit: Json<UnitRequest>) -> Result<Json<UnitResponse>, ApiError> {
    let unit = Unit::create(unit.into_inner())
        .to_api_error("/api/units", "Failed to create unit")
        .map_err(ApiError::from)?;
        
    Ok(Json(UnitResponse::from(unit)))
}

// Автоматическая конверсия в HTTP статусы
impl From<CubteraError> for ApiError {
    fn from(err: CubteraError) -> Self {
        match err.severity() {
            ErrorSeverity::Critical => ApiError::InternalServerError,
            ErrorSeverity::High => ApiError::BadRequest,
            ErrorSeverity::Medium => ApiError::UnprocessableEntity,
            ErrorSeverity::Low => ApiError::BadRequest,
        }
    }
}
```

## 🔧 Макросы для удобства

```rust
// Быстрая миграция старых patterns
let config = unwrap_or_exit!(load_config(), "Config required");

// Умная обработка
let config = handle_smart!(load_config());

// Предупреждение и default
let cache = warn_and_default!(load_cache(), "Cache unavailable");
```

## 📊 Метрики и мониторинг

```rust
impl CubteraError {
    pub fn category(&self) -> &'static str { /* ... */ }
    pub fn severity(&self) -> ErrorSeverity { /* ... */ }
}

// Можно использовать для метрик
error_counter.increment(&[
    ("category", error.category()),
    ("severity", &format!("{:?}", error.severity())),
]);
```

## 🎯 Следующие шаги

1. **Phase 2**: Мигрировать core модули на новую систему ошибок
2. **Phase 3**: Добавить structured logging с контекстом
3. **Phase 4**: Интегрировать с системой мониторинга
4. **Phase 5**: Добавить error recovery mechanisms

## 📚 Ресурсы

- [Rust Error Handling Best Practices](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [thiserror Documentation](https://docs.rs/thiserror/)
- [anyhow vs thiserror](https://nick.groenen.me/posts/rust-error-handling/)

---

**Заключение**: Гибридный подход дает нам лучшее из двух миров - централизованную обработку ошибок с сохранением модульности и возможностью постепенной миграции. 
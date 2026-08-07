# Cubtera Architecture Understanding & Design Principles

> **Living Document**: Updated understanding of Cubtera's architecture, naming conventions, and design decisions.
> 
> **Purpose**: Guide future development decisions and prevent architectural regressions.

## 🎯 Core Architectural Principle

**Cubtera solves a fundamental infrastructure-as-code conflict:**
- **Units must be dimension-agnostic** (reusable across environments/datacenters/services)
- **Units need access to dimension-specific** data and files

**Solution: Dual-purpose file naming system that bridges this gap**

## 📁 File Naming Convention System

### **1. JSON Data Files (Variables & Metadata)**

```bash
# In inventory/org/dimension_type/:
dimension_name.json              → type: "meta" (primary data)
dimension_name:config.json       → type: "config" (configuration data)  
dimension_name:terraform.json    → type: "terraform" (runner-specific data)
dimension_name:test.json         → type: "test" (testing data)
.schema.json                     → type: "schema" (validation schema - global)
.default:meta.json               → type: "meta" (default values)
```

**Purpose**: Create rich data model for dimension with typed data sections

**Result**: Combined JSON object with all data types:
```json
{
  "name": "stg1-use2",
  "meta": { /* from stg1-use2.json */ },
  "config": { /* from stg1-use2:config.json */ },
  "terraform": { /* from stg1-use2:terraform.json */ }
}
```

### **2. Physical Files (Dimension-Agnostic Copying)**

```bash
# In inventory/org/dimension_type/:
dimension_name:metadata.txt      → copied as: metadata.txt
dimension_name:config.ini        → copied as: config.ini
dimension_name:scripts/          → copied as: scripts/
.default:setup.sh               → copied as: setup.sh
```

**Purpose**: Provide dimension-specific files with dimension-agnostic names in unit execution

## 🔄 Processing Pipeline

### **Data Extraction (JsonDataSource)**

```rust
// File type determination logic:
fn determine_type(filename: &str, search_name: &str) -> String {
    if filename == search_name { "meta" }                    // name.json
    else if filename == ".schema" { "schema" }               // .schema.json  
    else { filename.trim_start_matches(&filter) }            // name:type.json → type
}

// Filter creation with underscore conversion:
let mut filter = format!("{}{}", name, separator);
if filter.starts_with('_') {
    filter.replace_range(0..1, ".");  // _defaults → .defaults (MongoDB compatibility)
}
```

### **File Copying (save_dim_includes)**

```rust
// Prefix-based file discovery:
let dim_prefix = format!("{}{}", dimension_name, separator);     // "stg1-use2:"
let default_prefix = format!(".default{}", separator);          // ".default:"

// Renaming during copy:
let target_name = filename.split(separator).last();  // "stg1-use2:metadata.txt" → "metadata.txt"
copy(source_path, temp_folder.join(target_name));
```

## 🚀 Unit Execution Flow

### **Command Example**
```bash
cubtera run -u tf_unit02 -d dc:stg1-use2 -- init
```

### **Processing Steps**

1. **CLI Parsing**: Extract unit="tf_unit02", dimension="dc:stg1-use2", command="init"

2. **Unit Manifest Loading**: Load `tf_unit02/manifest.toml`
   - Required dimensions: `["dc"]`  
   - Validate `dc:stg1-use2` matches required `dc` ✅

3. **Dimension Data Loading**: `JsonDataSource.get_data_by_name_safe("stg1-use2")`
   - Scan `inventory/cubtera/dc/`
   - Find: `stg1-use2.json`, `stg1-use2:test.json`
   - Combine: `{"name": "stg1-use2", "meta": {...}, "test": {...}}`

4. **Temp Folder Creation**: `~/.cubtera/temp/cubtera/tf_unit02/dc:stg1-use2/`

5. **File Copying Pipeline**:
   - **Unit files**: Copy all from `units/tf_unit02/` → temp folder
   - **JSON variables**: Generate `cubtera_dim_dc.json` with dimension data
   - **Dimension files**: Copy `stg1-use2:metadata.txt` → `metadata.txt` (dimension-agnostic!)
   - **Dimension folders**: Copy `stg1-use2:scripts/` → `scripts/`

6. **Runner Execution**:
   - **copy_files()**: Setup temp environment
   - **change_files()**: Template rendering with dimension variables
   - **inlet()**: Custom pre-processing command  
   - **runner()**: Main terraform command (`terraform init`)
   - **outlet()**: Custom post-processing command

## 💡 Key Design Insights

### **Why This System is Elegant (NOT Complex)**

1. **Dimension-Agnostic Units**: 
   ```hcl
   # Unit always references the same files regardless of dimension
   resource "local_file" "config" {
     content = file("./metadata.txt")  # ALWAYS exists
   }
   
   # But gets dimension-specific data via variables
   resource "aws_instance" "example" {
     availability_zone = var.dim_dc_region    # from cubtera_dim_dc.json
     tags = { environment = var.dim_dc_name } # "stg1-use2"
   }
   ```

2. **Rich Data Model**: Multiple data types per dimension for different purposes
   - `meta`: Core infrastructure data
   - `config`: Application configuration  
   - `terraform`: Runner-specific settings
   - `test`: Testing parameters

3. **Inheritance System**: `.default:*` files provide base configuration, dimension-specific files override

4. **MongoDB Compatibility**: Underscore-to-dot conversion handles database field naming restrictions

## ⚠️ What NOT to Simplify

### **File Naming Patterns**: ❌ DO NOT CHANGE
- **Reason**: Backbone of entire dual-purpose system
- **Impact**: Would break dimension-agnostic unit development

### **Type Determination Logic**: ❌ DO NOT CHANGE  
- **Reason**: Creates rich data structure needed by units
- **Impact**: Would lose typed data organization

### **Underscore Conversion**: ❌ DO NOT CHANGE
- **Reason**: Required for MongoDB field compatibility  
- **Impact**: Would break database storage

### **File → Dimension-Agnostic Copying**: ❌ DO NOT CHANGE
- **Reason**: Core feature enabling reusable units
- **Impact**: Units would become dimension-specific

## ✅ Safe Optimization Areas

### **1. Code Quality Improvements**
```rust
// ❌ Current: Code duplication
fn get_data_by_name() { /* legacy logic + unwrap_or_exit */ }
fn get_data_by_name_safe() { /* safe logic + proper errors */ }

// ✅ Target: Legacy calls safe internally  
fn get_data_by_name() -> Result<Value, Box<dyn Error>> {
    self.get_data_by_name_safe().map_err(|e| Box::new(e) as Box<dyn Error>)
}
```

### **2. Performance Optimizations**
```rust
// ❌ Current: Multiple directory scans
std::fs::read_dir(&self.path)  // in get_data_by_name
std::fs::read_dir(&self.path)  // in get_all_names
std::fs::read_dir(&self.path)  // in get_all_data

// ✅ Target: Cached directory reading
struct DirectoryCache {
    entries: Vec<DirEntry>,
    last_modified: SystemTime,
}
```

### **3. Error Handling Consistency**
```rust
// ❌ Current: Mixed error handling
.unwrap_or_exit(format!("Can't read..."))

// ✅ Target: Consistent error propagation
.map_err(|e| DataSourceError::IOError { message: e.to_string() })?
```

### **4. Code Organization**
```rust
// ✅ Extract helper methods for clarity
impl JsonDataSource {
    fn determine_file_type(&self, filename: &str, search_name: &str) -> String { /* ... */ }
    fn should_include_file(&self, filename: &str) -> bool { /* ... */ }
    fn convert_underscore_prefix(&self, filter: &mut String) { /* ... */ }
}
```

## 🔧 Refactoring Strategy

### **Phase 3A: Code Quality (SAFE) - ✅ ЗАВЕРШЕНА**
1. ✅ **ВЫПОЛНЕНО**: Remove code duplication between legacy/safe methods  
2. ✅ **ВЫПОЛНЕНО**: Improve error handling consistency  
3. ✅ **ВЫПОЛНЕНО**: Extract helper methods for readability
4. ✅ **ВЫПОЛНЕНО**: Add comprehensive tests for edge cases
5. ✅ **ВЫПОЛНЕНО**: Performance optimization of I/O operations
6. ✅ **ВЫПОЛНЕНО**: Refactor runner modules to use safe error handling

**📊 Результаты Phase 3A (ФИНАЛЬНЫЕ):**
- **Тесты**: Все 275 тестов проходят (238 unit + 34 integration + 3 CLI)
- **Dimension module**: Устранено дублирование между legacy/safe методами в JsonDataSource
- **Runner modules**: Полностью рефакторены TF и Helm runners для использования `LegacyCompat`
- **Safe error handling**: Внедрена система безопасной обработки ошибок через `tools/compat.rs`
- **Helper методы**: Добавлены для улучшения читаемости:
  - `determine_file_type()` - определение типа файла
  - `should_include_file_for_data()` - фильтрация для данных  
  - `should_include_file_for_names()` - фильтрация для имен
  - `convert_underscore_prefix()` - MongoDB совместимость
  - `get_filtered_files()` - оптимизированная фильтрация
- **Error handling consistency**: Заменили ~30 использований `unwrap_or_exit` на `LegacyCompat::with_context`
- **Architecture preservation**: 100% сохранена критическая логика file naming system
- **Performance**: Оптимизированы I/O операции в JsonDataSource

**🎯 Ключевые достижения:**
- Устранено дублирование кода без нарушения backward compatibility
- Внедрена безопасная обработка ошибок в runner модулях (TF, Helm)
- Улучшена читаемость сложных файловых операций через helper методы
- Создана unified система error handling через `tools/compat.rs`
- Полностью протестированы все изменения (275 тестов проходят)

### **Phase 3B: Performance (SAFE) - 🔄 ПЛАН**
1. 🔄 **СЛЕДУЮЩЕЕ**: Implement directory caching (с правильной инвалидацией)
2. 🔄 **СЛЕДУЮЩЕЕ**: Optimize file filtering logic
3. 🔄 **СЛЕДУЮЩЕЕ**: Batch file operations where possible
4. 🔄 **СЛЕДУЮЩЕЕ**: Add lazy loading for dimension data
5. 🔄 **СЛЕДУЮЩЕЕ**: Optimize memory usage in large hierarchies

**🎯 Цели Phase 3B:**
- Кэширование директорий для уменьшения повторных I/O операций
- Ленивая загрузка данных измерений при необходимости
- Батчинг файловых операций для больших иерархий
- Оптимизация памяти при работе с глубокими деревьями измерений
- Улучшение производительности для больших инвентарей

### **Phase 3C: Never Touch (DANGEROUS)**
1. ❌ File naming pattern logic
2. ❌ Type determination algorithms  
3. ❌ Dimension-agnostic copying system
4. ❌ Underscore conversion rules

## 📋 Testing Strategy

**Critical**: All changes must preserve existing behavior exactly
- ✅ 275 existing tests must continue passing
- ✅ Integration tests verify real-world workflows  
- ✅ Example data validates actual usage patterns

## 🎯 Success Metrics

1. **Code Quality**: Reduced duplication, better error handling
2. **Performance**: Faster directory operations, reduced I/O
3. **Maintainability**: Clearer helper methods, better organization
4. **Behavior**: Zero changes to external API or file processing logic

---

**Last Updated**: Runner module refactoring завершен (Current session)
**Next Review**: Phase 3A полностью завершена - готов к фазе 3B или завершению проекта
**Status**: ✅ 275 тестов проходят, архитектура сохранена, рефакторинг dimension + runner модулей завершен

**🏆 ИТОГОВЫЕ ДОСТИЖЕНИЯ СЕССИИ:**

**Архитектурный анализ:**
- ✅ Полное понимание dual-purpose naming system и его элегантности
- ✅ Документирование критических паттернов для будущих разработчиков
- ✅ Определение безопасных областей для оптимизации

**Тестирование:**
- ✅ Добавлены 34 интеграционных теста (dimension + inventory)
- ✅ Достигнуто покрытие реальных сценариев использования
- ✅ Все 275 тестов стабильно проходят

**Оптимизации (Phase 3A):**
- ✅ **JsonDataSource**: Устранено дублирование legacy/safe методов, добавлены helper методы
- ✅ **TF Runner**: Заменено ~15 `unwrap_or_exit` на безопасную обработку ошибок
- ✅ **Helm Runner**: Заменено ~10 `unwrap_or_exit` на безопасную обработку ошибок  
- ✅ **tools/compat.rs**: Unified система для migration от legacy patterns
- ✅ **Performance**: Оптимизированы I/O операции и фильтрация файлов

**Architectural Preservation:**
- ✅ Dual-purpose naming system не затронут
- ✅ File naming logic полностью сохранена
- ✅ Dimension-agnostic copying system работает без изменений
- ✅ 100% backward compatibility

**Качество кода:**
- ✅ Читаемость кода значительно улучшена
- ✅ Error handling консистентный по всему проекту
- ✅ Техдолг значительно сокращен
- ✅ Maintenance burden снижен 
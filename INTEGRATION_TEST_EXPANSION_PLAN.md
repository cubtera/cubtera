# План расширения интеграционных тестов и example данных

## Цель
Расширить example inventory и создать comprehensive интеграционные тесты, покрывающие все edge cases и сценарии использования системы Cubtera.

## Текущий статус интеграционных тестов ✅
**Успешно созданы и протестированы:**
- 10 inventory integration tests - все проходят
- 10 dimension integration tests - все проходят  
- Покрытие основной функциональности data extraction

**Что протестировано:**
- Структура файловой системы inventory
- Извлечение данных из JSON файлов
- Иерархии dimensions (dome → env → dc → service → mongodb/postgres)
- Parent relationships и type patterns
- Multi-org support (cubtera, teracub)
- Error handling для missing files/orgs
- Default values и схемы
- Complex file naming patterns (`:` и `#` separators)
- Environment-specific configurations
- Cross-dimension consistency

## План расширения example данных

### 1. **Дополнительные Edge Cases для File System**

#### A. Special Characters в именах dimensions
```
example/inventory/cubtera/dc/
├── test-env_v2.0.json         # Underscore + version
├── prod.east-1a:backup.json   # Dots в имени + type
├── stg_2023:config.yaml       # YAML формат
├── dev@branch:meta.json       # @ symbol
```

#### B. Глубокие иерархии с множественными типами
```
example/inventory/cubtera/
├── region/
│   ├── us-east-1.json
│   ├── eu-west-1.json
│   └── .default:meta.json
├── network/
│   ├── vpc-prod:terraform.json
│   ├── vpc-prod:ansible.yaml
│   ├── vpc-stg:helm.yaml
```

#### C. Сложные nested структуры данных
```json
{
  "name": "complex-dimension",
  "meta": {
    "parent": "region:us-east-1",
    "tags": ["prod", "critical"],
    "config": {
      "scaling": {
        "min": 2,
        "max": 10,
        "metrics": ["cpu", "memory"]
      }
    }
  },
  "terraform": {
    "backend": {
      "s3": {
        "bucket": "{{org}}-{{env}}-state",
        "key": "{{unit}}/terraform.tfstate"
      }
    }
  }
}
```

### 2. **Конфигурационные расширения**

#### A. Множественные org configurations
```toml
[acme]
inventory_path = "example/inventory"
file_name_separator = ":"

[acme.runner.tf]
version = "1.6.0"
state_backend = "gcs"

[acme.state.gcs]
bucket = "acme-tf-state"
prefix = "{{dim_tree}}/{{unit}}"

[acme.runner.helm]
version = "3.12.0"
namespace = "{{env}}-{{service}}"
```

#### B. Alternative separators и patterns
```
example/inventory_alt/
├── config_alt.toml     # separator = "#"
└── org1/
    ├── type1/
    │   ├── dim1#meta.json
    │   ├── dim1#config.yaml
    │   └── _defaults#base.json
```

### 3. **Unit Examples Expansion**

#### A. Различные типы units
```
example/units/
├── terraform_complex/
│   ├── manifest.toml
│   ├── main.tf
│   ├── variables.tf
│   ├── backend.tf
│   └── modules/
├── helm_chart/
│   ├── manifest.toml
│   ├── Chart.yaml
│   ├── values.yaml
│   └── templates/
├── ansible_playbook/
│   ├── manifest.toml
│   ├── site.yml
│   └── roles/
├── docker_compose/
│   ├── manifest.toml
│   ├── docker-compose.yml
│   └── .env.template
```

#### B. Complex manifest examples
```toml
[manifest]
dimensions = ["region", "env", "dc", "service"]
opt_dims = ["feature"]
unit_type = "terraform"

[manifest.affinity]
tags = ["database", "backend"]
anti_tags = ["frontend"]

[manifest.runner.terraform]
version = "1.6.0"
extra_args = ["-parallelism=5", "-compact-warnings"]
inlet_command = "echo 'Starting deployment to {{dim_tree}}'"
outlet_command = "terraform output -json > output.json"

[manifest.state.s3]
bucket = "{{org}}-terraform-state"
key = "{{dim_tree}}/{{unit}}.tfstate"
region = "{{region}}"
encrypt = true
dynamodb_table = "{{org}}-terraform-locks"
```

### 4. **Error Cases и Edge Scenarios**

#### A. Malformed JSON files
```json
// broken.json
{
  "name": "broken",
  "meta": {
    "invalid": ,
    "unterminated": "string
  }
}
```

#### B. Circular dependencies
```json
// dim1.json
{"parent": "type:dim2"}

// dim2.json  
{"parent": "type:dim1"}
```

#### C. Missing parent references
```json
{
  "name": "orphan",
  "meta": {
    "parent": "nonexistent:missing"
  }
}
```

### 5. **MongoDB Integration Examples**

#### A. Database test data
```json
// Collections to populate for testing
{
  "_id": ObjectId("..."),
  "name": "prod-db-dimension", 
  "context": "test-context",
  "data": {
    "meta": {
      "parent": "env:prod",
      "database_type": "mongodb"
    }
  }
}
```

### 6. **Performance Test Data**

#### A. Large inventory structures
```
example/inventory_large/
├── org1/
│   ├── dc/           # 100+ dimensions
│   ├── service/      # 200+ dimensions  
│   └── mongodb/      # 50+ dimensions
```

## План новых интеграционных тестов

### 1. **Configuration Integration Tests**
```rust
#[test]
fn test_multi_org_configuration_loading()

#[test] 
fn test_alternative_separators()

#[test]
fn test_mixed_format_files() // JSON + YAML
```

### 2. **Complex Hierarchy Tests**
```rust
#[test]
fn test_deep_dimension_hierarchies()

#[test]
fn test_circular_dependency_detection()

#[test]
fn test_missing_parent_handling()
```

### 3. **Unit Integration Tests**
```rust
#[test]
fn test_terraform_unit_with_real_dimensions()

#[test]
fn test_helm_unit_manifest_processing()

#[test]
fn test_complex_template_rendering()
```

### 4. **Performance Tests**
```rust
#[test]
fn test_large_inventory_performance()

#[test]
fn test_concurrent_dimension_loading()
```

### 5. **Error Handling Tests**
```rust
#[test]
fn test_malformed_json_recovery()

#[test]
fn test_filesystem_permission_errors()

#[test]
fn test_database_connection_failures()
```

### 6. **End-to-End Workflow Tests**
```rust
#[test]
fn test_full_deployment_simulation()
// config → inventory → units → execution

#[test]
fn test_bom_generation_integration()

#[test]
fn test_multi_unit_orchestration()
```

## Приоритеты реализации

### **Фаза 1: Расширение Edge Cases** 
- Специальные символы в именах
- Alternative файловые форматы  
- Malformed data handling

### **Фаза 2: Complex Scenarios**
- Deep hierarchies
- Circular dependencies
- Performance с большими данными

### **Фаза 3: End-to-End Integration**
- Full workflow testing
- Multi-component orchestration
- Production-like scenarios

### **Фаза 4: Error Recovery**
- Graceful degradation
- Partial failure handling
- Recovery mechanisms

## Ожидаемые результаты

1. **100% confidence** в рефакторинге data layer
2. **Production-ready** error handling  
3. **Performance benchmarks** для оптимизации
4. **Documentation** реальных использования patterns
5. **Regression protection** для будущих изменений

## Метрики успеха

- **50+ интеграционных тестов** покрывающих все сценарии
- **0 regression** после рефакторинга
- **< 100ms** для типичных dimension queries  
- **Graceful handling** всех error cases
- **Clear error messages** для пользователей 
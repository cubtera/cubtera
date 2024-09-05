# API features of Cubtera

## Prerequisites for running API service with docker:
1. `CUBTERA_DB` env var should be set to your mongoDB db url
2. `CUBTERA_API` env var should be set to `true` to enable API service start
3. `CUBTERA_DIM_RELATIONS` with proper string from the config. If not provided, parent-child related api calls will return `Null` values

API feature works only with DB storage. If you want to use local files, you should use cubtera as cli tool.

## API features

### Dimension management
- GET `/v1/orgs` - get all Orgs available in current inventory
- GET `/v1/${org}/dimTypes` - get list of all dimensions's types in a Org
- GET `/v1/${org}/dim?type=${type}&name=${name}` - get full dimension data by name and type
- GET `/v1/${org}/dims?type=${type}` - get list of all dimension's names by type 
- GET `/v1/${org}/dimsData?type=${type}` - get list of all dimension's with full data by type
- GET `/v1/${org}/dimDefaults?type=${type}` - get defaults data by dim type
- GET `/v1/${org}/dimParent?type=${type}&name=${name}` - get dimension's parent by dim name and type
- GET `/v1/${org}/dimsByParent?type=${type}&name=${name}` - get all dimension's kids by name and type

#####  Response example
GET `/v1/${org}/dim?type="dc"&name="staging1-us-e2"`
```json
{
    "status": "ok",
    "id": "dimByName",
    "type": "dc",
    "name": "staging1-us-e2",
    "data": {
        "meta" : {
            ...
        }, 
        "manifest": {
            ...
        }, // or null
        "defaults": {
            ...
        }, // or null
    },
}
```

GET `/v1/${org}/dims?type="dc"`
```json
{
    "status": "ok",
    "id": "dimsByType",
    "type": "dc",
    "data": ["staging1-us-e2", "production-us-e1", "production-us-e2"]
}
```


### Deployment log query
- GET `/v1/${org}/dlog?${dlog_key}=${dlog_value}&...&limit=${limit_number}` - get deployment log by any number of keys with required values, limit is optional parameter (default 10)

dlog example:
```json
{
    
}
```
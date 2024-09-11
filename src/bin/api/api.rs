use cubtera::prelude::*;

use rocket::serde::json::{json, Value};
use rocket::{catch, catchers, get, launch, routes, Build, Request, Rocket};

#[get("/<org>/dimTypes")] // -> list of all dim types in org
fn dim_types(org: &str) -> Value {
    get_all_dim_types(org, &Storage::DB)
}

#[get("/<org>/dims?<type>")] // -> list of dims by type
fn dims_by_type(r#type: &str, org: &str) -> Value {
    get_dim_names_by_type(r#type, org, &Storage::DB)
}

#[get("/<org>/dimsData?<type>")] // -> list of dims data by type
fn dims_data_by_type(r#type: &str, org: &str) -> Value {
    get_dims_data_by_type(r#type, org, &Storage::DB)
}

#[get("/<org>/dim?<type>&<name>&<context>")]
fn dim_by_name(
    r#type: &str,
    name: &str,
    org: &str,
    context: Option<String>,
) -> Value { // -> dim data by type and name
    get_dim_by_name(r#type, name, org, &Storage::DB, context)
}

#[get("/<org>/dimDefaults?<type>")]
fn dim_defaults_by_type(
    r#type: &str,
    org: &str,
) -> Value { // -> dim data by type and name
    get_dim_defaults_by_type(r#type, org, &Storage::DB)
}

#[get("/<org>/dimParent?<type>&<name>")]
fn dim_parent(r#type: &str, name: &str, org: &str) -> Value {
    get_dim_parent(r#type, name, org, &Storage::DB)
}

#[get("/<org>/dimsByParent?<type>&<name>")]
fn dims_by_parent(r#type: &str, name: &str, org: &str) -> Value {
    get_dim_kids(r#type, name, org, &Storage::DB)
}

#[get("/orgs")]
fn all_orgs(
    //key: ApiKey<'_> // <- Here we use our ApiKey guard
) -> Value {
    get_all_orgs(&Storage::DB)
}

#[catch(404)]
fn not_found(req: &Request) -> String {
    format!("Sorry, '{}' is not a valid path.", req.uri())
}

#[get("/health")]
fn health() -> Value {
    json!( {
        "status": "success",
        "message": "Cubtera is alive...",
    })
}

// #[get("/<org>/dlog?<limit>&<key..>")]
// fn get_dlog_handler(org: &str, limit: Option<i64>, key:Dlog) -> Value {
//     get_dlog(org, json!(key), limit)
// }

#[launch]
pub async fn rocket() -> Rocket<Build> {
    let _ = GLOBAL_CFG.db_client.clone().unwrap_or_exit(
        "Can't connect to mongodb. Provide correct connection string with CUBTERA_DB".to_string(),
    );
    rocket::build()
        .mount(
            "/v1",
            routes![
                dim_types,
                dim_by_name,
                dims_by_type,
                dim_parent,
                all_orgs,
                dims_by_parent,
                dims_data_by_type,
                dim_defaults_by_type,
                // get_dlog_handler,
            ],
        )
        .mount("/", routes![health])
        .register("/", catchers![not_found])
    //.manage(client)
    //.launch().await?;
    //Ok(())
}

// pub fn start() {
//     rocket::ignite()
//         .mount("/", routes![get_data, add_data, update_data, delete_data])
//         .launch();
// }


// API key guard implementation
// add ( key: ApiKey<'_> ) for all required routes params to enable it
use rocket::http::Status;
use rocket::request::{Outcome, FromRequest};
use cubtera::prelude::data::Storage;

struct ApiKey<'r>(&'r str);

#[derive(Debug)]
enum ApiKeyError {
    Missing,
    Invalid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiKey<'r> {
    type Error = ApiKeyError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        /// Returns true if `key` is a valid API key string.
        fn is_valid(key: &str) -> bool {
            key == "123456789"
        }

        match req.headers().get_one("x-api-key") {
            None => Outcome::Error((Status::Unauthorized, ApiKeyError::Missing)),
            Some(key) if is_valid(key) => Outcome::Success(ApiKey(key)),
            Some(_) => Outcome::Error((Status::Unauthorized, ApiKeyError::Invalid)),
        }
    }
}
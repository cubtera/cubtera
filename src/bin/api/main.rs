#![allow(dead_code, unused_variables)]

mod api;
mod mcp;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    // Logger init
    cubtera::utils::logger_init();

    // Launch both APIs
    let rocket = api::rocket().await;
    rocket.launch().await?;

    Ok(())
}

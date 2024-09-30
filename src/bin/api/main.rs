#![warn(clippy::all, clippy::pedantic)]
#![allow(dead_code, unused_variables)]

mod api;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    // Logger init
    cubtera::utils::logger_init();

    let rocket = api::rocket().await;
    rocket.launch().await?;

    Ok(())
}
